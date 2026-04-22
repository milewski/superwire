<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests\Feature;

use Generator;
use Illuminate\Support\Collection;
use Prism\Prism\Enums\FinishReason;
use Prism\Prism\Enums\Provider as EnumProvider;
use Prism\Prism\PrismManager;
use Prism\Prism\Providers\Provider;
use Prism\Prism\Streaming\Events\StreamEvent;
use Prism\Prism\Testing\TextResponseFake;
use Prism\Prism\Text\Request as TextRequest;
use Prism\Prism\Text\Step;
use Prism\Prism\Tool;
use Prism\Prism\ValueObjects\Messages\AssistantMessage;
use Prism\Prism\ValueObjects\Messages\ToolResultMessage;
use Prism\Prism\ValueObjects\Meta;
use Prism\Prism\ValueObjects\ToolCall;
use Prism\Prism\ValueObjects\ToolError;
use Prism\Prism\ValueObjects\ToolResult;
use Prism\Prism\ValueObjects\Usage;
use RuntimeException;
use Superwire\Laravel\Tests\TestCase;
use Superwire\Laravel\Tools\AbstractTool;
use Superwire\Laravel\Tools\WorkflowBoundInput;
use Superwire\Laravel\Tools\WorkflowToolInput;
use Superwire\Laravel\Workflow;

final class ToolSchemaValidationTest extends TestCase
{
    public function test_tool_registers_only_agent_input_schema_and_returns_failed_tool_result_for_invalid_arguments(): void
    {
        config()->set('superwire.runtime.stream', false);

        RetryWeatherTool::reset();

        $provider = $this->fakeRetryingToolProvider();

        $result = Workflow::fromFile(__DIR__ . '/../stubs/tool_schema_retry.wire')
            ->withTools([ new BoundSchemaTool(), new RetryWeatherTool() ])
            ->run();

        $registeredTool = $provider->registeredTool('bound_schema_tool');

        $this->assertNotNull($registeredTool);
        $this->assertSame([ 'city' ], $registeredTool->requiredParameters());
        $this->assertSame('string', $registeredTool->parametersAsArray()[ 'city' ][ 'type' ] ?? null);
        $this->assertArrayNotHasKey('tenant_id', $registeredTool->parametersAsArray());

        $this->assertTrue($provider->sawInvalidToolResultOnRetry());
        $this->assertStringContainsString('Parameter validation error', (string) $provider->invalidToolResultMessage());
        $this->assertStringContainsString('city', (string) $provider->invalidToolResultMessage());
        $this->assertSame('{"weather":"sunny in lisbon"}', $provider->validToolResult());
        $this->assertSame(1, RetryWeatherTool::handleCallCount());
        $this->assertSame([ 'weather' => 'sunny in lisbon' ], $result->output);
    }

    private function fakeRetryingToolProvider(): RetryingToolProvider
    {
        $provider = new RetryingToolProvider();

        app()->instance(PrismManager::class, new class (app(), $provider) extends PrismManager {
            public function __construct($app, private readonly RetryingToolProvider $provider)
            {
                parent::__construct($app);
            }

            public function resolve(EnumProvider |string $name, array $providerConfig = []): Provider
            {
                return $this->provider;
            }
        });

        return $provider;
    }
}

final class RetryingToolProvider extends Provider
{
    /**
     * @var array<int, TextRequest>
     */
    private array $requests = [];

    private ?string $invalidToolResultMessage = null;

    private bool $sawInvalidToolResultOnRetry = false;

    private string|array|null $validToolResult = null;

    public function text(TextRequest $request): TextResponseFake
    {
        $this->requests[] = $request;

        return match (count($this->requests)) {
            1 => $this->invalidToolCallResponse($request),
            2 => $this->validToolCallResponse($request),
            3 => $this->finalizeSuccessResponse($request),
            default => throw new RuntimeException('Unexpected extra text request in retrying tool provider.'),
        };
    }

    /**
     * @return Generator<StreamEvent>
     */
    public function stream(TextRequest $request): Generator
    {
        throw new RuntimeException('Streaming is disabled for this test provider.');
    }

    public function registeredTool(string $toolName): ?Tool
    {
        $firstRequest = $this->requests[ 0 ] ?? null;

        if (!$firstRequest instanceof TextRequest) {
            return null;
        }

        foreach ($firstRequest->tools() as $tool) {

            if ($tool->name() === $toolName) {
                return $tool;
            }

        }

        return null;
    }

    public function invalidToolResultMessage(): ?string
    {
        return $this->invalidToolResultMessage;
    }

    public function sawInvalidToolResultOnRetry(): bool
    {
        return $this->sawInvalidToolResultOnRetry;
    }

    public function validToolResult(): string|array|null
    {
        return $this->validToolResult;
    }

