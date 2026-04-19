<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Support;

use RuntimeException;
use Superwire\Contracts\Agent\AgentConversationMessage;
use Superwire\Contracts\Agent\AgentExecutionRequest;
use Superwire\Contracts\Agent\AgentExecutionResult;
use Superwire\Contracts\Agent\AgentToolCall;
use Superwire\Contracts\Agent\AgentToolDefinition;
use Superwire\Contracts\Agent\AgentToolResult;
use Superwire\Contracts\Agent\AgentTurnRequest;
use Superwire\Contracts\Agent\ConversationRole;
use Superwire\Contracts\Contracts\AgentDriverInterface;
use Superwire\Contracts\Contracts\AgentTurnDriverInterface;
use Superwire\Contracts\Contracts\RuntimeToolBatchInvokerInterface;
use Superwire\Contracts\Contracts\RuntimeToolInvokerInterface;
use Superwire\Contracts\Contracts\RuntimeToolMetadataProviderInterface;
use Superwire\Contracts\Contracts\RuntimeToolSchemaProviderInterface;
use Superwire\Contracts\Support\Stages\CompletionToolLoopStage;
use Superwire\Contracts\Support\Stages\WorkflowTypeValidationStage;

final class LoopAgentDriver implements AgentDriverInterface
{
    private const int MAX_ITERATIONS = 8;

    private CompletionToolLoopStage $completionToolLoopStage;

    private WorkflowTypeValidationStage $workflowTypeValidationStage;

    public function __construct(
        private readonly AgentTurnDriverInterface $turnDriver,
        private readonly ?RuntimeToolInvokerInterface $runtimeToolInvoker = null,
    )
    {
        $this->completionToolLoopStage = new CompletionToolLoopStage();
        $this->workflowTypeValidationStage = new WorkflowTypeValidationStage();
    }

    public function execute(AgentExecutionRequest $request): AgentExecutionResult
    {
        $messages = [ new AgentConversationMessage(ConversationRole::User, [ 'content' => $request->prompt ]) ];
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

            $messages[] = new AgentConversationMessage(ConversationRole::Assistant, [
                'content' => $turnResponse->text,
                'tool_calls' => $turnResponse->toolCalls,
            ]);

            $finalization = $this->completionToolLoopStage->decide($turnResponse->toolCalls);

            if ($finalization->isSuccess()) {

                $output = $this->resolvedFinalizeOutput($finalization->output, $recentRuntimeToolResults);

                $this->workflowTypeValidationStage->validate(
                    value: $output,
                    workflowType: $request->expectedOutput->workflowType,
                    context: sprintf('agent `%s` output', $request->agentName),
                );

                return new AgentExecutionResult(
                    output: $output,
                    context: $output,
                    metadata: [
                        'mode' => 'tool_loop',
                        'iterations' => $iterationIndex + 1,
                        'conversation' => $this->serializeMessages($messages),
                    ],
                );

            }

            if ($finalization->isError()) {

                if ($finalization->reason === 'unknown reason') {

                    $messages[] = new AgentConversationMessage(ConversationRole::User, [
                        'content' => sprintf(
                            "%s\nIf you call %s, you must provide a non-empty reason.",
                            $this->completionToolLoopStage->completionInstruction(),
                            $this->completionToolLoopStage->finalizeErrorToolName(),
                        ),
                    ]);

                    continue;

                }

                if ($this->shouldRetryAfterToolResponseMissingError($finalization->reason, $recentRuntimeToolResults)) {

                    $messages[] = new AgentConversationMessage(ConversationRole::User, [
                        'content' => sprintf(
                            'A runtime tool result is available in the previous tool_result message. Use that data and call %s with the final answer.',
                            $this->completionToolLoopStage->finalizeSuccessToolName(),
                        ),
                    ]);

                    $completionPhaseEnabled = true;

                    continue;

                }

                throw new RuntimeException("agent `{$request->agentName}` finalized with error: {$finalization->reason}");

            }

            if ($turnResponse->toolCalls === []) {

                if ($completionPhaseEnabled) {

                    $syntheticToolCall = $this->synthesizeCompletionToolCallFromText($request, $turnResponse->text);

                    $finalization = $this->completionToolLoopStage->decide([ $syntheticToolCall ]);

                    if ($finalization->isSuccess()) {

                        $output = $finalization->output;

                        $this->workflowTypeValidationStage->validate($output, $request->expectedOutput->workflowType, "agent `{$request->agentName}` output");

                        return new AgentExecutionResult(
                            output: $output,
                            context: $output,
                            metadata: [
                                'mode' => 'tool_loop',
                                'iterations' => $iterationIndex + 1,
                                'synthetic_completion' => true,
                                'conversation' => $this->serializeMessages($messages),
                            ],
                        );

                    }

                    $messages[] = new AgentConversationMessage(ConversationRole::User, [
                        'content' => sprintf(
                            "%s\n\nThe previous reply did not include a valid completion tool call. Reply using %s or %s only.",
                            $this->completionToolLoopStage->completionInstruction(),
                            $this->completionToolLoopStage->finalizeSuccessToolName(),
                            $this->completionToolLoopStage->finalizeErrorToolName(),
                        ),
                    ]);

                    continue;

                }

                $completionPhaseEnabled = true;
                $messages[] = new AgentConversationMessage(ConversationRole::User, [ 'content' => $this->completionToolLoopStage->completionInstruction() ]);

                continue;

            }

            $toolResults = $this->resolveToolResults($request, $turnResponse->toolCalls, $turnResponse->toolResults);

            if ($toolResults !== []) {

                $recentRuntimeToolResults = $toolResults;
                $messages[] = new AgentConversationMessage(ConversationRole::ToolResult, [ 'tool_results' => $toolResults ]);

            }

            if ($this->completionToolLoopStage->hasMixedFinalizeAndRuntimeToolCalls($turnResponse->toolCalls)) {

                $messages[] = new AgentConversationMessage(ConversationRole::User, [
                    'content' => sprintf(
                        "%s\n\nFinalize tool calls were ignored because they were submitted with runtime tool calls. Call %s or %s alone in the next turn after tool results are available.",
                        $this->completionToolLoopStage->completionInstruction(),
                        $this->completionToolLoopStage->finalizeSuccessToolName(),
                        $this->completionToolLoopStage->finalizeErrorToolName(),
                    ),
                ]);

            }

        }

