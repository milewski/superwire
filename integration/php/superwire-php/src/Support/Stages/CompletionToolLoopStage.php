<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Support\Stages;

use Superwire\Contracts\Agent\AgentToolCall;

final class CompletionToolLoopStage
{
    public function finalizeSuccessToolName(): string
    {
        return 'finalize_success';
    }

    public function finalizeErrorToolName(): string
    {
        return 'finalize_error';
    }

    public function completionInstruction(): string
    {
        return sprintf(
            'You must call exactly one completion tool now: `%s` when done, or `%s` with the reason when blocked.',
            $this->finalizeSuccessToolName(),
            $this->finalizeErrorToolName(),
        );
    }

    /**
     * @param array<int, AgentToolCall> $toolCalls
     */
    public function decide(array $toolCalls): CompletionToolDecision
    {
        $runtimeToolCalls = [];
        $finalizeSuccessCalls = [];
        $finalizeErrorCalls = [];

        foreach ($toolCalls as $toolCall) {

            $toolName = $toolCall->name;

            if ($toolName === $this->finalizeSuccessToolName()) {

                $finalizeSuccessCalls[] = $toolCall;

                continue;

            }

            if ($toolName === $this->finalizeErrorToolName()) {

                $finalizeErrorCalls[] = $toolCall;

                continue;

            }

            $runtimeToolCalls[] = $toolCall;

        }

        if ($runtimeToolCalls !== []) {
            return CompletionToolDecision::continue($runtimeToolCalls);
        }

        if ($finalizeSuccessCalls !== [] && $finalizeErrorCalls === []) {

            $finalizeCall = $finalizeSuccessCalls[ count($finalizeSuccessCalls) - 1 ];

            return CompletionToolDecision::success($finalizeCall->arguments[ 'answer' ] ?? null);

        }

        if ($finalizeErrorCalls !== [] && $finalizeSuccessCalls === []) {

            $finalizeCall = $finalizeErrorCalls[ count($finalizeErrorCalls) - 1 ];
            $reason = $finalizeCall->arguments[ 'reason' ] ?? 'unknown reason';

            return CompletionToolDecision::error(is_string($reason) ? $reason : 'unknown reason');

        }

        return CompletionToolDecision::continue([]);
    }

    /**
     * @param array<int, AgentToolCall> $toolCalls
     */
    public function hasMixedFinalizeAndRuntimeToolCalls(array $toolCalls): bool
    {
        $hasFinalizeToolCall = false;
        $hasRuntimeToolCall = false;

        foreach ($toolCalls as $toolCall) {

            if ($toolCall->name === $this->finalizeSuccessToolName() || $toolCall->name === $this->finalizeErrorToolName()) {

                $hasFinalizeToolCall = true;

                continue;

            }

            $hasRuntimeToolCall = true;

        }

        return $hasFinalizeToolCall && $hasRuntimeToolCall;
    }
}
