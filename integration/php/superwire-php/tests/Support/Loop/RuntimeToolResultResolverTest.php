<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Tests;

use PHPUnit\Framework\TestCase;
use Superwire\Contracts\Agent\AgentExecutionRequest;
use Superwire\Contracts\Agent\AgentExpectedOutput;
use Superwire\Contracts\Agent\AgentToolCall;
use Superwire\Contracts\Agent\AgentToolResult;
use Superwire\Contracts\Provider\ProviderExecution;
use Superwire\Contracts\Support\Loop\RuntimeToolResultResolver;
use Superwire\Contracts\Support\Stages\CompletionToolLoopStage;
use Superwire\Contracts\Tests\Fakes\BatchRecordingRuntimeToolInvoker;
use Superwire\Contracts\Tool\ToolExecution;

final class RuntimeToolResultResolverTest extends TestCase
{
    public function testItReusesExistingResultsAndBatchesOnlyPendingRuntimeCalls(): void
    {
        $completionToolLoopStage = new CompletionToolLoopStage();
        $runtimeToolInvoker = new BatchRecordingRuntimeToolInvoker();
        $runtimeToolResultResolver = new RuntimeToolResultResolver(
            runtimeToolInvoker: $runtimeToolInvoker,
            finalizeSuccessToolName: $completionToolLoopStage->finalizeSuccessToolName(),
            finalizeErrorToolName: $completionToolLoopStage->finalizeErrorToolName(),
        );

        $agentExecutionRequest = new AgentExecutionRequest(
            agentName: 'summary',
            provider: new ProviderExecution('openai', 'openai', []),
            model: 'gpt-4.1-mini',
            prompt: 'summarize',
            expectedOutput: new AgentExpectedOutput(
                workflowType: [ 'kind' => 'object' ],
                jsonSchema: [ 'type' => 'object' ],
            ),
            tools: [
                new ToolExecution('lookup_a', []),
                new ToolExecution('lookup_b', []),
            ],
        );

        $toolCalls = [
            new AgentToolCall('existing-call', 'lookup_a', [ 'value' => 'a' ]),
            new AgentToolCall('pending-call', 'lookup_b', [ 'value' => 'b' ]),
            new AgentToolCall('finalize-call', $completionToolLoopStage->finalizeSuccessToolName(), [ 'answer' => [ 'summary' => 'done' ] ]),
        ];

        $turnToolResults = [
            new AgentToolResult('existing-call', 'lookup_a', [ 'value' => 'a' ], [ 'tool' => 'lookup_a', 'ok' => true ]),
        ];

        $resolvedToolResults = $runtimeToolResultResolver->resolve($agentExecutionRequest, $toolCalls, $turnToolResults);

        $this->assertCount(2, $resolvedToolResults);
        $this->assertSame('lookup_a', $resolvedToolResults[ 0 ]->toolName);
        $this->assertSame('lookup_b', $resolvedToolResults[ 1 ]->toolName);
        $this->assertTrue($runtimeToolInvoker->batchInvoked);
        $this->assertSame([ 'lookup_b' ], $runtimeToolInvoker->invokedToolNames);
    }
}