        throw new RuntimeException("agent `{$request->agentName}` reached max iterations without completion tools");
    }

    /**
     * @param array<int, AgentToolResult> $recentRuntimeToolResults
     */
    private function shouldRetryAfterToolResponseMissingError(string $reason, array $recentRuntimeToolResults): bool
    {
        if ($recentRuntimeToolResults === []) {
            return false;
        }

        $normalizedReason = strtolower($reason);

        if (!str_contains($normalizedReason, 'tool response')) {
            return false;
        }

        return str_contains($normalizedReason, 'not received')
            || str_contains($normalizedReason, 'no tool response')
            || str_contains($normalizedReason, 'without actual tool payload');
    }

    private function synthesizeCompletionToolCallFromText(AgentExecutionRequest $request, string $text): AgentToolCall
    {
        if ($request->expectedOutput->isPlainString()) {

            return new AgentToolCall(
                id: 'synthetic-finalize-success',
                name: $this->completionToolLoopStage->finalizeSuccessToolName(),
                arguments: [ 'answer' => $text ],
            );

        }

        $decoded = json_decode($text, true);

        if (is_array($decoded)) {

            return new AgentToolCall(
                id: 'synthetic-finalize-success',
                name: $this->completionToolLoopStage->finalizeSuccessToolName(),
                arguments: [ 'answer' => $decoded ],
            );

        }

        return new AgentToolCall(
            id: 'synthetic-finalize-error',
            name: $this->completionToolLoopStage->finalizeErrorToolName(),
            arguments: [ 'reason' => 'model ignored completion tools and returned non-json text' ],
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
     * @param array<int, AgentToolResult> $turnToolResults
     * @return array<int, AgentToolResult>
     */
    private function resolveToolResults(AgentExecutionRequest $request, array $toolCalls, array $turnToolResults): array
    {
        $invoker = $this->runtimeToolInvoker ?? new DefaultRuntimeToolInvoker();
        $toolResults = [];
        $pendingRuntimeToolCalls = [];

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

            $pendingRuntimeToolCalls[] = $toolCall;

        }

        if ($pendingRuntimeToolCalls === []) {
            return $toolResults;
        }

        if ($invoker instanceof RuntimeToolBatchInvokerInterface) {

            $batchedToolResults = $invoker->invokeBatch($request, $pendingRuntimeToolCalls);

            return [ ...$toolResults, ...$batchedToolResults ];

        }

        foreach ($pendingRuntimeToolCalls as $pendingRuntimeToolCall) {
            $toolResults[] = $invoker->invoke($request, $pendingRuntimeToolCall);
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
                description: $this->runtimeToolDescription($toolExecution->name),
                parametersSchema: $this->runtimeToolParametersSchema($toolExecution->name),
                strict: $this->runtimeToolStrictMode($toolExecution->name),
            );

        }

        return [ ...$runtimeTools, ...$completionTools ];
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
                'required' => [ 'answer' ],
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
                    'reason' => [ 'type' => 'string' ],
                ],
                'required' => [ 'reason' ],
                'additionalProperties' => false,
            ],
        );
    }

    /**
     * @return array<string, mixed>
     */
    private function runtimeToolParametersSchema(string $toolName): array
    {
        if ($this->runtimeToolInvoker instanceof RuntimeToolSchemaProviderInterface) {

            $toolSchema = $this->runtimeToolInvoker->schemaForTool($toolName);

            if (is_array($toolSchema)) {
                return $toolSchema;
            }

        }

        if (str_contains($toolName, 'by_participant')) {

            return [
                'type' => 'object',
                'properties' => [
                    'participant_id' => [ 'type' => 'integer' ],
                ],
                'required' => [ 'participant_id' ],
                'additionalProperties' => false,
            ];

        }

        return [
            'type' => 'object',
            'properties' => [],
            'additionalProperties' => false,
        ];
    }

    private function runtimeToolDescription(string $toolName): string
    {
        if ($this->runtimeToolInvoker instanceof RuntimeToolMetadataProviderInterface) {

            $toolDescription = $this->runtimeToolInvoker->descriptionForTool($toolName);

            if (is_string($toolDescription) && $toolDescription !== '') {
                return $toolDescription;
            }

        }

        return "Execute runtime tool `{$toolName}` and use result to continue";
    }

    private function runtimeToolStrictMode(string $toolName): bool
    {
        if ($this->runtimeToolInvoker instanceof RuntimeToolMetadataProviderInterface) {

            $strictMode = $this->runtimeToolInvoker->strictSchemaForTool($toolName);

            if (is_bool($strictMode)) {
                return $strictMode;
            }

        }

        return true;
    }

    /**
     * @param array<int, AgentConversationMessage> $messages
     * @return list<array{role: string, content?: string, tool_calls?: list<array{name: string, arguments: string}>, tool_results?: list<array{tool_call_id: string, result: string}>}>
     */
    private function serializeMessages(array $messages): array
    {
        return array_map(
            function (AgentConversationMessage $message): array {

                $payload = $message->payload;
                $role = $message->role;

                if ($role === 'tool_result') {

                    return [
                        'role' => 'tool',
                        'tool_results' => array_map(
                            function (AgentToolResult $tr): array {

                                return [
                                    'tool_call_id' => $tr->toolCallId,
                                    'result' => is_string($tr->result) ? $tr->result : json_encode($tr->result),
                                ];

                            },
                            $payload[ 'tool_results' ] ?? [],
                        ),
                    ];

                }

                $content = $payload[ 'content' ] ?? '';

                if (($payload[ 'tool_calls' ] ?? []) !== []) {

                    return [
                        'role' => $role,
                        'content' => $content,
                        'tool_calls' => array_map(
                            static fn (AgentToolCall $tc): array => [
                                'name' => $tc->name,
                                'arguments' => is_array($tc->arguments) ? json_encode($tc->arguments) : $tc->arguments,
                            ],
                            $payload[ 'tool_calls' ],
                        ),
                    ];

                }

                return [ 'role' => $role, 'content' => $content ];

            },
            $messages,
        );
    }
}
