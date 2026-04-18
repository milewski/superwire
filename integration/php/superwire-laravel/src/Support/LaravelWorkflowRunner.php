<?php

declare(strict_types=1);

namespace Superwire\Laravel\Support;

use Illuminate\Support\Facades\Concurrency;
use Superwire\Contracts\AgentDefinition;
use Superwire\Contracts\AgentExecutionResult;
use Superwire\Contracts\Contracts\DriverRegistryInterface;
use Superwire\Contracts\Contracts\WorkflowRunnerInterface;
use Superwire\Contracts\Exception\ExpressionResolutionException;
use Superwire\Contracts\Exception\InvalidWorkflowDefinitionException;
use Superwire\Contracts\Support\ExecutionPipeline;
use Superwire\Contracts\Support\ExecutionPlanResolver;
use Superwire\Contracts\Support\ExpressionResolver;
use Superwire\Contracts\Support\Stages\WorkflowDefinitionValidationStage;
use Superwire\Contracts\Support\Stages\WorkflowTypeValidationStage;
use Superwire\Contracts\WorkflowDefinition;
use Superwire\Contracts\WorkflowExecutionResult;
use Superwire\Laravel\Data\ResolvedAgentExecutionData;
use Superwire\Laravel\Data\ResolvedProviderData;
use Superwire\Laravel\Data\ResolvedToolData;

final class LaravelWorkflowRunner implements WorkflowRunnerInterface
{
    private ExecutionPlanResolver $executionPlanResolver;

    private ExpressionResolver $expressionResolver;

    private WorkflowDefinitionValidationStage $workflowDefinitionValidationStage;

    private WorkflowTypeValidationStage $workflowTypeValidationStage;

    public function __construct(
        private readonly DriverRegistryInterface $driverRegistry,
    ) {
        $this->executionPlanResolver = new ExecutionPlanResolver();
        $this->expressionResolver = new ExpressionResolver();
        $this->workflowDefinitionValidationStage = new WorkflowDefinitionValidationStage();
        $this->workflowTypeValidationStage = new WorkflowTypeValidationStage();
    }

    public function run(WorkflowDefinition $workflowDefinition, array $input = [], array $secrets = []): WorkflowExecutionResult
    {
        $pipelineContext = new LaravelWorkflowExecutionPipelineContext($workflowDefinition, $input, $secrets);
        $executionPipeline = (new ExecutionPipeline())
            ->addStage(fn (LaravelWorkflowExecutionPipelineContext $context): LaravelWorkflowExecutionPipelineContext => $this->validateWorkflowStage($context))
            ->addStage(fn (LaravelWorkflowExecutionPipelineContext $context): LaravelWorkflowExecutionPipelineContext => $this->resolveBatchesStage($context))
            ->addStage(fn (LaravelWorkflowExecutionPipelineContext $context): LaravelWorkflowExecutionPipelineContext => $this->executeAgentsStage($context))
            ->addStage(fn (LaravelWorkflowExecutionPipelineContext $context): LaravelWorkflowExecutionPipelineContext => $this->resolveOutputStage($context));

        /** @var LaravelWorkflowExecutionPipelineContext $resolvedContext */
        $resolvedContext = $executionPipeline->run($pipelineContext);

        return new WorkflowExecutionResult($resolvedContext->resolvedOutput, $resolvedContext->agentOutputs, $resolvedContext->agentContexts);
    }

    private function validateWorkflowStage(LaravelWorkflowExecutionPipelineContext $context): LaravelWorkflowExecutionPipelineContext
    {
        $this->workflowDefinitionValidationStage->validate($context->workflowDefinition);

        return $context;
    }

    private function resolveBatchesStage(LaravelWorkflowExecutionPipelineContext $context): LaravelWorkflowExecutionPipelineContext
    {
        $context->executionBatches = $this->resolveExecutionBatches($context->workflowDefinition);

        return $context;
    }

    private function executeAgentsStage(LaravelWorkflowExecutionPipelineContext $context): LaravelWorkflowExecutionPipelineContext
    {
        foreach ($context->executionBatches as $executionBatch) {
            $batchResults = $this->executeBatch(
                workflowDefinition: $context->workflowDefinition,
                batchAgentNames: $executionBatch,
                input: $context->input,
                secrets: $context->secrets,
                agentOutputs: $context->agentOutputs,
                agentContexts: $context->agentContexts,
            );

            foreach ($batchResults as $agentName => $agentExecutionResult) {
                $context->agentOutputs[$agentName] = $agentExecutionResult['output'];
                $context->agentContexts[$agentName] = $agentExecutionResult['context'];
            }
        }

        return $context;
    }

