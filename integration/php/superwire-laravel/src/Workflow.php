<?php

declare(strict_types = 1);

namespace Superwire\Laravel;

use JsonException;
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
use Swaggest\JsonSchema\Exception as JsonSchemaException;
use Swaggest\JsonSchema\InvalidValue;
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

                foreach ($toolBindings as $bindingName => $bindingExpression) {

                    if (!is_string($bindingName)) {
                        continue;
                    }

                    $bindingPropertySchema = $bindingProperties[ $bindingName ] ?? null;

                    if (!is_array($bindingPropertySchema)) {
                        continue;
                    }

                    $bindingSamples = $this->bindingValidationSamples($bindingExpression, $workflowDefinition);

                    if ($bindingSamples === []) {
                        continue;
                    }

                    $bindingPropertySchemaObject = $this->schemaFromArray(
                        $bindingPropertySchema,
                        "tool `{$toolName}` binding `{$bindingName}` schema",
                    );

                    if ($this->bindingSamplesMatchSchema($bindingSamples, $bindingPropertySchemaObject)) {
                        continue;
                    }

                    throw new InvalidWorkflowDefinitionException(
                        "agent `{$agentDefinition->name}` tool `{$toolName}` binding `{$bindingName}` does not match bound input schema",
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
     * @return list<mixed>
     */
    private function bindingValidationSamples(mixed $bindingExpression, WorkflowDefinition $workflowDefinition): array
    {
        if (!is_array($bindingExpression) || !array_key_exists('$ref', $bindingExpression) || !is_string($bindingExpression[ '$ref' ])) {
            return [ $bindingExpression ];
        }

        $resolvedBindingKind = $this->resolvedReferenceKind($bindingExpression[ '$ref' ], $workflowDefinition);

        if (!is_string($resolvedBindingKind)) {
            return [];
        }

        return match ($resolvedBindingKind) {
            'string' => [ 'sample' ],
            'integer' => [ 1 ],
            'float' => [ 1.5 ],
            'boolean' => [ true ],
            'null' => [ null ],
            'array' => [ [] ],
            'object' => [ (object) [] ],
            default => [],
        };
    }

    /**
     * @param list<mixed> $bindingSamples
     */
    private function bindingSamplesMatchSchema(array $bindingSamples, Schema $bindingPropertySchema): bool
    {
        foreach ($bindingSamples as $bindingSample) {

            try {

                $bindingPropertySchema->in($this->jsonSchemaCompatibleValue($bindingSample));

                return true;

            } catch (JsonSchemaException) {
            }

        }

        return false;
    }

    /**
     * @param array<string, mixed> $schemaPayload
     */
    private function schemaFromArray(array $schemaPayload, string $context): Schema
    {
        try {

            return Schema::import(json_decode(json_encode($schemaPayload, JSON_THROW_ON_ERROR), false, 512, JSON_THROW_ON_ERROR));

        } catch (InvalidValue|JsonException $error) {

            throw new InvalidWorkflowDefinitionException("{$context} is invalid: {$error->getMessage()}");

        }
    }

    private function jsonSchemaCompatibleValue(mixed $value): mixed
    {
        if (!is_array($value)) {
            return $value;
        }

        if (array_is_list($value)) {

            $normalizedValues = [];

            foreach ($value as $listValue) {
                $normalizedValues[] = $this->jsonSchemaCompatibleValue($listValue);
            }

            return $normalizedValues;

        }

        $normalizedObject = [];

        foreach ($value as $objectKey => $objectValue) {

            if (!is_string($objectKey)) {
                continue;
            }

            $normalizedObject[ $objectKey ] = $this->jsonSchemaCompatibleValue($objectValue);

        }

        return (object) $normalizedObject;
    }

    private function resolvedReferenceKind(string $referencePath, WorkflowDefinition $workflowDefinition): ?string
    {
        $referenceSegments = explode('.', $referencePath);

        if ($referenceSegments === []) {
            return null;
        }

        $referenceRoot = array_shift($referenceSegments);

        if (!is_string($referenceRoot) || $referenceRoot === '') {
            return null;
        }

        if ($referenceRoot === 'input') {
            return $this->resolvedKindFromRootWorkflowType($workflowDefinition->input[ 'workflow_type' ] ?? null, $referenceSegments);
        }

        if ($referenceRoot === 'secrets') {
            return $this->resolvedKindFromRootWorkflowType($workflowDefinition->secrets[ 'workflow_type' ] ?? null, $referenceSegments);
        }

        if ($referenceRoot === 'agent') {

            $agentName = array_shift($referenceSegments);

            if (!is_string($agentName) || $agentName === '') {
                return null;
            }

            $referencedAgent = $workflowDefinition->agentByName($agentName);

            if ($referencedAgent === null) {
                return null;
            }

            return $this->resolvedKindFromRootWorkflowType(
                $referencedAgent->output[ 'final_output' ][ 'workflow_type' ] ?? null,
                $referenceSegments,
            );

        }

        return null;
    }

    /**
     * @param array<string, mixed>|null $workflowType
     * @param list<string> $referenceSegments
     */
    private function resolvedKindFromRootWorkflowType(?array $workflowType, array $referenceSegments): ?string
    {
        if (!is_array($workflowType)) {
            return null;
        }

        if ($referenceSegments === []) {
            return $this->normalizedWorkflowKind($workflowType[ 'kind' ] ?? null);
        }

        $workflowKind = $workflowType[ 'kind' ] ?? null;

        if (!is_string($workflowKind)) {
            return null;
        }

        $segment = array_shift($referenceSegments);

        if (!is_string($segment) || $segment === '') {
            return null;
        }

        if ($workflowKind === 'object') {

            $fields = $workflowType[ 'fields' ] ?? null;

            if (!is_array($fields) || !array_key_exists($segment, $fields) || !is_array($fields[ $segment ])) {
                return null;
            }

            return $this->resolvedKindFromRootWorkflowType($fields[ $segment ], $referenceSegments);

        }

        if ($workflowKind === 'array') {

            if (!ctype_digit($segment)) {
                return null;
            }

            $itemType = $workflowType[ 'item_type' ] ?? null;

            if (!is_array($itemType)) {
                return null;
            }

            return $this->resolvedKindFromRootWorkflowType($itemType, $referenceSegments);

        }

        if ($workflowKind === 'tuple') {

            if (!ctype_digit($segment)) {
                return null;
            }

            $tupleItems = $workflowType[ 'items' ] ?? null;

            if (!is_array($tupleItems)) {
                return null;
            }

            $tupleIndex = (int) $segment;
            $tupleItemType = $tupleItems[ $tupleIndex ] ?? null;

            if (!is_array($tupleItemType)) {
                return null;
            }

            return $this->resolvedKindFromRootWorkflowType($tupleItemType, $referenceSegments);

        }

        return null;
    }

    private function normalizedWorkflowKind(mixed $workflowKind): ?string
    {
        if (!is_string($workflowKind)) {
            return null;
        }

        return match ($workflowKind) {
            'string', 'integer', 'float', 'boolean', 'null', 'array', 'object' => $workflowKind,
            'string_enum' => 'string',
            default => null,
        };
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
