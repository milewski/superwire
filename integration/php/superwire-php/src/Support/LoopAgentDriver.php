<?php

declare(strict_types=1);

namespace Superwire\Contracts\Support;

use RuntimeException;
use Superwire\Contracts\AgentConversationMessage;
use Superwire\Contracts\AgentExecutionRequest;
use Superwire\Contracts\AgentExecutionResult;
use Superwire\Contracts\AgentToolCall;
use Superwire\Contracts\AgentToolDefinition;
use Superwire\Contracts\AgentToolResult;
use Superwire\Contracts\AgentTurnRequest;
use Superwire\Contracts\Contracts\AgentDriverInterface;
use Superwire\Contracts\Contracts\AgentTurnDriverInterface;
use Superwire\Contracts\Contracts\RuntimeToolInvokerInterface;
use Superwire\Contracts\Support\Stages\CompletionToolLoopStage;
use Superwire\Contracts\Support\Stages\WorkflowTypeValidationStage;

final class LoopAgentDriver implements AgentDriverInterface
{
    private const MAX_ITERATIONS = 8;

    private CompletionToolLoopStage $completionToolLoopStage;

    private WorkflowTypeValidationStage $workflowTypeValidationStage;

    public function __construct(
        private readonly AgentTurnDriverInterface $turnDriver,
        private readonly ?RuntimeToolInvokerInterface $runtimeToolInvoker = null,
    ) {
        $this->completionToolLoopStage = new CompletionToolLoopStage();
        $this->workflowTypeValidationStage = new WorkflowTypeValidationStage();
    }

    public function execute(AgentExecutionRequest $request): AgentExecutionResult
    {
        $messages = [new AgentConversationMessage('user', ['content' => $request->prompt])];
        $completionPhaseEnabled = false;
        $recentRuntimeToolResults = [];

        for ($iterationIndex = 0; $iterationIndex < self::MAX_ITERATIONS; $iterationIndex++) {
            $tools = $this->buildTurnTools($request, $completionPhaseEnabled);
            $turnResponse = $this->turnDriver->generateTurn(new AgentTurnRequest(
                provider: $request->provider->provider,
                model: $request->model,
                providerConfig: $request->provider->config,
                messages: $messages,
                tools: $tools,
                requireToolCall: $completionPhaseEnabled,
            ));

            $messages[] = new AgentConversationMessage('assistant', [
                'content' => $turnResponse->text,
                'tool_calls' => $turnResponse->toolCalls,
            ]);

            $finalization = $this->completionToolLoopStage->decide($this->toolCallsForDecision($turnResponse->toolCalls));

            if ($finalization['status'] === 'success') {
                $output = $this->resolvedFinalizeOutput($finalization['output'] ?? null, $recentRuntimeToolResults);

                $this->workflowTypeValidationStage->validate($output, $request->expectedOutput->workflowType, "agent `{$request->agentName}` output");

                return new AgentExecutionResult(
                    output: $output,
                    context: $output,
                    metadata: [
                        'mode' => 'tool_loop',
                        'iterations' => $iterationIndex + 1,
                    ],
                );
            }

            if ($finalization['status'] === 'error') {
                if (($finalization['reason'] ?? null) === 'unknown reason') {
                    $messages[] = new AgentConversationMessage('user', [
                        'content' => $this->completionToolLoopStage->completionInstruction() . "\nIf you call finalize_error, you must provide a non-empty reason.",
                    ]);

                    continue;
                }

                throw new RuntimeException("agent `{$request->agentName}` finalized with error: {$finalization['reason']}");
            }

            if ($turnResponse->toolCalls === []) {
                if ($completionPhaseEnabled) {
                    $syntheticToolCall = $this->synthesizeCompletionToolCallFromText($request, $turnResponse->text);

                    $finalization = $this->completionToolLoopStage->decide([[
                        'id' => $syntheticToolCall->id,
                        'name' => $syntheticToolCall->name,
                        'arguments' => $syntheticToolCall->arguments,
                    ]]);

                    if ($finalization['status'] === 'success') {
                        $output = $finalization['output'];

                        $this->workflowTypeValidationStage->validate($output, $request->expectedOutput->workflowType, "agent `{$request->agentName}` output");

                        return new AgentExecutionResult(
                            output: $output,
                            context: $output,
                            metadata: [
                                'mode' => 'tool_loop',
                                'iterations' => $iterationIndex + 1,
                                'synthetic_completion' => true,
                            ],
                        );
                    }

                    $messages[] = new AgentConversationMessage('user', [
                        'content' => $this->completionToolLoopStage->completionInstruction() . "\nThe previous reply did not include a valid completion tool call. Reply using finalize_success or finalize_error only.",
                    ]);

                    continue;
                }

                $completionPhaseEnabled = true;
                $messages[] = new AgentConversationMessage('user', ['content' => $this->completionToolLoopStage->completionInstruction()]);

                continue;
            }

            $toolResults = $this->resolveToolResults($request, $turnResponse->toolCalls, $turnResponse->toolResults);

            if ($toolResults !== []) {
                $recentRuntimeToolResults = $toolResults;
                $messages[] = new AgentConversationMessage('tool_result', ['tool_results' => $toolResults]);
            }
        }

        throw new RuntimeException("agent `{$request->agentName}` reached max iterations without completion tools");
    }

