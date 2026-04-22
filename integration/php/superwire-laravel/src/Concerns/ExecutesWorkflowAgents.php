<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Concerns;

use Prism\Prism\Streaming\StreamCollector;
use Prism\Prism\Text\PendingRequest;
use Prism\Prism\Text\Response;
use RuntimeException;
use Superwire\Laravel\AgentExecutionResult;
use Superwire\Laravel\Data\Agent\Agent;
use Superwire\Laravel\Tools\AgentToolset;

trait ExecutesWorkflowAgents
{
    private function runAgent(Agent $agent, array $agentOutputs): AgentExecutionResult
    {
        /**
         * If the agent is not within a for each loop then it can be executed direcly,
         */
        if (!$agent->runsForEach()) {

            return $this->executeAgent(
                agent: $agent,
                prompt: $this->promptParser->render($agent->prompt, $agentOutputs, [], $this->inputValues, $this->secretValues),
                outputSchema: $agent->finalOutputJsonSchema(),
                agentOutputs: $agentOutputs,
            );

        }

        /**
         * When agent is within a for each loop first the dependencies has to be fetch to be provided for each interaction
         */
        $iterationValues = $this->resolveForEachValues($agent, $agentOutputs);
        $iterationIdentifier = $agent->forEachIdentifier();

        if ($iterationIdentifier === null) {
            throw new RuntimeException(sprintf('Agent %s is missing a for_each identifier.', $agent->name));
        }

        $results = [];

        /**
         * then a check is done to determine if this can actually be ran in parallel. for  example if only 1 values is present
         * then there is no point in running parallel
         */
        if ($this->shouldForkIterations($agent, $iterationValues)) {

            $results = $this->forkRunner()->run(...$this->iterationTasks(
                agent: $agent,
                agentOutputs: $agentOutputs,
                iterationIdentifier: $iterationIdentifier,
                iterationValues: $iterationValues,
            ));

            ksort($results);

            $iterationResults = array_map(
                callback: fn (mixed $result): AgentExecutionResult => $this->normalizeExecutionResult($result, sprintf('iteration agent %s', $agent->name)),
                array: array_values($results),
            );

            return new AgentExecutionResult(
                output: array_map(
                    callback: fn (AgentExecutionResult $iterationResult): mixed => $iterationResult->output,
                    array: $iterationResults,
                ),
                iterations: $iterationResults,
            );

        }

        foreach ($iterationValues as $iterationValue) {

            $prompt = $this->promptParser->render(
                prompt: $agent->prompt,
                agentOutputs: $agentOutputs,
                scope: [ $iterationIdentifier => $iterationValue ],
                inputValues: $this->inputValues,
                secretValues: $this->secretValues,
            );

            $results[] = $this->executeAgent(
                agent: $agent,
                prompt: $prompt,
                outputSchema: $agent->iterationJsonSchema(),
                agentOutputs: $agentOutputs,
                scope: [ $iterationIdentifier => $iterationValue ],
            );

        }

        return new AgentExecutionResult(
            output: array_map(
                callback: fn (AgentExecutionResult $iterationResult): mixed => $iterationResult->output,
                array: array_map(
                    fn (mixed $result): AgentExecutionResult => $this->normalizeExecutionResult($result, sprintf('iteration agent %s', $agent->name)),
                    $results,
                ),
            ),
            iterations: array_map(
                callback: fn (mixed $result): AgentExecutionResult => $this->normalizeExecutionResult($result, sprintf('iteration agent %s', $agent->name)),
                array: $results,
            ),
        );
    }

    private function executeAgent(Agent $agent, string $prompt, array $outputSchema, array $agentOutputs, array $scope = []): AgentExecutionResult
    {
        $toolset = AgentToolset::fromArray(
            tools: $this->resolveToolsForAgent($agent),
            outputSchema: $outputSchema,
            toolBindings: $this->resolveToolBindingsForAgent($agent, $agentOutputs, $scope),
        );

        $conversationMessages = [];

        for ($toolStepNumber = 1; $toolStepNumber <= $this->maxAgentToolSteps(); $toolStepNumber++) {

            $toolset->resetFinalization();

            $request = $this->agentRequest($agent)
                ->withSystemPrompt($this->finalizationPrompt($outputSchema))
                ->withTools($toolset->prismTools())
                ->withMaxSteps(1);

            if ($conversationMessages === []) {
                $request->withPrompt($prompt);
            }

            if ($conversationMessages !== []) {
                $request->withMessages($conversationMessages);
            }

            $response = $this->executePendingRequest($request);
            $conversationMessages = $response->messages->all();

            $finalizedExecutionResult = $toolset->finalizeExecutionResult(
                agentName: $agent->name,
                messages: $this->messagesToArray($conversationMessages),
            );

            if ($finalizedExecutionResult !== null) {
                return $finalizedExecutionResult;
            }

        }

        throw new RuntimeException(
            message: sprintf('Agent %s did not call finalize_success or finalize_error after %d tool steps.', $agent->name, $this->maxAgentToolSteps()),
        );
    }

    private function executePendingRequest(PendingRequest $request): Response
    {
        if (!$this->shouldStreamResponses()) {
            return $request->asText();
        }

        $textRequest = $request->toRequest();
        $streamedResponse = null;

        $streamCollector = new StreamCollector(
            stream: $request->asStream(),
            pendingRequest: $request,
            onCompleteCallback: static function ($completedRequest, $messages, Response $response) use (&$streamedResponse): void {
                $streamedResponse = $response;
            },
        );

        foreach ($streamCollector->collect() as $streamEvent) {
            unset($streamEvent);
        }

        if (!$streamedResponse instanceof Response) {
            throw new RuntimeException('Streaming request completed without producing a text response.');
        }

        return new Response(
            steps: $streamedResponse->steps,
            text: $streamedResponse->text,
            finishReason: $streamedResponse->finishReason,
            toolCalls: $streamedResponse->toolCalls,
            toolResults: $streamedResponse->toolResults,
            usage: $streamedResponse->usage,
            meta: $streamedResponse->meta,
            messages: collect([ ...$textRequest->messages(), ...$streamedResponse->messages->all() ]),
            additionalContent: $streamedResponse->additionalContent,
            raw: $streamedResponse->raw,
        );
    }

    private function maxAgentToolSteps(): int
    {
        return (int) config('superwire.runtime.max_agent_tool_steps', 20);
    }

    /**
     * @param array<int, object> $messages
     * @return array<int, array<string, mixed>>
     */
    private function messagesToArray(array $messages): array
    {
        return array_map(
            callback: static fn (object $message): array => method_exists($message, 'toArray') ? $message->toArray() : [ 'type' => 'unknown' ],
            array: $messages,
        );
    }

    private function finalizationPrompt(array $outputSchema): string
    {
        return sprintf(
            <<<Prompt
            You must finish by calling exactly one tool: `finalize_success` or `finalize_error`.
            Call finalize_success when you have the final agent output.
            The finalize_success result argument must match this JSON schema exactly: %s.
            If you cannot complete the task, call finalize_error with a clear message.
            Do not end with plain text without calling one of these tools.
            Prompt,
            json_encode($outputSchema, JSON_THROW_ON_ERROR),
        );
    }
}
