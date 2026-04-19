<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Tests;

use PHPUnit\Framework\TestCase;
use RuntimeException;
use Superwire\Contracts\Agent\AgentConversationMessage;
use Superwire\Contracts\Agent\AgentExecutionRequest;
use Superwire\Contracts\Agent\AgentExpectedOutput;
use Superwire\Contracts\Agent\AgentToolCall;
use Superwire\Contracts\Agent\AgentTurnResponse;
use Superwire\Contracts\HasCompletionToolStage;
use Superwire\Contracts\Provider\ProviderExecution;
use Superwire\Contracts\Support\LoopAgentDriver;
use Superwire\Contracts\Tests\Fakes\BatchRecordingRuntimeToolInvoker;
use Superwire\Contracts\Tests\Fakes\FakeTurnDriver;
use Superwire\Contracts\Tool\ToolExecution;
use Swaggest\JsonSchema\Schema;

final class LoopAgentDriverTest extends TestCase
{
    use HasCompletionToolStage;

    public function test_it_forces_completion_tool_when_model_returns_only_text(): void
    {
        $turnDriver = new FakeTurnDriver([
            new AgentTurnResponse([], 'plain text with no tool calls'),
            new AgentTurnResponse([
                new AgentToolCall('1', $this->completionStage->finalizeSuccessToolName(), [ 'answer' => [ 'summary' => 'ok' ] ]),
            ]),
        ]);

        $driver = new LoopAgentDriver($turnDriver);

        $result = $driver->execute(new AgentExecutionRequest(
            agentName: 'summary',
            provider: new ProviderExecution('openai', 'openai', []),
            model: 'gpt-4.1-mini',
            prompt: 'do work',
            expectedOutput: new AgentExpectedOutput(
                workflowType: [ 'kind' => 'object', 'fields' => [ 'summary' => [ 'kind' => 'string' ] ] ],
                jsonSchema: Schema::object(),
            ),
        ));

        $this->assertSame([ 'summary' => 'ok' ], $result->output);
        $this->assertCount(2, $turnDriver->requests);
        $this->assertFalse($turnDriver->requests[ 0 ]->requireToolCall);
        $this->assertTrue($turnDriver->requests[ 1 ]->requireToolCall);
    }

    public function test_it_synthesizes_finalize_success_from_text_in_forced_completion_mode(): void
    {
        $turnDriver = new FakeTurnDriver([
            new AgentTurnResponse([], 'plain text with no tool calls'),
            new AgentTurnResponse([], 'final plain text'),
        ]);

        $driver = new LoopAgentDriver($turnDriver);

        $result = $driver->execute(new AgentExecutionRequest(
            agentName: 'summary',
            provider: new ProviderExecution('openai', 'openai', []),
            model: 'gpt-4.1-mini',
            prompt: 'do work',
            expectedOutput: new AgentExpectedOutput(
                workflowType: [ 'kind' => 'string' ],
                jsonSchema: Schema::string(),
            ),
        ));

        $this->assertSame('final plain text', $result->output);
        $this->assertTrue((bool) ($result->metadata[ 'synthetic_completion' ] ?? false));
    }

    public function test_it_throws_when_max_iterations_is_reached_without_valid_completion(): void
    {
        $invalidToolCallResponse = new AgentTurnResponse([
            new AgentToolCall('success', $this->completionStage->finalizeSuccessToolName(), [ 'answer' => [ 'summary' => 'ok' ] ]),
            new AgentToolCall('error', $this->completionStage->finalizeErrorToolName(), [ 'reason' => 'conflicting completion' ]),
        ]);

        $turnDriver = new FakeTurnDriver([
            $invalidToolCallResponse,
            $invalidToolCallResponse,
            $invalidToolCallResponse,
            $invalidToolCallResponse,
            $invalidToolCallResponse,
            $invalidToolCallResponse,
            $invalidToolCallResponse,
            $invalidToolCallResponse,
        ]);

        $driver = new LoopAgentDriver($turnDriver);

        $this->expectException(RuntimeException::class);
        $this->expectExceptionMessage('agent `summary` reached max iterations without completion tools');

        try {

            $driver->execute(new AgentExecutionRequest(
                agentName: 'summary',
                provider: new ProviderExecution('openai', 'openai', []),
                model: 'gpt-4.1-mini',
                prompt: 'do work',
                expectedOutput: new AgentExpectedOutput(
                    workflowType: [ 'kind' => 'object', 'fields' => [ 'summary' => [ 'kind' => 'string' ] ] ],
                    jsonSchema: Schema::object(),
                ),
            ));

        } finally {
            $this->assertCount(8, $turnDriver->requests);
        }
    }

