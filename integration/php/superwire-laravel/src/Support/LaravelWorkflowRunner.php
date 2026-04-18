<?php

declare(strict_types=1);

namespace Superwire\Laravel\Support;

use Superwire\Contracts\AgentDefinition;
use Superwire\Contracts\AgentExecutionRequest;
use Superwire\Contracts\Contracts\DriverRegistryInterface;
use Superwire\Contracts\Contracts\WorkflowRunnerInterface;
use Superwire\Contracts\Exception\ExpressionResolutionException;
use Superwire\Contracts\Exception\InvalidWorkflowDefinitionException;
use Superwire\Contracts\Support\ExecutionPlanResolver;
use Superwire\Contracts\Support\ExpressionResolver;
use Superwire\Contracts\WorkflowDefinition;
use Superwire\Contracts\WorkflowExecutionResult;

final class LaravelWorkflowRunner implements WorkflowRunnerInterface
{
    private ExecutionPlanResolver $executionPlanResolver;

    private ExpressionResolver $expressionResolver;

    public function __construct(
        private readonly DriverRegistryInterface $driverRegistry,
    ) {
        $this->executionPlanResolver = new ExecutionPlanResolver();
        $this->expressionResolver = new ExpressionResolver();
    }

    public function run(WorkflowDefinition $workflowDefinition, array $input = [], array $secrets = []): WorkflowExecutionResult
    {
        $executionBatches = $this->resolveExecutionBatches($workflowDefinition);
        $agentOutputs = [];
        $agentContexts = [];

        foreach ($executionBatches as $executionBatch) {
            foreach ($executionBatch as $agentName) {
                $agentDefinition = $workflowDefinition->agentByName($agentName);

                if ($agentDefinition === null) {
                    throw new InvalidWorkflowDefinitionException("execution batch references unknown agent `{$agentName}`");
                }

                if ($agentDefinition->forEach !== null) {
                    $iterationResults = $this->executeForEachAgent($workflowDefinition, $agentDefinition, $input, $secrets, $agentOutputs, $agentContexts);
                    $agentOutputs[$agentDefinition->name] = $iterationResults['outputs'];
                    $agentContexts[$agentDefinition->name] = $iterationResults['contexts'];

                    continue;
                }

                $agentResult = $this->executeAgent(
                    workflowDefinition: $workflowDefinition,
                    agentDefinition: $agentDefinition,
                    input: $input,
                    secrets: $secrets,
                    agentOutputs: $agentOutputs,
                    agentContexts: $agentContexts,
                    localBindings: []
                );

                $agentOutputs[$agentDefinition->name] = $agentResult->output;
                $agentContexts[$agentDefinition->name] = $agentResult->context;
            }
        }

        $resolvedOutput = $this->resolveWorkflowOutput($workflowDefinition, $input, $secrets, $agentOutputs, $agentContexts);

        return new WorkflowExecutionResult($resolvedOutput, $agentOutputs, $agentContexts);
    }

