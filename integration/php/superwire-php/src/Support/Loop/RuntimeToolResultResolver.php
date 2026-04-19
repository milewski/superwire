<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Support\Loop;

use Superwire\Contracts\Agent\AgentExecutionRequest;
use Superwire\Contracts\Agent\AgentToolCall;
use Superwire\Contracts\Agent\AgentToolResult;
use Superwire\Contracts\Contracts\RuntimeToolBatchInvokerInterface;
use Superwire\Contracts\Contracts\RuntimeToolInvokerInterface;
use Superwire\Contracts\Support\DefaultRuntimeToolInvoker;

final readonly class RuntimeToolResultResolver
{
    public function __construct(
        private ?RuntimeToolInvokerInterface $runtimeToolInvoker,
        private string $finalizeSuccessToolName,
        private string $finalizeErrorToolName,
    ) {
    }

    /**
     * @param array<int, AgentToolCall> $toolCalls
     * @param array<int, AgentToolResult> $turnToolResults
     * @return array<int, AgentToolResult>
     */
    public function resolve(AgentExecutionRequest $request, array $toolCalls, array $turnToolResults): array
    {
        $runtimeToolInvoker = $this->runtimeToolInvoker ?? new DefaultRuntimeToolInvoker();
        $resolvedToolResults = [];
        $pendingRuntimeToolCalls = [];

        foreach ($toolCalls as $toolCall) {

            if ($this->isCompletionTool($toolCall)) {
                continue;
            }

            $existingToolResult = $this->existingToolResult($turnToolResults, $toolCall->id);

            if ($existingToolResult !== null) {

                $resolvedToolResults[] = $existingToolResult;

                continue;

            }

            $pendingRuntimeToolCalls[] = $toolCall;

        }

        if ($pendingRuntimeToolCalls === []) {
            return $resolvedToolResults;
        }

        if ($runtimeToolInvoker instanceof RuntimeToolBatchInvokerInterface) {

            $batchedToolResults = $runtimeToolInvoker->invokeBatch($request, $pendingRuntimeToolCalls);

            return [ ...$resolvedToolResults, ...$batchedToolResults ];

        }

        foreach ($pendingRuntimeToolCalls as $pendingRuntimeToolCall) {
            $resolvedToolResults[] = $runtimeToolInvoker->invoke($request, $pendingRuntimeToolCall);
        }

        return $resolvedToolResults;
    }

    private function isCompletionTool(AgentToolCall $toolCall): bool
    {
        return $toolCall->name === $this->finalizeSuccessToolName
            || $toolCall->name === $this->finalizeErrorToolName;
    }

    /**
     * @param array<int, AgentToolResult> $toolResults
     */
    private function existingToolResult(array $toolResults, string $toolCallId): ?AgentToolResult
    {
        foreach ($toolResults as $toolResult) {

            if ($toolResult->toolCallId === $toolCallId) {
                return $toolResult;
            }

        }

        return null;
    }
}
