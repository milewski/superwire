<?php

declare(strict_types = 1);

namespace Superwire\Laravel;

use RuntimeException;
use Superwire\Contracts\Contracts\DriverRegistryInterface;
use Superwire\Contracts\Contracts\WorkflowRunnerInterface;
use Superwire\Contracts\Exception\InvalidWorkflowDefinitionException;
use Superwire\Contracts\Support\LoopAgentDriver;
use Superwire\Contracts\Workflow\WorkflowDefinition;
use Superwire\Laravel\Data\WorkflowRunResult;
use Superwire\Laravel\Driver\PrismAgentDriver;
use Superwire\Laravel\Support\CachedWorkflowDefinitionCompiler;
use Superwire\Laravel\Support\LaravelRuntimeToolInvoker;
use Swaggest\JsonSchema\Schema;

final class Workflow
{
    /**
     * @param list<class-string> $toolClasses
     * @param array<string, mixed> $driverConfiguration
     * @param array<string, mixed> $input
     * @param array<string, mixed> $secrets
     */
    private function __construct(
        private readonly string $workflowPath,
        private readonly array $toolClasses = [],
        private readonly string $driverName = 'prism',
        private readonly array $driverConfiguration = [],
        private readonly array $input = [],
        private readonly array $secrets = [],
        private readonly ?string $outputClass = null,
    ) {
    }

    public static function fromFile(string $workflowPath): self
    {
        return new self($workflowPath);
    }

    /**
     * @param list<class-string> $toolClasses
     */
    public function withTools(array $toolClasses): self
    {
        return new self(
            workflowPath: $this->workflowPath,
            toolClasses: $toolClasses,
            driverName: $this->driverName,
            driverConfiguration: $this->driverConfiguration,
            input: $this->input,
            secrets: $this->secrets,
            outputClass: $this->outputClass,
        );
    }

    /**
     * @param array<string, mixed> $driverConfiguration
     */
    public function usingDriver(string $driverName, array $driverConfiguration = []): self
    {
        return new self(
            workflowPath: $this->workflowPath,
            toolClasses: $this->toolClasses,
            driverName: $driverName,
            driverConfiguration: $driverConfiguration,
            input: $this->input,
            secrets: $this->secrets,
            outputClass: $this->outputClass,
        );
    }

    /**
     * @param array<string, mixed> $input
     */
    public function withInputs(array $input): self
    {
        return new self(
            workflowPath: $this->workflowPath,
            toolClasses: $this->toolClasses,
            driverName: $this->driverName,
            driverConfiguration: $this->driverConfiguration,
            input: $input,
            secrets: $this->secrets,
            outputClass: $this->outputClass,
        );
    }

    /**
     * @param array<string, mixed> $secrets
     */
    public function withSecrets(array $secrets): self
    {
        return new self(
            workflowPath: $this->workflowPath,
            toolClasses: $this->toolClasses,
            driverName: $this->driverName,
            driverConfiguration: $this->driverConfiguration,
            input: $this->input,
            secrets: $secrets,
            outputClass: $this->outputClass,
        );
    }

    /**
     * @param class-string $outputClass
     */
    public function mapInto(string $outputClass): self
    {
        return new self(
            workflowPath: $this->workflowPath,
            toolClasses: $this->toolClasses,
            driverName: $this->driverName,
            driverConfiguration: $this->driverConfiguration,
            input: $this->input,
            secrets: $this->secrets,
            outputClass: $outputClass,
        );
    }

    public function run(): WorkflowRunResult
    {
        $workflowDefinition = app(CachedWorkflowDefinitionCompiler::class)->compile($this->workflowPath);
        $runtimeToolInvoker = $this->resolveRuntimeToolInvoker();

        $this->validateConfiguredWorkflowTools($workflowDefinition, $runtimeToolInvoker);
        $this->registerExecutionDriver($runtimeToolInvoker);

        $workflowResult = app(WorkflowRunnerInterface::class)->run(
            $workflowDefinition,
            $this->input,
            $this->resolvedSecrets(),
        );

        return new WorkflowRunResult(
            output: $this->mapOutput($workflowResult->output),
            context: [
                'workflow_output' => $workflowResult->output,
                'agent_outputs' => $workflowResult->agentOutputs,
                'agent_contexts' => $workflowResult->agentContexts,
                'agent_metadata' => $workflowResult->agentMetadata,
                'execution_history' => $workflowResult->executionHistory,
            ],
        );
    }