    private function invalidToolCallResponse(TextRequest $request): TextResponseFake
    {
        $toolCall = new ToolCall(
            id: 'invalid-retry-weather-tool-call',
            name: 'retry_weather_tool',
            arguments: [ 'country' => 'portugal' ],
        );

        [ 'toolResult' => $toolResult ] = $this->executeToolCall($request, $toolCall);

        $this->invalidToolResultMessage = is_string($toolResult->result) ? $toolResult->result : null;

        return $this->toolResponse($request, $toolCall, $toolResult);
    }

    private function finalizeSuccessResponse(TextRequest $request): TextResponseFake
    {
        $toolCall = new ToolCall(
            id: 'finalize-success-tool-call',
            name: 'finalize_success',
            arguments: [
                'result' => [
                    'weather' => 'sunny in lisbon',
                ],
            ],
        );

        [ 'toolResult' => $toolResult ] = $this->executeToolCall($request, $toolCall);

        return $this->toolResponse($request, $toolCall, $toolResult);
    }

    private function validToolCallResponse(TextRequest $request): TextResponseFake
    {
        $this->sawInvalidToolResultOnRetry = $this->requestContainsInvalidToolResult($request);

        $toolCall = new ToolCall(
            id: 'valid-retry-weather-tool-call',
            name: 'retry_weather_tool',
            arguments: [
                'city' => 'lisbon',
            ],
        );

        [ 'toolResult' => $toolResult ] = $this->executeToolCall($request, $toolCall);

        $this->validToolResult = $toolResult->result;

        return $this->toolResponse($request, $toolCall, $toolResult);
    }

    /**
     * @return array{toolResult: ToolResult}
     */
    private function executeToolCall(TextRequest $request, ToolCall $toolCall): array
    {
        $tool = $this->resolveTool($toolCall->name, $request->tools());
        $output = $tool->handle(...$toolCall->arguments());

        $toolResult = new ToolResult(
            toolCallId: $toolCall->id,
            toolName: $toolCall->name,
            args: $toolCall->arguments(),
            result: $output instanceof ToolError ? $output->message : $output,
        );

        return [ 'toolResult' => $toolResult ];
    }

    private function requestContainsInvalidToolResult(TextRequest $request): bool
    {
        foreach ($request->messages() as $message) {

            if (!$message instanceof ToolResultMessage) {
                continue;
            }

            foreach ($message->toolResults as $toolResult) {

                if ($toolResult->toolName !== 'retry_weather_tool') {
                    continue;
                }

                if ($toolResult->result === $this->invalidToolResultMessage) {
                    return true;
                }

            }

        }

        return false;
    }

    private function toolResponse(TextRequest $request, ToolCall $toolCall, ToolResult $toolResult): TextResponseFake
    {
        $assistantMessage = new AssistantMessage(content: '', toolCalls: [ $toolCall ]);
        $toolResultMessage = new ToolResultMessage([ $toolResult ]);

        return TextResponseFake::make()
            ->withFinishReason(FinishReason::ToolCalls)
            ->withToolCalls([ $toolCall ])
            ->withToolResults([ $toolResult ])
            ->withUsage(new Usage(0, 0))
            ->withMeta(new Meta('fake', 'fake'))
            ->withSteps(collect([
                new Step(
                    text: '',
                    finishReason: FinishReason::ToolCalls,
                    toolCalls: [ $toolCall ],
                    toolResults: [ $toolResult ],
                    providerToolCalls: [],
                    usage: new Usage(0, 0),
                    meta: new Meta('fake', 'fake'),
                    messages: $request->messages(),
                    systemPrompts: $request->systemPrompts(),
                ),
            ]))
            ->withMessages(new Collection([
                ...$request->messages(),
                $assistantMessage,
                $toolResultMessage,
            ]));
    }

    /**
     * @param array<int, Tool> $tools
     */
    private function resolveTool(string $name, array $tools): Tool
    {
        foreach ($tools as $tool) {

            if ($tool->name() === $name) {
                return $tool;
            }

        }

        throw new RuntimeException(sprintf('Tool not found in retrying tool provider: %s', $name));
    }
}

final class RetryWeatherTool extends AbstractTool
{
    private static int $handleCallCount = 0;

    public static function reset(): void
    {
        self::$handleCallCount = 0;
    }

    public static function handleCallCount(): int
    {
        return self::$handleCallCount;
    }

    protected function handle(RetryWeatherToolInput $agentInput): array
    {
        self::$handleCallCount++;

        return [
            'weather' => sprintf('sunny in %s', $agentInput->city),
        ];
    }
}

final class BoundSchemaTool extends AbstractTool
{
    protected function handle(BoundSchemaToolInput $agentInput, BoundSchemaToolBoundInput $boundInput): array
    {
        return [
            'value' => sprintf('%s-%s', $agentInput->city, $boundInput->tenant_id),
        ];
    }
}

final class BoundSchemaToolInput extends WorkflowToolInput
{
    public function __construct(
        public string $city,
    )
    {
    }
}

final class BoundSchemaToolBoundInput extends WorkflowBoundInput
{
    public function __construct(
        public string $tenant_id,
    )
    {
    }
}

final class RetryWeatherToolInput extends WorkflowToolInput
{
    public function __construct(
        public string $city,
    )
    {
    }
}
