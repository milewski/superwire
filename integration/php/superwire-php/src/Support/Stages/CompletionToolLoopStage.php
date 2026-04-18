<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Support\Stages;

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
        return 'You must call exactly one completion tool now: `finalize_success` when done, or `finalize_error` with the reason when blocked.';
    }

    /**
     * @param array<int, array{name: string, arguments: array<string, mixed>}> $toolCalls
     * @return array{status: string, runtime_tool_calls: array<int, array{name: string, arguments: array<string, mixed>, id: string}>, output?: mixed, reason?: string}
     */
    public function decide(array $toolCalls): array
    {
        $runtimeToolCalls = [];
        $finalizeSuccessCalls = [];
        $finalizeErrorCalls = [];

        foreach ($toolCalls as $toolCall) {

            $toolName = $toolCall[ 'name' ];

            if ($toolName === $this->finalizeSuccessToolName()) {

                $finalizeSuccessCalls[] = $toolCall;

                continue;

            }

            if ($toolName === $this->finalizeErrorToolName()) {

                $finalizeErrorCalls[] = $toolCall;

                continue;

            }

            $runtimeToolCalls[] = [
                'name' => $toolName,
                'arguments' => $toolCall[ 'arguments' ],
                'id' => $toolCall[ 'id' ],
            ];

        }

        if ($runtimeToolCalls !== []) {

            return [
                'status' => 'continue',
                'runtime_tool_calls' => $runtimeToolCalls,
            ];

        }

        if ($finalizeSuccessCalls !== [] && $finalizeErrorCalls === []) {

            $finalizeCall = $finalizeSuccessCalls[ count($finalizeSuccessCalls) - 1 ];

            return [
                'status' => 'success',
                'runtime_tool_calls' => [],
                'output' => $finalizeCall[ 'arguments' ][ 'answer' ] ?? null,
            ];

        }

        if ($finalizeErrorCalls !== [] && $finalizeSuccessCalls === []) {

            $finalizeCall = $finalizeErrorCalls[ count($finalizeErrorCalls) - 1 ];
            $reason = $finalizeCall[ 'arguments' ][ 'reason' ] ?? 'unknown reason';

            return [
                'status' => 'error',
                'runtime_tool_calls' => [],
                'reason' => is_string($reason) ? $reason : 'unknown reason',
            ];

        }

        return [
            'status' => 'continue',
            'runtime_tool_calls' => [],
        ];
    }
}
