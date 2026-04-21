<?php

declare(strict_types = 1);

namespace Superwire\Laravel;

use Prism\Prism\Enums\Provider;
use Prism\Prism\PrismManager;
use RuntimeException;
use Spatie\Fork\Fork;
use Superwire\Laravel\Data\Workflow\Agent;
use Superwire\Laravel\Data\Workflow\OutputFieldReference;
use Superwire\Laravel\Data\Workflow\WorkflowDefinition;
use Superwire\Laravel\Support\PromptParser;

final readonly class Runtime
{
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

    /**
     * @param list<string> $batchAgentNames
     * @param array<string, mixed> $agentOutputs
     * @return array<string, mixed>
     */
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
                throw new RuntimeException(sprintf(
                    'Agent %s cannot run in parallel with dependency %s in the same batch.',
                    $agent->name,
                    $dependencyName,
                ));
            }

            if (!array_key_exists($dependencyName, $agentOutputs)) {
                throw new RuntimeException(sprintf(
                    'Agent %s dependency %s has not completed before its batch.',
                    $agent->name,
                    $dependencyName,
                ));
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

    /**
     * @param array<string, mixed> $agentOutputs
     */
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

            return Fork::new()->run(...$this->iterationTasks(
                agent: $agent,
                agentOutputs: $agentOutputs,
                iterationIdentifier: $iterationIdentifier,
                iterationValues: $iterationValues,
            ));

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
     * @param array<string, mixed> $agentOutputs
     * @param list<mixed> $iterationValues
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

    /**
     * @param list<mixed> $iterationValues
     */
    private function shouldForkIterations(Agent $agent, array $iterationValues): bool
    {
        if (count($iterationValues) < 2) {
            return false;
        }

        $provider = $this->definition->providers->findByName($agent->provider);
        $providerInstance = app(PrismManager::class)->resolve(
            $this->intoProvider($provider->driver),
            $this->normalizeProviderConfig($provider->config),
        );

        return !str_starts_with($providerInstance::class, 'Prism\\Prism\\Testing\\');
    }

    /**
     * @param array<string, mixed> $agentOutputs
     * @return list<mixed>
     */
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

    /**
     * @param array<string, mixed> $outputSchema
     */
    private function executeAgent(Agent $agent, string $prompt, array $outputSchema): mixed
    {
        $provider = $this->definition->providers->findByName($agent->provider);
        $response = prism()
            ->text()
            ->using($this->intoProvider($provider->driver), $agent->model->name, $this->normalizeProviderConfig($provider->config))
            ->withSystemPrompt($this->outputSchemaPrompt($outputSchema))
            ->withPrompt($prompt)
            ->asText();

        return $this->decodeAgentResponse($agent->name, $response->text);
    }

    /**
     * @param array<string, mixed> $providerConfig
     * @return array<string, mixed>
     */
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

    /**
     * @param array<string, mixed> $outputSchema
     */
    private function outputSchemaPrompt(array $outputSchema): string
    {
        return sprintf(
            "Respond only with valid JSON matching this schema exactly. Do not include markdown, code fences, or explanation. Schema: %s",
            json_encode($outputSchema, JSON_THROW_ON_ERROR),
        );
    }

    private function decodeAgentResponse(string $agentName, string $responseText): mixed
    {
        $normalizedResponseText = trim($responseText);

        if (str_starts_with($normalizedResponseText, '```')) {
            $normalizedResponseText = preg_replace('/^```(?:json)?\s*|\s*```$/', '', $normalizedResponseText) ?? $normalizedResponseText;
            $normalizedResponseText = trim($normalizedResponseText);
        }

        try {
            return json_decode($normalizedResponseText, true, flags: JSON_THROW_ON_ERROR);
        } catch (\JsonException $jsonException) {
            throw new RuntimeException(
                sprintf('Agent %s returned invalid JSON: %s', $agentName, $responseText),
                previous: $jsonException,
            );
        }
    }

    /**
     * @param array<string, mixed> $agentOutputs
     * @return array<string, mixed>
     */
    private function resolveWorkflowOutput(array $agentOutputs): array
    {
        $output = [];

        foreach ($this->definition->output->fields as $fieldName => $reference) {
            $output[ $fieldName ] = $this->resolveOutputField($reference, $agentOutputs);
        }

        return $output;
    }

    /**
     * @param array<string, mixed> $agentOutputs
     */
    private function resolveOutputField(OutputFieldReference $reference, array $agentOutputs): mixed
    {
        return $this->promptParser->resolveReference($reference->ref, $agentOutputs);
    }
}