    /**
     * @param array<string, mixed> $input
     * @param array<string, mixed> $secrets
     * @param array<string, mixed> $agentOutputs
     * @param array<string, mixed> $agentContexts
     * @param array<string, mixed> $localBindings
     */
    private function executeAgent(
        WorkflowDefinition $workflowDefinition,
        AgentDefinition $agentDefinition,
        array $input,
        array $secrets,
        array $agentOutputs,
        array $agentContexts,
        array $localBindings,
    ): \Superwire\Contracts\AgentExecutionResult {
        $providerDefinition = $workflowDefinition->providerByName($agentDefinition->provider);

        if ($providerDefinition === null) {
            throw new InvalidWorkflowDefinitionException(
                "agent `{$agentDefinition->name}` references unknown provider `{$agentDefinition->provider}`"
            );
        }

        $runtimeContext = $this->buildRuntimeContext($input, $secrets, $agentOutputs, $agentContexts, $localBindings);
        $resolvedModel = $this->stringifyResolvedValue(
            $this->expressionResolver->resolve($agentDefinition->model, $runtimeContext),
            "agent `{$agentDefinition->name}` model"
        );
        $resolvedPrompt = $this->stringifyResolvedValue(
            $this->expressionResolver->resolve($agentDefinition->prompt, $runtimeContext),
            "agent `{$agentDefinition->name}` prompt"
        );
        $resolvedProviderConfig = $this->expressionResolver->resolve($providerDefinition->config, $runtimeContext);
        $resolvedContext = $agentDefinition->context === null
            ? null
            : $this->expressionResolver->resolve($agentDefinition->context, $runtimeContext);
        $resolvedInference = $agentDefinition->inference === null
            ? null
            : $this->expressionResolver->resolve($agentDefinition->inference, $runtimeContext);

        $resolvedTools = [];

        foreach ($agentDefinition->tools as $toolDefinition) {
            $resolvedToolBindings = [];

            foreach ($toolDefinition['bind'] as $bindingName => $bindingValue) {
                if (!is_string($bindingName)) {
                    throw new InvalidWorkflowDefinitionException('tool binding names must be strings');
                }

                $resolvedToolBindings[$bindingName] = $this->expressionResolver->resolve($bindingValue, $runtimeContext);
            }

            $resolvedTools[] = [
                'name' => $toolDefinition['name'],
                'bind' => $resolvedToolBindings,
            ];
        }

        $driver = $this->driverRegistry->get($providerDefinition->driver);
        $request = new AgentExecutionRequest(
            agentName: $agentDefinition->name,
            providerName: $providerDefinition->name,
            driverName: $providerDefinition->driver,
            model: $resolvedModel,
            prompt: $resolvedPrompt,
            provider: is_array($resolvedProviderConfig) ? $resolvedProviderConfig : $providerDefinition->config,
            context: $resolvedContext,
            inference: $resolvedInference,
            tools: $resolvedTools,
            localBindings: $localBindings,
            metadata: [
                'dependencies' => $agentDefinition->dependencies,
                'dependents' => $agentDefinition->dependents,
                'batch' => $agentDefinition->batch,
                'output' => $agentDefinition->output,
            ],
        );

        return $driver->execute($request);
    }

    /**
     * @param array<string, mixed> $input
     * @param array<string, mixed> $secrets
     * @param array<string, mixed> $agentOutputs
     * @param array<string, mixed> $agentContexts
     * @return array{outputs: list<mixed>, contexts: list<mixed>}
     */
    private function executeForEachAgent(
        WorkflowDefinition $workflowDefinition,
        AgentDefinition $agentDefinition,
        array $input,
        array $secrets,
        array $agentOutputs,
        array $agentContexts,
    ): array {
        $runtimeContext = $this->buildRuntimeContext($input, $secrets, $agentOutputs, $agentContexts, []);
        $iterableValues = $this->expressionResolver->resolve($agentDefinition->forEach->iterable, $runtimeContext);

        if (!is_array($iterableValues)) {
            throw new ExpressionResolutionException("agent `{$agentDefinition->name}` for_each iterable must resolve to array");
        }

        $iterationOutputs = [];
        $iterationContexts = [];

        foreach ($iterableValues as $iterableValue) {
            $localBindings = $this->buildForEachBindings($agentDefinition, $iterableValue);
            $iterationResult = $this->executeAgent(
                workflowDefinition: $workflowDefinition,
                agentDefinition: $agentDefinition,
                input: $input,
                secrets: $secrets,
                agentOutputs: $agentOutputs,
                agentContexts: $agentContexts,
                localBindings: $localBindings,
            );

            $iterationOutputs[] = $iterationResult->output;
            $iterationContexts[] = $iterationResult->context;
        }

        return [
            'outputs' => $iterationOutputs,
            'contexts' => $iterationContexts,
        ];
    }

