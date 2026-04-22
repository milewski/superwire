<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests\Feature;

use Generator;
use Prism\Prism\Testing\TextResponseFake;
use Prism\Prism\Text\Request as TextRequest;
use Prism\Prism\ValueObjects\ToolCall;
use RuntimeException;
use Superwire\Laravel\Tests\Fakes\AbstractToolLoopProvider;
use Superwire\Laravel\Tests\TestCase;
use Superwire\Laravel\Tools\Internal\FinalizeSuccessTool;
use Superwire\Laravel\Workflow;

final class LoopExecutionTest extends TestCase
{
    public function test_can_run_inputs_secrets_and_for_each_workflow(): void
    {
        $provider = $this->fakeToolLoopProvider([
            'generate a sequence of numbers from 1 to 3.' => [ 1, 2, 3 ],
            'spell out this number: 1.' => 'one',
            'spell out this number: 2.' => 'two',
            'spell out this number: 3.' => 'three',
        ]);

        $result = Workflow::fromFile(__DIR__ . '/../stubs/inputs_secrets_loop.wire')
            ->withInputs([ 'min' => 1, 'max' => 3 ])
            ->withSecrets([ 'api_key' => 'secret-token', 'model' => 'secret-model' ])
            ->run();

        $this->assertSame([ 'numbers' => [ 'one', 'two', 'three' ] ], $result->output);
        $this->assertSame([ 1, 2, 3 ], $result->agents[ 'numbers' ]->output);
        $this->assertCount(3, $result->agents[ 'counter' ]->iterations);
        $this->assertSame('secret-token', $provider->providerConfigs()[ 0 ][ 'api_key' ]);
        $this->assertSame('http://example.test/v1', $provider->providerConfigs()[ 0 ][ 'url' ]);
    }

    public function test_executes_parallel_batch_workflow(): void
    {
        $this->fakeToolLoopProvider([
            'Write customer story.' => 'customer story',
            'Write investor story.' => 'investor story',
            'Combine customer story and investor story.' => 'combined review',
        ]);

        $result = Workflow::fromFile(__DIR__ . '/../stubs/parallel_batch.wire')->run();

        $this->assertSame([ 'review' => 'combined review' ], $result->output);
        $this->assertSame('customer story', $result->agents[ 'customer_story' ]->output);
        $this->assertSame('investor story', $result->agents[ 'investor_story' ]->output);
        $this->assertSame('combined review', $result->agents[ 'review' ]->output);
    }

    public function test_bubbles_real_exception_for_forked_iterations(): void
    {
        $this->fakeToolLoopProvider([
            'generate a sequence of numbers from 1 to 3.' => [ 1, 2, 3 ],
            'spell out this number: 1.' => 'one',
            'spell out this number: 2.' => new RuntimeException('OpenAI: unhandled finish reason "unknown" (status: n/a, type: n/a)'),
            'spell out this number: 3.' => 'three',
        ]);

        $this->expectException(RuntimeException::class);
        $this->expectExceptionMessage('Execution failed for iteration agent counter: RuntimeException: OpenAI: unhandled finish reason "unknown" (status: n/a, type: n/a)');

        Workflow::fromFile(__DIR__ . '/../stubs/inputs_secrets_loop.wire')
            ->withInputs([ 'min' => 1, 'max' => 3 ])
            ->withSecrets([ 'api_key' => 'secret-token', 'model' => 'secret-model' ])
            ->run();
    }

    public function test_resets_database_connections_before_forked_batches(): void
    {
        $this->app[ 'config' ]->set('superwire.runtime.stream', false);

        $databaseManager = new ProcessBoundDatabaseManager([ 'superwire' => 3 ]);

        $this->app->instance('db', $databaseManager);

        $databaseManager->connection('superwire')->countWidgets();

        $this->useFakeProvider(new DatabaseQueryToolLoopProvider());

        $result = Workflow::fromFile(__DIR__ . '/../stubs/parallel_batch.wire')->run();

        $this->assertSame([ 'review' => 'combined review' ], $result->output);
        $this->assertSame('customer story (3)', $result->agents[ 'customer_story' ]->output);
        $this->assertSame('investor story (3)', $result->agents[ 'investor_story' ]->output);
    }
}

final class DatabaseQueryToolLoopProvider extends AbstractToolLoopProvider
{
    public function text(TextRequest $request): TextResponseFake
    {
        $this->recordTextRequest($request);

        return match ($request->prompt()) {
            'Write customer story.' => $this->finalizeSuccessResponse($request, $this->databaseResult('customer')),
            'Write investor story.' => $this->finalizeSuccessResponse($request, $this->databaseResult('investor')),
            'Combine customer story (3) and investor story (3).' => $this->finalizeSuccessResponse($request, 'combined review'),
            default => throw new RuntimeException(sprintf('No fake tool-loop response registered for prompt: %s', $request->prompt() ?? 'null')),
        };
    }

    /**
     * @return Generator<mixed>
     */
    public function stream(TextRequest $request): Generator
    {
        unset($request);

        throw new RuntimeException('Streaming is not supported by DatabaseQueryToolLoopProvider.');
    }

    private function finalizeSuccessResponse(TextRequest $request, mixed $result): TextResponseFake
    {
        $toolCall = new ToolCall(
            id: 'fake-finalize-success',
            name: FinalizeSuccessTool::name(),
            arguments: [ 'result' => $result ],
        );

        return $this->toolResponse($request, $toolCall, $this->executeToolCall($request, $toolCall));
    }

    private function databaseResult(string $story): string
    {
        $widgetCount = app('db')->connection('superwire')->countWidgets();

        return sprintf('%s story (%d)', $story, $widgetCount);
    }
}

final class ProcessBoundDatabaseManager
{
    /**
     * @var array<string, ProcessBoundDatabaseConnection>
     */
    private array $connections = [];

    /**
     * @param array<string, int> $widgetCounts
     */
    public function __construct(
        private readonly array $widgetCounts,
    )
    {
    }

    /**
     * @return array<string, ProcessBoundDatabaseConnection>
     */
    public function getConnections(): array
    {
        return $this->connections;
    }

    public function purge(string $connectionName): void
    {
        unset($this->connections[ $connectionName ]);
    }

    public function connection(string $connectionName): ProcessBoundDatabaseConnection
    {
        if (array_key_exists($connectionName, $this->connections)) {
            return $this->connections[ $connectionName ];
        }

        $widgetCount = $this->widgetCounts[ $connectionName ] ?? 0;

        $this->connections[ $connectionName ] = new ProcessBoundDatabaseConnection($widgetCount);

        return $this->connections[ $connectionName ];
    }
}

final class ProcessBoundDatabaseConnection
{
    private readonly int $createdProcessId;

    public function __construct(
        private readonly int $widgetCount,
    )
    {
        $this->createdProcessId = getmypid();
    }

    public function countWidgets(): int
    {
        $this->ensureCurrentProcess();

        return $this->widgetCount;
    }

    private function ensureCurrentProcess(): void
    {
        $currentProcessId = getmypid();

        if ($currentProcessId === $this->createdProcessId) {
            return;
        }

        throw new RuntimeException(sprintf(
            'Database connection from parent process %d reused in child process %d.',
            $this->createdProcessId,
            $currentProcessId,
        ));
    }
}