    /**
     * @param list<string> $batchAgentNames
     * @param array<string, mixed> $input
     * @param array<string, mixed> $secrets
     * @param array<string, mixed> $agentOutputs
     * @param array<string, mixed> $agentContexts
     * @return array<string, array{output: mixed, context: mixed}>
     */
    private function executeBatch(
        WorkflowDefinition $workflowDefinition,
        array $batchAgentNames,
        array $input,
        array $secrets,
        array $agentOutputs,
        array $agentContexts,
    ): array {
        $tasks = [];

        foreach ($batchAgentNames as $agentName) {
            $agentDefinition = $workflowDefinition->agentByName($agentName);

            if ($agentDefinition === null) {
                throw new InvalidWorkflowDefinitionException("execution batch references unknown agent `{$agentName}`");
            }

            $tasks[$agentName] = function () use ($workflowDefinition, $agentDefinition, $input, $secrets, $agentOutputs, $agentContexts): array {
                if ($agentDefinition->forEach !== null) {
                    $iterationResults = $this->executeForEachAgent(
                        $workflowDefinition,
                        $agentDefinition,
                        $input,
                        $secrets,
                        $agentOutputs,
                        $agentContexts,
                    );

                    return [
                        'output' => $iterationResults['outputs'],
                        'context' => $iterationResults['contexts'],
                    ];
                }

                $agentResult = $this->executeAgent(
                    workflowDefinition: $workflowDefinition,
                    agentDefinition: $agentDefinition,
                    input: $input,
                    secrets: $secrets,
                    agentOutputs: $agentOutputs,
                    agentContexts: $agentContexts,
                    localBindings: [],
                    expectedOutputKey: 'final_output',
                );

                return [
                    'output' => $agentResult->output,
                    'context' => $agentResult->context,
                ];
            };
        }

        return $this->runConcurrentTasks($tasks);
    }

    /**
     * @param array<string, callable(): array{output: mixed, context: mixed}> $tasks
     * @return array<string, array{output: mixed, context: mixed}>
     */
    private function runConcurrentTasks(array $tasks): array
    {
        try {
            /** @var array<string, array{output: mixed, context: mixed}> $parallelResults */
            $parallelResults = Concurrency::driver(config('superwire.parallel.driver', 'fork'))->run($tasks);

            return $parallelResults;
        } catch (\Throwable) {
            $sequentialResults = [];

            foreach ($tasks as $taskKey => $task) {
                $sequentialResults[$taskKey] = $task();
            }

            return $sequentialResults;
        }
    }

    private function resolveOutputStage(LaravelWorkflowExecutionPipelineContext $context): LaravelWorkflowExecutionPipelineContext
    {
        $context->resolvedOutput = $this->resolveWorkflowOutput(
            $context->workflowDefinition,
            $context->input,
            $context->secrets,
            $context->agentOutputs,
            $context->agentContexts,
        );

        return $context;
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
        string $expectedOutputKey,
    ): AgentExecutionResult {
        $providerDefinition = $workflowDefinition->providerByName($agentDefinition->provider);

        if ($providerDefinition === null) {
            throw new InvalidWorkflowDefinitionException(
                "agent `{$agentDefinition->name}` references unknown provider `{$agentDefinition->provider}`"
            );
        }

        $runtimeContext = $this->buildRuntimeContext($input, $secrets, $agentOutputs, $agentContexts, $localBindings);

        $resolvedModelExpression = $this->expressionResolver->resolve($agentDefinition->model, $runtimeContext);
        $resolvedModel = $this->resolveModelName($resolvedModelExpression, $agentDefinition->name, $providerDefinition->driver);

        $resolvedPrompt = $this->stringifyResolvedValue(
            $this->expressionResolver->resolve($agentDefinition->prompt, $runtimeContext),
            "agent `{$agentDefinition->name}` prompt"
        );

        $resolvedProviderConfig = $this->resolveProviderConfig($providerDefinition->config, $runtimeContext, $agentDefinition->name);

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

            $resolvedTools[] = new ResolvedToolData($toolDefinition['name'], $resolvedToolBindings);
        }

        $resolvedExecution = new ResolvedAgentExecutionData(
            agentName: $agentDefinition->name,
            provider: new ResolvedProviderData(
                name: $providerDefinition->name,
                provider: $providerDefinition->driver,
                config: $resolvedProviderConfig,
            ),
            model: $resolvedModel,
            prompt: $resolvedPrompt,
            context: $resolvedContext,
            inference: $resolvedInference,
            tools: $resolvedTools,
            localBindings: $localBindings,
            dependencies: $agentDefinition->dependencies,
            dependents: $agentDefinition->dependents,
            batch: $agentDefinition->batch,
            expectedOutput: $this->resolveExpectedOutputContract($agentDefinition, $expectedOutputKey),
        );

        $executionDriverKey = $this->resolveExecutionDriverKey($resolvedProviderConfig);
        $executionDriver = $this->driverRegistry->get($executionDriverKey);
        $agentRequest = $resolvedExecution->toRequest();
        $agentResult = $executionDriver->execute($agentRequest);