    /**
     * @param array<string, mixed> $input
     * @param array<string, mixed> $secrets
     * @param array<string, mixed> $agentOutputs
     * @param array<string, mixed> $agentContexts
     * @param array<string, mixed> $localBindings
     * @return array<string, mixed>
     */
    private function buildRuntimeContext(
        array $input,
        array $secrets,
        array $agentOutputs,
        array $agentContexts,
        array $localBindings,
    ): array {
        return array_merge(
            [
                'input' => $input,
                'secrets' => $secrets,
                'agent' => $agentOutputs,
                'context' => $agentContexts,
            ],
            $localBindings,
        );
    }

    /**
     * @param array<string, mixed> $input
     * @param array<string, mixed> $secrets
     * @param array<string, mixed> $agentOutputs
     * @param array<string, mixed> $agentContexts
     * @return array<string, mixed>
     */
    private function resolveWorkflowOutput(
        WorkflowDefinition $workflowDefinition,
        array $input,
        array $secrets,
        array $agentOutputs,
        array $agentContexts,
    ): array {
        if (!array_key_exists('fields', $workflowDefinition->output) || !is_array($workflowDefinition->output['fields'])) {
            throw new InvalidWorkflowDefinitionException('workflow output requires an object `fields` entry');
        }

        $runtimeContext = $this->buildRuntimeContext($input, $secrets, $agentOutputs, $agentContexts, []);
        $resolvedOutput = [];

        foreach ($workflowDefinition->output['fields'] as $outputFieldName => $outputFieldValue) {
            if (!is_string($outputFieldName)) {
                throw new InvalidWorkflowDefinitionException('workflow output field names must be strings');
            }

            $resolvedOutput[$outputFieldName] = $this->expressionResolver->resolve($outputFieldValue, $runtimeContext);
        }

        return $resolvedOutput;
    }

    /**
     * @param array<string, mixed> $iterableValue
     * @return array<string, mixed>
     */
    private function buildForEachBindings(AgentDefinition $agentDefinition, mixed $iterableValue): array
    {
        $pattern = $agentDefinition->forEach->pattern;

        if (array_key_exists('identifier', $pattern) && is_string($pattern['identifier'])) {
            return [$pattern['identifier'] => $iterableValue];
        }

        if (array_key_exists('object', $pattern) && is_array($pattern['object'])) {
            if (!is_array($iterableValue)) {
                throw new InvalidWorkflowDefinitionException(
                    "agent `{$agentDefinition->name}` object destructuring requires iterable object values"
                );
            }

            $bindings = [];

            foreach ($pattern['object'] as $fieldName) {
                if (!is_string($fieldName)) {
                    throw new InvalidWorkflowDefinitionException('for_each object pattern fields must be strings');
                }

                $bindings[$fieldName] = $iterableValue[$fieldName] ?? null;
            }

            return $bindings;
        }

        throw new InvalidWorkflowDefinitionException("agent `{$agentDefinition->name}` has invalid for_each pattern");
    }

    /**
     * @return list<list<string>>
     */
    private function resolveExecutionBatches(WorkflowDefinition $workflowDefinition): array
    {
        if (
            array_key_exists('batches', $workflowDefinition->execution)
            && is_array($workflowDefinition->execution['batches'])
            && $workflowDefinition->execution['batches'] !== []
        ) {
            return array_map(
                static fn (mixed $batch): array => is_array($batch) ? array_values($batch) : [],
                array_values($workflowDefinition->execution['batches'])
            );
        }

        return $this->executionPlanResolver->resolveBatches($workflowDefinition->agents);
    }

    private function stringifyResolvedValue(mixed $resolvedValue, string $context): string
    {
        if (is_string($resolvedValue)) {
            return $resolvedValue;
        }

        if (is_int($resolvedValue) || is_float($resolvedValue) || is_bool($resolvedValue)) {
            return (string) $resolvedValue;
        }

        throw new ExpressionResolutionException("{$context} must resolve to a scalar string value");
    }
}
