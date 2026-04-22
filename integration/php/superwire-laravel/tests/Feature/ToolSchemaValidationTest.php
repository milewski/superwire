<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests\Feature;

use Prism\Prism\Testing\TextResponseFake;
use Prism\Prism\Text\Request as TextRequest;
use Prism\Prism\ValueObjects\Messages\ToolResultMessage;
use Prism\Prism\ValueObjects\ToolCall;
use Superwire\Laravel\Tests\Fakes\ScriptedToolLoopProvider;
use Superwire\Laravel\Tests\TestCase;
use Superwire\Laravel\Tools\AbstractTool;
use Superwire\Laravel\Tools\Internal\FinalizeSuccessTool;
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

        $registeredTool = $provider->registeredTool(BoundSchemaTool::name());

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

        $this->useFakeProvider($provider);

        return $provider;
    }
}

final class RetryingToolProvider extends ScriptedToolLoopProvider
{
    private ?string $invalidToolResultMessage = null;

    private bool $sawInvalidToolResultOnRetry = false;

    private string|array|null $validToolResult = null;

    public function __construct()
    {
        parent::__construct([
            fn (TextRequest $request, ScriptedToolLoopProvider $provider) => $this->invalidToolCallResponse($request, $provider),
            fn (TextRequest $request, ScriptedToolLoopProvider $provider) => $this->validToolCallResponse($request, $provider),
            fn (TextRequest $request, ScriptedToolLoopProvider $provider) => $this->finalizeSuccessResponse($request, $provider),
        ]);
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

    private function invalidToolCallResponse(TextRequest $request, ScriptedToolLoopProvider $provider): TextResponseFake
    {
        $toolCall = new ToolCall(
            id: 'invalid-retry-weather-tool-call',
            name: RetryWeatherTool::name(),
            arguments: [ 'country' => 'portugal' ],
        );

        $toolResult = $provider->executeToolCall($request, $toolCall);

        $this->invalidToolResultMessage = is_string($toolResult->result) ? $toolResult->result : null;

        return $provider->toolResponse($request, $toolCall, $toolResult);
    }

    private function finalizeSuccessResponse(TextRequest $request, ScriptedToolLoopProvider $provider): TextResponseFake
    {
        $toolCall = new ToolCall(
            id: 'finalize-success-tool-call',
            name: FinalizeSuccessTool::name(),
            arguments: [
                'result' => [
                    'weather' => 'sunny in lisbon',
                ],
            ],
        );

        $toolResult = $provider->executeToolCall($request, $toolCall);

        return $provider->toolResponse($request, $toolCall, $toolResult);
    }

    private function validToolCallResponse(TextRequest $request, ScriptedToolLoopProvider $provider): TextResponseFake
    {
        $this->sawInvalidToolResultOnRetry = $this->requestContainsInvalidToolResult($request);

        $toolCall = new ToolCall(
            id: 'valid-retry-weather-tool-call',
            name: RetryWeatherTool::name(),
            arguments: [
                'city' => 'lisbon',
            ],
        );

        $toolResult = $provider->executeToolCall($request, $toolCall);

        $this->validToolResult = $toolResult->result;

        return $provider->toolResponse($request, $toolCall, $toolResult);
    }

    private function requestContainsInvalidToolResult(TextRequest $request): bool
    {
        foreach ($request->messages() as $message) {

            if (!$message instanceof ToolResultMessage) {
                continue;
            }

            foreach ($message->toolResults as $toolResult) {

                if ($toolResult->toolName !== RetryWeatherTool::name()) {
                    continue;
                }

                if ($toolResult->result === $this->invalidToolResultMessage) {
                    return true;
                }

            }

        }

        return false;
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
