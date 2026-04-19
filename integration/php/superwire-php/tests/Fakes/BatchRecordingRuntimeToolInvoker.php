<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Tests\Fakes;

use Superwire\Contracts\Agent\AgentExecutionRequest;
use Superwire\Contracts\Agent\AgentToolCall;
use Superwire\Contracts\Agent\AgentToolResult;
use Superwire\Contracts\Contracts\RuntimeToolBatchInvokerInterface;
use Superwire\Contracts\Contracts\RuntimeToolInvokerInterface;

final class BatchRecordingRuntimeToolInvoker implements RuntimeToolBatchInvokerInterface, RuntimeToolInvokerInterface
{
    /**
     * @var list<string>
     */
    public array $invokedToolNames = [];

    public bool $batchInvoked = false;

    public int $singleInvokeCount = 0;

    /**
     * @param array<int, AgentToolCall> $toolCalls
     * @return array<int, AgentToolResult>
     */
    public function invokeBatch(AgentExecutionRequest $request, array $toolCalls): array
    {
        $this->batchInvoked = true;

        $toolResults = [];

        foreach ($toolCalls as $toolCall) {

            $this->invokedToolNames[] = $toolCall->name;
            $toolResults[] = $this->buildToolResult($toolCall);

        }

        return $toolResults;
    }

    public function invoke(AgentExecutionRequest $request, AgentToolCall $toolCall): AgentToolResult
    {
        $this->singleInvokeCount++;
        $this->invokedToolNames[] = $toolCall->name;

        return $this->buildToolResult($toolCall);
    }

    private function buildToolResult(AgentToolCall $toolCall): AgentToolResult
    {
        return new AgentToolResult(
            toolCallId: $toolCall->id,
            toolName: $toolCall->name,
            arguments: $toolCall->arguments,
            result: [
                'tool' => $toolCall->name,
                'ok' => true,
            ],
        );
    }
}
