<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Concerns;

use RuntimeException;
use Spatie\Fork\Fork;
use Superwire\Laravel\AgentExecutionResult;
use Superwire\Laravel\Data\Agent\Agent;
use Superwire\Laravel\ForkExecutionFailure;
use Throwable;

trait HandlesForkedWorkflowExecution
{
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

        $batchResults = $this->forkRunner()->run(...$this->batchTasks($agents, $agentOutputs));
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

            $tasks[] = fn (): ForkExecutionFailure|AgentExecutionResult => $this->runAgentInFork(
                agent: $agent,
                agentOutputs: $agentOutputs,
            );

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

    /**
     * @return array<int, callable(): mixed>
     */
    private function iterationTasks(Agent $agent, array $agentOutputs, string $iterationIdentifier, array $iterationValues): array
    {
        $tasks = [];

        foreach ($iterationValues as $iterationValue) {

            $tasks[] = fn (): ForkExecutionFailure|AgentExecutionResult => $this->executeAgentInFork(
                agent: $agent,
                prompt: $this->promptParser->render(
                    prompt: $agent->prompt,
                    agentOutputs: $agentOutputs,
                    scope: [ $iterationIdentifier => $iterationValue ],
                    inputValues: $this->inputValues,
                    secretValues: $this->secretValues,
                ),
                outputSchema: $agent->iterationJsonSchema(),
                agentOutputs: $agentOutputs,
                scope: [ $iterationIdentifier => $iterationValue ],
            );

        }

        return $tasks;
    }

    private function shouldForkIterations(Agent $agent, array $iterationValues): bool
    {
        if (!(bool) config('superwire.runtime.fork', true)) {
            return false;
        }

        if (count($iterationValues) < 2) {
            return false;
        }

        $providerInstance = $this->providerInstance($agent);

        return !str_starts_with($providerInstance::class, 'Prism\\Prism\\Testing\\');
    }

    private function forkRunner(): Fork
    {
        return Fork::new()->before(
            child: function (): void {
                $this->prepareForkedChildProcess();
            },
        );
    }

    private function prepareForkedChildProcess(): void
    {
        if (!app()->bound('db')) {
            return;
        }

        $databaseManager = app('db');

        if (!method_exists($databaseManager, 'getConnections') || !method_exists($databaseManager, 'purge')) {
            return;
        }

        // Forked children must not reuse inherited PDO sockets from the parent process.
        foreach (array_keys($databaseManager->getConnections()) as $connectionName) {
            $databaseManager->purge($connectionName);
        }
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

    private function executeAgentInFork(Agent $agent, string $prompt, array $outputSchema, array $agentOutputs, array $scope = []): AgentExecutionResult|ForkExecutionFailure
    {
        try {

            return $this->executeAgent($agent, $prompt, $outputSchema, $agentOutputs, $scope);

        } catch (Throwable $throwable) {

            return ForkExecutionFailure::fromThrowable($throwable);

        }
    }
}
