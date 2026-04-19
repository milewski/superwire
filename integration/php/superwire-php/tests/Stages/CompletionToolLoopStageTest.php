<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Tests\Stages;

use PHPUnit\Framework\TestCase;
use Superwire\Contracts\Agent\AgentToolCall;
use Superwire\Contracts\HasCompletionToolStage;
use Superwire\Contracts\Support\Stages\CompletionToolDecisionStatus;

final class CompletionToolLoopStageTest extends TestCase
{
    use HasCompletionToolStage;

    public function test_it_prefers_runtime_tool_calls_over_finalize_calls_in_same_turn(): void
    {
        $decision = $this->completionStage->decide([
            new AgentToolCall(id: '1', name: $this->completionStage->finalizeSuccessToolName(), arguments: [ 'answer' => 'done' ]),
            new AgentToolCall(id: '2', name: 'search_docs', arguments: [ 'query' => 'x' ]),
        ]);

        $this->assertSame(CompletionToolDecisionStatus::Continue, $decision->status);
        $this->assertCount(1, $decision->runtimeToolCalls);
        $this->assertSame('search_docs', $decision->runtimeToolCalls[ 0 ]->name);
    }

    public function test_it_extracts_finalize_success_payload(): void
    {
        $decision = $this->completionStage->decide([
            new AgentToolCall(id: '3', name: $this->completionStage->finalizeSuccessToolName(), arguments: [ 'answer' => [ 'summary' => 'ok' ] ]),
        ]);

        $this->assertSame(CompletionToolDecisionStatus::Success, $decision->status);
        $this->assertSame([ 'summary' => 'ok' ], $decision->output);
    }
}
