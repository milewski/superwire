<?php

declare(strict_types = 1);

namespace Superwire\Laravel;

use Illuminate\Contracts\Container\BindingResolutionException;
use Illuminate\Contracts\Container\CircularDependencyException;
use JsonException;
use Prism\Prism\Enums\Provider;
use Prism\Prism\Providers\Provider as PrismProvider;
use Prism\Prism\PrismManager;
use Prism\Prism\Text\PendingRequest;
use RuntimeException;
use Spatie\Fork\Fork;
use Superwire\Laravel\Data\Workflow\Agent;
use Superwire\Laravel\Data\Workflow\OutputFieldReference;
use Superwire\Laravel\Data\Workflow\WorkflowDefinition;
use Superwire\Laravel\Exceptions\FinalizeError;
use Superwire\Laravel\Exceptions\FinalizeSuccess;
use Superwire\Laravel\Support\PromptParser;
use Superwire\Laravel\Tools\FinalizeErrorTool;
use Superwire\Laravel\Tools\FinalizeSuccessTool;

final readonly class Runtime
{
    private const int MAX_AGENT_REQUEST_ATTEMPTS = 10;
    private const int MAX_AGENT_TOOL_STEPS = 20;

    public function __construct(
        private WorkflowDefinition $definition,
        private PromptParser $promptParser = new PromptParser(),
    )
    {
    }

    /**
     * @return array<string, mixed>
     */
    public function run(): array
    {
        $agentOutputs = [];

        foreach ($this->definition->execution->batches as $batchAgentNames) {

            $agentOutputs = array_merge(
                $agentOutputs,
                $this->runBatch($batchAgentNames, $agentOutputs),
            );

        }

        return $this->resolveWorkflowOutput($agentOutputs);
    }

    private function runBatch(array $batchAgentNames, array $agentOutputs): array
    {
        $agents = [];

        foreach ($batchAgentNames as $agentName) {

            $agent = $this->definition->agents->findByName($agentName);

            $this->validateAgentDependencies($agent, $agentOutputs, $batchAgentNames);

            $agents[ $agentName ] = $agent;

        }

        if (count($agents) === 1) {

            $agentName = array_key_first($agents);

            if ($agentName === null) {
                return [];
            }

            return [
                $agentName => $this->runAgent($agents[ $agentName ], $agentOutputs),
            ];

        }

        $batchResults = Fork::new()->run(...$this->batchTasks($agents, $agentOutputs));
        $resolvedResults = [];

        foreach (array_values(array_keys($agents)) as $index => $agentName) {
            $resolvedResults[ $agentName ] = $batchResults[ $index ];
        }

        return $resolvedResults;
    }

    /**
     * @param array<string, Agent> $agents
     * @param array<string, mixed> $agentOutputs
     * @return array<int, callable(): mixed>
     */
    private function batchTasks(array $agents, array $agentOutputs): array
    {
        $tasks = [];

        foreach ($agents as $agent) {
            $tasks[] = fn (): mixed => $this->runAgent($agent, $agentOutputs);
        }

        return $tasks;
    }

    /**
     * @param array<string, mixed> $agentOutputs
     * @param list<string> $batchAgentNames
     */
    private function validateAgentDependencies(Agent $agent, array $agentOutputs, array $batchAgentNames): void
    {
        foreach ($agent->dependencies as $dependencyName) {

            if (in_array($dependencyName, $batchAgentNames, true)) {

                throw new RuntimeException(
                    message: sprintf('Agent %s cannot run in parallel with dependency %s in the same batch.', $agent->name, $dependencyName),
                );

            }

            if (!array_key_exists($dependencyName, $agentOutputs)) {

                throw new RuntimeException(
                    message: sprintf('Agent %s dependency %s has not completed before its batch.', $agent->name, $dependencyName),
                );

            }

        }
    }

    private function intoProvider(string $provider): Provider
    {
        return match ($provider) {
            'openai' => Provider::OpenAI,
            'ollama' => Provider::Ollama,
            default => throw new RuntimeException(sprintf("Unknown provider: {%s}", $provider)),
        };
    }

    private function runAgent(Agent $agent, array $agentOutputs): mixed
    {
        /**
         * If the agent is not within a for each loop then it can be executed direcly,
         */
        if (!$agent->runsForEach()) {

            return $this->executeAgent(
                agent: $agent,
                prompt: $this->promptParser->render($agent->prompt, $agentOutputs),
                outputSchema: $agent->finalOutputJsonSchema(),
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

            $results = Fork::new()->run(...$this->iterationTasks(
                agent: $agent,
                agentOutputs: $agentOutputs,
                iterationIdentifier: $iterationIdentifier,
                iterationValues: $iterationValues,
            ));

            ksort($results);

            return array_values($results);

        }

        foreach ($iterationValues as $iterationValue) {

            $prompt = $this->promptParser->render($agent->prompt, $agentOutputs, [
                $iterationIdentifier => $iterationValue,
            ]);

            $results[] = $this->executeAgent(
                agent: $agent,
                prompt: $prompt,
                outputSchema: $agent->iterationJsonSchema(),
            );

        }

        return $results;
    }

    /**
     * @return array<int, callable(): mixed>
     */
    private function iterationTasks(Agent $agent, array $agentOutputs, string $iterationIdentifier, array $iterationValues): array
    {
        $tasks = [];

        foreach ($iterationValues as $iterationValue) {

            $tasks[] = fn (): mixed => $this->executeAgent(
                agent: $agent,
                prompt: $this->promptParser->render($agent->prompt, $agentOutputs, [ $iterationIdentifier => $iterationValue ]),
                outputSchema: $agent->iterationJsonSchema(),
            );

        }

        return $tasks;
    }

    private function shouldForkIterations(Agent $agent, array $iterationValues): bool
    {
        if (count($iterationValues) < 2) {
            return false;
        }

        $providerInstance = $this->providerInstance($agent);

        return !str_starts_with($providerInstance::class, 'Prism\\Prism\\Testing\\');
    }

    private function resolveForEachValues(Agent $agent, array $agentOutputs): array
    {
        $reference = $agent->forEachReference();

        if ($reference === null) {
            throw new RuntimeException(sprintf('Agent %s is missing a for_each iterable reference.', $agent->name));
        }

        $resolvedValue = $this->promptParser->resolveReference($reference, $agentOutputs);

        if (!is_array($resolvedValue)) {
            throw new RuntimeException(sprintf('Agent %s for_each iterable must resolve to an array.', $agent->name));
        }

        return array_values($resolvedValue);
    }

    private function executeAgent(Agent $agent, string $prompt, array $outputSchema): mixed
    {
        $finalizeSuccessTool = new FinalizeSuccessTool($outputSchema);
        $finalizeErrorTool = new FinalizeErrorTool();
        $conversationMessages = [];

        for ($attemptNumber = 1; $attemptNumber <= self::MAX_AGENT_REQUEST_ATTEMPTS; $attemptNumber++) {

            $request = $this->agentRequest($agent)
                ->withSystemPrompt($this->finalizationPrompt($outputSchema))
                ->withTools([ $finalizeSuccessTool, $finalizeErrorTool ])
                ->withMaxSteps(self::MAX_AGENT_TOOL_STEPS);

            if ($conversationMessages === []) {
                $request->withPrompt($prompt);
            }

            if ($conversationMessages !== []) {
                $request->withMessages($conversationMessages);
            }

            try {

                $response = $request->asText();

            } catch (FinalizeSuccess $finalizeSuccess) {

                return $finalizeSuccess->result;

            } catch (FinalizeError $finalizeError) {

                throw new RuntimeException(
                    message: sprintf('Agent %s failed: %s', $agent->name, $finalizeError->reason),
                    previous: $finalizeError,
                );

            }

            $conversationMessages = $response->messages->all();
        }

        throw new RuntimeException(
            message: sprintf('Agent %s did not call finalize_success or finalize_error after %d attempts.', $agent->name, self::MAX_AGENT_REQUEST_ATTEMPTS),
        );
    }

    private function agentRequest(Agent $agent): PendingRequest
    {
        $provider = $this->definition->providers->findByName($agent->provider);

        return prism()
            ->text()
            ->using(
                $this->intoProvider($provider->driver),
                $agent->model->name,
                $this->normalizeProviderConfig($provider->config),
            );
    }

    private function normalizeProviderConfig(array $providerConfig): array
    {
        $normalizedConfig = $providerConfig;

        if (array_key_exists('endpoint', $normalizedConfig)) {
            $normalizedConfig[ 'url' ] = $normalizedConfig[ 'endpoint' ];
            unset($normalizedConfig[ 'endpoint' ]);
        }

        unset($normalizedConfig[ 'driver' ], $normalizedConfig[ 'models' ]);

        return $normalizedConfig;
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

    private function resolveWorkflowOutput(array $agentOutputs): array
    {
        return array_map(
            callback: fn (OutputFieldReference $reference) => $this->resolveOutputField($reference, $agentOutputs),
            array: $this->definition->output->fields,
        );
    }

    private function resolveOutputField(OutputFieldReference $reference, array $agentOutputs): mixed
    {
        return $this->promptParser->resolveReference($reference->ref, $agentOutputs);
    }

    /**
     * @throws CircularDependencyException
     * @throws BindingResolutionException
     */
    private function providerInstance(Agent $agent): PrismProvider
    {
        $provider = $this->definition->providers->findByName($agent->provider);

        return app(PrismManager::class)->resolve(
            $this->intoProvider($provider->driver),
            $this->normalizeProviderConfig($provider->config),
        );
    }
}
