<?php

declare(strict_types = 1);

namespace Superwire\Laravel;

use Illuminate\Contracts\Container\BindingResolutionException;
use Illuminate\Contracts\Container\CircularDependencyException;
use Prism\Prism\Enums\Provider;
use Prism\Prism\PrismManager;
use Prism\Prism\Providers\Provider as PrismProvider;
use Prism\Prism\Text\PendingRequest;
use Prism\Prism\Tool;
use RuntimeException;
use Spatie\Fork\Fork;
use Superwire\Laravel\Data\Workflow\Agent;
use Superwire\Laravel\Data\Workflow\OutputFieldReference;
use Superwire\Laravel\Data\Workflow\WorkflowDefinition;
use Superwire\Laravel\Support\PromptParser;
use Superwire\Laravel\Tools\AgentToolset;
use Superwire\Laravel\Tools\WorkflowTool;
use Throwable;

final readonly class Runtime
{
    public function __construct(
        private WorkflowDefinition $definition,
        private PromptParser $promptParser = new PromptParser(),
        private array $inputValues = [],
        private array $secretValues = [],
        private array $tools = [],
    )
    {
    }

    /**
     * @param array<string, mixed> $inputValues
     */
    public function withInputs(array $inputValues): self
    {
        return new self($this->definition, $this->promptParser, $inputValues, $this->secretValues, $this->tools);
    }

    /**
     * @param array<string, mixed> $secretValues
     */
    public function withSecrets(array $secretValues): self
    {
        return new self($this->definition, $this->promptParser, $this->inputValues, $secretValues, $this->tools);
    }

    /**
     * @param array<int, Tool|WorkflowTool> $tools
     */
    public function withTools(array $tools): self
    {
        return new self($this->definition, $this->promptParser, $this->inputValues, $this->secretValues, $tools);
    }

    public function run(): WorkflowExecutionResult
    {
        $this->definition->validateInputValues($this->inputValues);
        $this->definition->validateSecretValues($this->secretValues);

        $agentOutputs = [];

        foreach ($this->definition->execution->batches as $batchAgentNames) {

            $agentOutputs = array_merge(
                $agentOutputs,
                $this->runBatch($batchAgentNames, $agentOutputs),
            );

        }

        return new WorkflowExecutionResult(
            output: $this->resolveWorkflowOutput($agentOutputs),
            agents: $agentOutputs,
        );
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
            $resolvedResults[ $agentName ] = $this->normalizeExecutionResult($batchResults[ $index ], sprintf('batch agent %s', $agentName));
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
            $tasks[] = fn (): mixed => $this->runAgentInFork($agent, $agentOutputs);
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
                fn (mixed $result): AgentExecutionResult => $this->normalizeExecutionResult($result, sprintf('iteration agent %s', $agent->name)),
                $results,
            ),
        );
    }

    /**
     * @return array<int, callable(): mixed>
     */
    private function iterationTasks(Agent $agent, array $agentOutputs, string $iterationIdentifier, array $iterationValues): array
    {
        $tasks = [];

        foreach ($iterationValues as $iterationValue) {

            $tasks[] = fn (): mixed => $this->executeAgentInFork(
                agent: $agent,
                prompt: $this->promptParser->render($agent->prompt, $agentOutputs, [ $iterationIdentifier => $iterationValue ], $this->inputValues, $this->secretValues),
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

        $resolvedValue = $this->promptParser->resolveReference($reference, $agentOutputs, [], $this->inputValues, $this->secretValues);

        if (!is_array($resolvedValue)) {
            throw new RuntimeException(sprintf('Agent %s for_each iterable must resolve to an array.', $agent->name));
        }

        return array_values($resolvedValue);
    }

    private function executeAgent(Agent $agent, string $prompt, array $outputSchema): AgentExecutionResult
    {
        $toolset = AgentToolset::fromArray($this->tools, $outputSchema);

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

            $response = $request->asText();
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

    private function agentRequest(Agent $agent): PendingRequest
    {
        $provider = $this->definition->providers->findByName($agent->provider);

        $request = prism()
            ->text()
            ->using(
                $this->intoProvider($provider->driver),
                $this->resolveModel($agent),
                $this->normalizeProviderConfig($provider->config),
            );

        if ($agent->inference->temperature() !== null) {
            $request->usingTemperature($agent->inference->temperature());
        }

        if ($agent->inference->maxTokens() !== null) {
            $request->withMaxTokens($agent->inference->maxTokens());
        }

        if ($agent->inference->topP() !== null) {
            $request->usingTopP($agent->inference->topP());
        }

        return $request;
    }

    private function normalizeProviderConfig(array $providerConfig): array
    {
        $normalizedConfig = $this->resolveConfigReferences($providerConfig);

        if (array_key_exists('endpoint', $normalizedConfig)) {
            $normalizedConfig[ 'url' ] = $normalizedConfig[ 'endpoint' ];
            unset($normalizedConfig[ 'endpoint' ]);
        }

        unset($normalizedConfig[ 'driver' ], $normalizedConfig[ 'models' ]);

        return $normalizedConfig;
    }

    private function resolveConfigReferences(mixed $value): mixed
    {
        if (!is_array($value)) {
            return $value;
        }

        if (array_keys($value) === [ '$ref' ] && is_string($value[ '$ref' ])) {
            return $this->promptParser->resolveReference($value[ '$ref' ], [], [], $this->inputValues, $this->secretValues);
        }

        $resolvedValue = [];

        foreach ($value as $key => $itemValue) {
            $resolvedValue[ $key ] = $this->resolveConfigReferences($itemValue);
        }

        return $resolvedValue;
    }

    private function resolveModel(Agent $agent): string
    {
        if ($agent->model->name !== null) {
            return $agent->model->name;
        }

        if ($agent->model->reference !== null) {

            $resolvedModel = $this->promptParser->resolveReference(
                reference: $agent->model->reference,
                agentOutputs: [],
                inputValues: $this->inputValues,
                secretValues: $this->secretValues,
            );

            if (!is_string($resolvedModel)) {
                throw new RuntimeException(sprintf('Resolved model reference for agent %s must be a string.', $agent->name));
            }

            return $resolvedModel;

        }

        throw new RuntimeException(sprintf('Agent %s does not define a resolvable model.', $agent->name));
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

    private function normalizeExecutionResult(mixed $result, string $context): AgentExecutionResult
    {
        if ($result instanceof AgentExecutionResult) {
            return $result;
        }

        if ($result instanceof ForkExecutionFailure) {
            throw $result->toRuntimeException($context);
        }

        throw new RuntimeException(
            message: sprintf(
                'Invalid execution result returned for %s. Expected %s, received %s. This usually means a forked child process terminated before returning a valid result.',
                $context,
                AgentExecutionResult::class,
                get_debug_type($result),
            ),
        );
    }

    private function runAgentInFork(Agent $agent, array $agentOutputs): AgentExecutionResult|ForkExecutionFailure
    {
        try {
            return $this->runAgent($agent, $agentOutputs);
        } catch (Throwable $throwable) {
            return ForkExecutionFailure::fromThrowable($throwable);
        }
    }

    private function executeAgentInFork(Agent $agent, string $prompt, array $outputSchema): AgentExecutionResult|ForkExecutionFailure
    {
        try {
            return $this->executeAgent($agent, $prompt, $outputSchema);
        } catch (Throwable $throwable) {
            return ForkExecutionFailure::fromThrowable($throwable);
        }
    }

    private function maxAgentToolSteps(): int
    {
        return (int)config('superwire.runtime.max_agent_tool_steps', 20);
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