    private function synthesizeCompletionToolCallFromText(AgentExecutionRequest $request, string $text): AgentToolCall
    {
        if ($request->expectedOutput->isPlainString()) {
            return new AgentToolCall(
                id: 'synthetic-finalize-success',
                name: $this->completionToolLoopStage->finalizeSuccessToolName(),
                arguments: ['answer' => $text],
            );
        }

        $decoded = json_decode($text, true);

        if (is_array($decoded)) {
            return new AgentToolCall(
                id: 'synthetic-finalize-success',
                name: $this->completionToolLoopStage->finalizeSuccessToolName(),
                arguments: ['answer' => $decoded],
            );
        }

        return new AgentToolCall(
            id: 'synthetic-finalize-error',
            name: $this->completionToolLoopStage->finalizeErrorToolName(),
            arguments: ['reason' => 'model ignored completion tools and returned non-json text'],
        );
    }

    /**
     * @param array<int, AgentToolResult> $recentRuntimeToolResults
     */
    private function resolvedFinalizeOutput(mixed $finalizeOutput, array $recentRuntimeToolResults): mixed
    {
        if ($finalizeOutput !== null) {
            return $finalizeOutput;
        }

        foreach (array_reverse($recentRuntimeToolResults) as $toolResult) {
            if (is_array($toolResult->result)) {
                return $toolResult->result;
            }

            if (is_string($toolResult->result)) {
                $decodedResult = json_decode($toolResult->result, true);

                if (is_array($decodedResult)) {
                    return $decodedResult;
                }
            }
        }

        return null;
    }

    /**
     * @param array<int, AgentToolCall> $toolCalls
     * @return array<int, array{name: string, arguments: array<string, mixed>, id: string}>
     */
    private function toolCallsForDecision(array $toolCalls): array
    {
        return array_map(
            static fn (AgentToolCall $toolCall): array => [
                'id' => $toolCall->id,
                'name' => $toolCall->name,
                'arguments' => $toolCall->arguments,
            ],
            $toolCalls,
        );
    }

    /**
     * @param array<int, AgentToolCall> $toolCalls
     * @param array<int, AgentToolResult> $turnToolResults
     * @return array<int, AgentToolResult>
     */
    private function resolveToolResults(AgentExecutionRequest $request, array $toolCalls, array $turnToolResults): array
    {
        $toolResults = [];

        foreach ($toolCalls as $toolCall) {
            if (
                $toolCall->name === $this->completionToolLoopStage->finalizeSuccessToolName()
                || $toolCall->name === $this->completionToolLoopStage->finalizeErrorToolName()
            ) {
                continue;
            }

            $existing = $this->existingToolResult($turnToolResults, $toolCall->id);

            if ($existing !== null) {
                $toolResults[] = $existing;

                continue;
            }

            $invoker = $this->runtimeToolInvoker ?? new DefaultRuntimeToolInvoker();
            $toolResults[] = $invoker->invoke($request, $toolCall);
        }

        return $toolResults;
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

    /**
     * @return array<int, AgentToolDefinition>
     */
    private function buildTurnTools(AgentExecutionRequest $request, bool $completionPhaseEnabled): array
    {
        $completionTools = [
            $this->finalizeSuccessTool($request),
            $this->finalizeErrorTool(),
        ];

        if ($completionPhaseEnabled) {
            return $completionTools;
        }

        $runtimeTools = [];

        foreach ($request->tools as $toolExecution) {
            $runtimeTools[] = new AgentToolDefinition(
                name: $toolExecution->name,
                description: "Execute runtime tool `{$toolExecution->name}` and use result to continue",
                parametersSchema: $this->runtimeToolParametersSchema($toolExecution->name),
            );
        }

        return [...$runtimeTools, ...$completionTools];
    }

    private function finalizeSuccessTool(AgentExecutionRequest $request): AgentToolDefinition
    {
        return new AgentToolDefinition(
            name: $this->completionToolLoopStage->finalizeSuccessToolName(),
            description: 'Call this only when the task is completed successfully and provide final answer',
            parametersSchema: [
                'type' => 'object',
                'properties' => [
                    'answer' => $request->expectedOutput->jsonSchema,
                ],
                'required' => ['answer'],
                'additionalProperties' => false,
            ],
        );
    }

    private function finalizeErrorTool(): AgentToolDefinition
    {
        return new AgentToolDefinition(
            name: $this->completionToolLoopStage->finalizeErrorToolName(),
            description: 'Call this only when task cannot be completed and provide reason',
            parametersSchema: [
                'type' => 'object',
                'properties' => [
                    'reason' => ['type' => 'string'],
                ],
                'required' => ['reason'],
                'additionalProperties' => false,
            ],
        );
    }

    /**
     * @return array<string, mixed>
     */
    private function runtimeToolParametersSchema(string $toolName): array
    {
        if (str_contains($toolName, 'by_participant')) {
            return [
                'type' => 'object',
                'properties' => [
                    'participant_id' => ['type' => 'integer'],
                ],
                'required' => ['participant_id'],
                'additionalProperties' => true,
            ];
        }

        return [
            'type' => 'object',
            'additionalProperties' => true,
        ];
    }
}