    private function registerExecutionDriver(LaravelRuntimeToolInvoker $toolInvoker): void
    {
        $driverRegistry = app(DriverRegistryInterface::class);

        if ($this->driverName !== 'prism') {
            throw new RuntimeException("unsupported workflow driver `{$this->driverName}`");
        }

        $driverRegistry->register('prism', new LoopAgentDriver(new PrismAgentDriver($this->driverConfiguration), $toolInvoker));
    }

    private function resolveRuntimeToolInvoker(): LaravelRuntimeToolInvoker
    {
        return app(LaravelRuntimeToolInvoker::class)->withTools($this->toolClasses);
    }

    private function validateConfiguredWorkflowTools(WorkflowDefinition $workflowDefinition, LaravelRuntimeToolInvoker $runtimeToolInvoker): void
    {
        foreach ($workflowDefinition->agents as $agentDefinition) {

            foreach ($agentDefinition->tools as $toolConfiguration) {

                $toolName = $toolConfiguration[ 'name' ];
                $toolBindings = $toolConfiguration[ 'bind' ];

                if (!$runtimeToolInvoker->hasTool($toolName)) {
                    throw new InvalidWorkflowDefinitionException("agent `{$agentDefinition->name}` references unregistered tool `{$toolName}`");
                }

                $bindingSchema = $runtimeToolInvoker->bindingSchemaForTool($toolName);

                if (!$bindingSchema instanceof Schema) {
                    continue;
                }

                $bindingSchemaArray = $this->schemaToArray($bindingSchema, "tool `{$toolName}` binding schema");
                $requiredBindingKeys = $bindingSchemaArray[ 'required' ] ?? [];
                $bindingProperties = $bindingSchemaArray[ 'properties' ] ?? [];
                $allowsAdditionalBindings = ($bindingSchemaArray[ 'additionalProperties' ] ?? true) !== false;

                foreach ($requiredBindingKeys as $requiredBindingKey) {

                    if (!is_string($requiredBindingKey)) {
                        continue;
                    }

                    if (array_key_exists($requiredBindingKey, $toolBindings)) {
                        continue;
                    }

                    throw new InvalidWorkflowDefinitionException(
                        "agent `{$agentDefinition->name}` tool `{$toolName}` is missing required binding `{$requiredBindingKey}`",
                    );

                }

                if ($allowsAdditionalBindings) {
                    continue;
                }

                if (!is_array($bindingProperties)) {
                    continue;
                }

                foreach (array_keys($toolBindings) as $bindingName) {

                    if (!is_string($bindingName)) {
                        continue;
                    }

                    if (array_key_exists($bindingName, $bindingProperties)) {
                        continue;
                    }

                    throw new InvalidWorkflowDefinitionException(
                        "agent `{$agentDefinition->name}` tool `{$toolName}` contains unknown binding `{$bindingName}`",
                    );

                }

            }

        }
    }

    /**
     * @return array<string, mixed>
     */
    private function schemaToArray(Schema $schema, string $context): array
    {
        $decodedSchema = json_decode(json_encode($schema, JSON_THROW_ON_ERROR), true, 512, JSON_THROW_ON_ERROR);

        if (!is_array($decodedSchema)) {
            throw new InvalidWorkflowDefinitionException("{$context} must encode into object payload");
        }

        return $decodedSchema;
    }

    /**
     * @return array<string, mixed>
     */
    private function resolvedSecrets(): array
    {
        return $this->secrets;
    }

    private function mapOutput(mixed $workflowOutput): mixed
    {
        if ($this->outputClass === null) {
            return $workflowOutput;
        }

        if (!class_exists($this->outputClass)) {
            throw new RuntimeException("mapped output class `{$this->outputClass}` does not exist");
        }

        if (!is_array($workflowOutput)) {
            throw new RuntimeException('workflow output must be an array to map into output class');
        }

        $outputClass = $this->outputClass;

        if (method_exists($outputClass, 'from')) {
            return $outputClass::from($workflowOutput);
        }

        return new $outputClass(...$workflowOutput);
    }
}