        $this->workflowTypeValidationStage->validate(
            value: $agentResult->output,
            workflowType: $resolvedExecution->expectedOutputWorkflowType(),
            context: "agent `{$agentDefinition->name}` output"
        );

        return $agentResult;
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

        $iterationTasks = [];

        foreach (array_values($iterableValues) as $index => $iterableValue) {
            $iterationTasks[(string) $index] = function () use (
                $workflowDefinition,
                $agentDefinition,
                $input,
                $secrets,
                $agentOutputs,
                $agentContexts,
                $iterableValue
            ): array {
                $localBindings = $this->buildForEachBindings($agentDefinition, $iterableValue);
                $iterationResult = $this->executeAgent(
                    workflowDefinition: $workflowDefinition,
                    agentDefinition: $agentDefinition,
                    input: $input,
                    secrets: $secrets,
                    agentOutputs: $agentOutputs,
                    agentContexts: $agentContexts,
                    localBindings: $localBindings,
                    expectedOutputKey: 'iteration',
                );

                return [
                    'output' => $iterationResult->output,
                    'context' => $iterationResult->context,
                ];
            };
        }

        $iterationResults = $this->runConcurrentTasks($iterationTasks);

        foreach (array_keys($iterationTasks) as $taskIndex) {
            $iterationOutputs[] = $iterationResults[$taskIndex]['output'];
            $iterationContexts[] = $iterationResults[$taskIndex]['context'];
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

    /**
     * @param array<string, mixed> $providerConfig
     * @param array<string, mixed> $runtimeContext
     * @return array<string, mixed>
     */
    private function resolveProviderConfig(array $providerConfig, array $runtimeContext, string $agentName): array
    {
        $resolvedProviderConfig = $this->expressionResolver->resolve($providerConfig, $runtimeContext);

        if (!is_array($resolvedProviderConfig)) {
            throw new InvalidWorkflowDefinitionException(
                "provider config for agent `{$agentName}` must resolve into an object"
            );
        }

        return $resolvedProviderConfig;
    }

    /**
     * @param array<string, mixed> $resolvedProviderConfig
     */
    private function resolveExecutionDriverKey(array $resolvedProviderConfig): string
    {
        $configuredDriver = $resolvedProviderConfig['execution_driver'] ?? null;

        if (is_string($configuredDriver) && $configuredDriver !== '') {
            return strtolower($configuredDriver);
        }

        return 'prism';
    }

    private function resolveModelName(mixed $resolvedModelExpression, string $agentName, string $providerDriver): string
    {
        if (is_array($resolvedModelExpression) && array_key_exists('$call', $resolvedModelExpression)) {
            $callName = $resolvedModelExpression['$call'] ?? null;

            if (!is_string($callName)) {
                throw new ExpressionResolutionException("agent `{$agentName}` model call contains invalid call target");
            }

            if ($callName !== $providerDriver) {
                throw new ExpressionResolutionException(
                    "agent `{$agentName}` model provider call `{$callName}` does not match provider driver `{$providerDriver}`"
                );
            }

            $callArguments = $resolvedModelExpression['args'] ?? [];

            if (!is_array($callArguments) || $callArguments === []) {
                throw new ExpressionResolutionException("agent `{$agentName}` model call must include at least one argument");
            }

            return $this->stringifyResolvedValue($callArguments[0], "agent `{$agentName}` model argument");
        }

        return $this->stringifyResolvedValue($resolvedModelExpression, "agent `{$agentName}` model");
    }

    /**
     * @return array<string, mixed>
     */
    private function resolveExpectedOutputContract(AgentDefinition $agentDefinition, string $expectedOutputKey): array
    {
        if (!is_array($agentDefinition->output) || !array_key_exists($expectedOutputKey, $agentDefinition->output)) {
            throw new InvalidWorkflowDefinitionException(
                "agent `{$agentDefinition->name}` output contract is missing `{$expectedOutputKey}`"
            );
        }

        $expectedOutput = $agentDefinition->output[$expectedOutputKey];

        if (!is_array($expectedOutput)) {
            throw new InvalidWorkflowDefinitionException(
                "agent `{$agentDefinition->name}` output contract `{$expectedOutputKey}` must be an object"
            );
        }

        return $expectedOutput;
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

final class LaravelWorkflowExecutionPipelineContext
{
    /**
     * @var list<list<string>>
     */
    public array $executionBatches = [];

    /**
     * @var array<string, mixed>
     */
    public array $agentOutputs = [];

    /**
     * @var array<string, mixed>
     */
    public array $agentContexts = [];

    /**
     * @var array<string, mixed>
     */
    public array $resolvedOutput = [];

    /**
     * @param array<string, mixed> $input
     * @param array<string, mixed> $secrets
     */
    public function __construct(
        public readonly WorkflowDefinition $workflowDefinition,
        public readonly array $input,
        public readonly array $secrets,
    ) {
    }
}
