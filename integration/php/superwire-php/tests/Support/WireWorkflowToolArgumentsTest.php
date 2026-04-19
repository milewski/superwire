<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Tests;

use PHPUnit\Framework\TestCase;
use RuntimeException;
use Superwire\Contracts\Agent\AgentExecutionRequest;
use Superwire\Contracts\Agent\AgentToolCall;
use Superwire\Contracts\Agent\AgentTurnResponse;
use Superwire\Contracts\Exception\InvalidWorkflowDefinitionException;
use Superwire\Contracts\HasCompletionToolStage;
use Superwire\Contracts\Support\LoopAgentDriver;
use Superwire\Contracts\Tests\Fakes\InputSchema;
use Superwire\Contracts\Tests\Fakes\RecordingRuntimeToolInvoker;
use Superwire\Contracts\Tests\Fakes\ScriptedTurnDriver;
use Superwire\Contracts\Tests\Fakes\WireFixtureWorkflowFactory;

final class WireWorkflowToolArgumentsTest extends TestCase
{
    use HasCompletionToolStage;

    public function test_tool_invocation_bails_out_when_arguments_do_not_match_tool_schema(): void
    {
        $this->expectException(RuntimeException::class);
        $this->expectExceptionMessageMatches('/invalid schema:.*entity_id/i');

        $runtimeToolInvoker = RecordingRuntimeToolInvoker::fake(
            name: 'fetch_record_action',
            inputSchema: InputSchema::class,
        );

        $turnDriver = ScriptedTurnDriver::fake([
            new AgentTurnResponse(
                toolCalls: [
                    new AgentToolCall($runtimeToolInvoker->id(), $runtimeToolInvoker->name(), [ 'actor_id' => 99 ]),
                ],
            ),
        ]);

        $loopAgentDriver = new LoopAgentDriver($turnDriver, $runtimeToolInvoker);
        $loopAgentDriver->execute($this->make_agent_execution_request());
    }

    public function test_finalize_success_output_must_match_output_schema(): void
    {
        $this->expectException(InvalidWorkflowDefinitionException::class);
        $this->expectExceptionMessage('agent `action_summary` output output is missing required field `summary`');

        $turnDriver = ScriptedTurnDriver::fake([
            new AgentTurnResponse(
                toolCalls: [
                    new AgentToolCall(
                        id: 'finalize-1',
                        name: $this->completionStage->finalizeSuccessToolName(),
                        arguments: [
                            'answer' => [
                                'actor_id' => 99,
                            ],
                        ],
                    ),
                ],
            ),
        ]);

        $loopAgentDriver = new LoopAgentDriver($turnDriver);
        $loopAgentDriver->execute($this->make_agent_execution_request());
    }

    public function test_finalize_success_output_is_returned_when_schema_matches(): void
    {
        $turnDriver = ScriptedTurnDriver::fake([
            new AgentTurnResponse(
                toolCalls: [
                    new AgentToolCall(
                        id: 'finalize-1',
                        name: $this->completionStage->finalizeSuccessToolName(),
                        arguments: [
                            'answer' => [
                                'actor_id' => 99,
                                'summary' => 'some summary',
                            ],
                        ],
                    ),
                ],
            ),
        ]);

        $loopAgentDriver = new LoopAgentDriver($turnDriver);
        $agentExecutionResult = $loopAgentDriver->execute($this->make_agent_execution_request());

        $this->assertSame(
            expected: [ 'actor_id' => 99, 'summary' => 'some summary' ],
            actual: $agentExecutionResult->output,
        );
    }

    public function test_finalize_error_aborts_execution_and_returns_reason(): void
    {
        $runtimeToolInvoker = RecordingRuntimeToolInvoker::fake(
            name: 'fetch_record_action',
        );

        $turnDriver = ScriptedTurnDriver::fake([
            new AgentTurnResponse(
                toolCalls: [
                    new AgentToolCall(
                        id: 'finalize-error-1',
                        name: $this->completionStage->finalizeErrorToolName(),
                        arguments: [
                            'reason' => 'unable to continue due to missing upstream data',
                        ],
                    ),
                ],
            ),
            new AgentTurnResponse(
                toolCalls: [
                    new AgentToolCall($runtimeToolInvoker->id(), $runtimeToolInvoker->name(), [ 'actor_id' => 99 ]),
                ],
            ),
        ]);

        $loopAgentDriver = new LoopAgentDriver($turnDriver, $runtimeToolInvoker);

        $this->expectException(RuntimeException::class);
        $this->expectExceptionMessage('agent `action_summary` finalized with error: unable to continue due to missing upstream data');

        try {
            $loopAgentDriver->execute($this->make_agent_execution_request());
        } finally {
            $this->assertSame(expected: 1, actual: count($turnDriver->requests));
            $this->assertSame(expected: 0, actual: count($runtimeToolInvoker->invocations));
        }
    }

    private function make_agent_execution_request(): AgentExecutionRequest
    {
        return WireFixtureWorkflowFactory::makeAgentExecutionRequest(
            fixturePath: __DIR__ . '/../Stubs/Wire/tool_argument_passthrough.wire',
            agentName: 'action_summary',
            input: [
                'workspace_id' => 10,
                'record_id' => 22,
                'actor_id' => 99,
            ],
        );
    }
}
