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
use Superwire\Contracts\Contracts\RuntimeToolInvokerInterface;
use Superwire\Contracts\Support\Loop\CompletionToolDefinitionFactory;
use Superwire\Contracts\Support\Loop\ConversationMessageSerializer;
use Superwire\Contracts\Support\Loop\RuntimeToolDefinitionFactory;
use Superwire\Contracts\Support\Loop\RuntimeToolResultResolver;
use Superwire\Contracts\Support\Stages\CompletionToolLoopStage;
use Superwire\Contracts\Support\Stages\WorkflowTypeValidationStage;

final class LoopAgentDriver implements AgentDriverInterface
{
    private const int MAX_ITERATIONS = 8;

    private CompletionToolLoopStage $completionToolLoopStage;
    private WorkflowTypeValidationStage $workflowTypeValidationStage;
    private CompletionToolDefinitionFactory $completionToolDefinitionFactory;
    private RuntimeToolDefinitionFactory $runtimeToolDefinitionFactory;
    private RuntimeToolResultResolver $runtimeToolResultResolver;
    private ConversationMessageSerializer $conversationMessageSerializer;

    public function __construct(
        private readonly AgentTurnDriverInterface $turnDriver,
        private readonly ?RuntimeToolInvokerInterface $runtimeToolInvoker = null,
        ?CompletionToolLoopStage $completionToolLoopStage = null,
        ?WorkflowTypeValidationStage $workflowTypeValidationStage = null,
        ?CompletionToolDefinitionFactory $completionToolDefinitionFactory = null,
        ?RuntimeToolDefinitionFactory $runtimeToolDefinitionFactory = null,
        ?RuntimeToolResultResolver $runtimeToolResultResolver = null,
        ?ConversationMessageSerializer $conversationMessageSerializer = null,
    )
    {
        $this->completionToolLoopStage = $completionToolLoopStage ?? new CompletionToolLoopStage();
        $this->workflowTypeValidationStage = $workflowTypeValidationStage ?? new WorkflowTypeValidationStage();
        $this->completionToolDefinitionFactory = $completionToolDefinitionFactory ?? new CompletionToolDefinitionFactory($this->completionToolLoopStage);
        $this->runtimeToolDefinitionFactory = $runtimeToolDefinitionFactory ?? new RuntimeToolDefinitionFactory($this->runtimeToolInvoker);
        $this->runtimeToolResultResolver = $runtimeToolResultResolver ?? new RuntimeToolResultResolver(
            $this->runtimeToolInvoker,
            $this->completionToolLoopStage->finalizeSuccessToolName(),
            $this->completionToolLoopStage->finalizeErrorToolName(),
        );
        $this->conversationMessageSerializer = $conversationMessageSerializer ?? new ConversationMessageSerializer();
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
                        'conversation' => $this->conversationMessageSerializer->serialize($messages),
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
                                'conversation' => $this->conversationMessageSerializer->serialize($messages),
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

            $toolResults = $this->runtimeToolResultResolver->resolve($request, $turnResponse->toolCalls, $turnResponse->toolResults);

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
     * @return array<int, AgentToolDefinition>
     */
    private function buildTurnTools(AgentExecutionRequest $request, bool $completionPhaseEnabled): array
    {
        $completionTools = [
            $this->completionToolDefinitionFactory->finalizeSuccessTool($request),
            $this->completionToolDefinitionFactory->finalizeErrorTool(),
        ];

        if ($completionPhaseEnabled) {
            return $completionTools;
        }

        $runtimeTools = [];

        foreach ($request->tools as $toolExecution) {
            $runtimeTools[] = $this->runtimeToolDefinitionFactory->definitionForToolName($toolExecution->name);
        }

        return [ ...$runtimeTools, ...$completionTools ];
    }
}