    public function test_it_executes_multiple_runtime_tool_calls_in_a_single_batched_invocation(): void
    {
        $turnDriver = new FakeTurnDriver([
            new AgentTurnResponse([
                new AgentToolCall('tool-a', 'lookup_a', [ 'value' => 'a' ]),
                new AgentToolCall('tool-b', 'lookup_b', [ 'value' => 'b' ]),
            ]),
            new AgentTurnResponse([
                new AgentToolCall('finalize', $this->completionStage->finalizeSuccessToolName(), [ 'answer' => [ 'summary' => 'ok' ] ]),
            ]),
        ]);

        $runtimeToolInvoker = new BatchRecordingRuntimeToolInvoker();
        $driver = new LoopAgentDriver($turnDriver, $runtimeToolInvoker);

        $result = $driver->execute(new AgentExecutionRequest(
            agentName: 'summary',
            provider: new ProviderExecution('openai', 'openai', []),
            model: 'gpt-4.1-mini',
            prompt: 'do work',
            expectedOutput: new AgentExpectedOutput(
                workflowType: [ 'kind' => 'object', 'fields' => [ 'summary' => [ 'kind' => 'string' ] ] ],
                jsonSchema: Schema::object(),
            ),
            tools: [
                new ToolExecution('lookup_a', []),
                new ToolExecution('lookup_b', []),
            ],
        ));

        $this->assertSame([ 'summary' => 'ok' ], $result->output);
        $this->assertTrue($runtimeToolInvoker->batchInvoked);
        $this->assertSame(0, $runtimeToolInvoker->singleInvokeCount);
        $this->assertSame([ 'lookup_a', 'lookup_b' ], $runtimeToolInvoker->invokedToolNames);
    }

    public function test_it_ignores_finalize_when_mixed_with_runtime_tool_calls_and_prompts_retry(): void
    {
        $turnDriver = new FakeTurnDriver([
            new AgentTurnResponse([
                new AgentToolCall('tool-a', 'lookup_a', [ 'value' => 'a' ]),
                new AgentToolCall('early-finalize', $this->completionStage->finalizeSuccessToolName(), [ 'answer' => [ 'summary' => 'too early' ] ]),
            ]),
            new AgentTurnResponse([
                new AgentToolCall('finalize', $this->completionStage->finalizeSuccessToolName(), [ 'answer' => [ 'summary' => 'ok' ] ]),
            ]),
        ]);

        $runtimeToolInvoker = new BatchRecordingRuntimeToolInvoker();
        $driver = new LoopAgentDriver($turnDriver, $runtimeToolInvoker);

        $result = $driver->execute(new AgentExecutionRequest(
            agentName: 'summary',
            provider: new ProviderExecution('openai', 'openai', []),
            model: 'gpt-4.1-mini',
            prompt: 'do work',
            expectedOutput: new AgentExpectedOutput(
                workflowType: [ 'kind' => 'object', 'fields' => [ 'summary' => [ 'kind' => 'string' ] ] ],
                jsonSchema: Schema::object(),
            ),
            tools: [
                new ToolExecution('lookup_a', []),
            ],
        ));

        $secondTurnMessages = array_map(
            callback: fn (AgentConversationMessage $message): string => (string) ($message->payload[ 'content' ] ?? ''),
            array: $turnDriver->requests[ 1 ]->messages,
        );

        $combinedSecondTurnContent = implode("\n", $secondTurnMessages);

        $this->assertSame([ 'summary' => 'ok' ], $result->output);
        $this->assertCount(2, $turnDriver->requests);
        $this->assertStringContainsString('Finalize tool calls were ignored', $combinedSecondTurnContent);
    }

    public function test_it_ignores_finalize_error_when_mixed_with_runtime_tool_calls_and_prompts_retry(): void
    {
        $turnDriver = new FakeTurnDriver([
            new AgentTurnResponse([
                new AgentToolCall('tool-a', 'lookup_a', [ 'value' => 'a' ]),
                new AgentToolCall('early-finalize-error', $this->completionStage->finalizeErrorToolName(), [ 'reason' => 'too early' ]),
            ]),
            new AgentTurnResponse([
                new AgentToolCall('finalize', $this->completionStage->finalizeSuccessToolName(), [ 'answer' => [ 'summary' => 'ok' ] ]),
            ]),
        ]);

        $runtimeToolInvoker = new BatchRecordingRuntimeToolInvoker();
        $driver = new LoopAgentDriver($turnDriver, $runtimeToolInvoker);

        $result = $driver->execute(new AgentExecutionRequest(
            agentName: 'summary',
            provider: new ProviderExecution('openai', 'openai', []),
            model: 'gpt-4.1-mini',
            prompt: 'do work',
            expectedOutput: new AgentExpectedOutput(
                workflowType: [ 'kind' => 'object', 'fields' => [ 'summary' => [ 'kind' => 'string' ] ] ],
                jsonSchema: Schema::object(),
            ),
            tools: [
                new ToolExecution('lookup_a', []),
            ],
        ));

        $secondTurnMessages = array_map(
            callback: fn (AgentConversationMessage $message): string => (string) ($message->payload[ 'content' ] ?? ''),
            array: $turnDriver->requests[ 1 ]->messages,
        );

        $combinedSecondTurnContent = implode("\n", $secondTurnMessages);

        $this->assertSame([ 'summary' => 'ok' ], $result->output);
        $this->assertCount(2, $turnDriver->requests);
        $this->assertStringContainsString('Finalize tool calls were ignored', $combinedSecondTurnContent);
    }
}
