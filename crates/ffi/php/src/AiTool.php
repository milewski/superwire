<?php

declare(strict_types=1);

namespace EngineAi\Ffi;

use EngineAi\Ffi\Attributes\InputArrayOf;
use EngineAi\Ffi\Attributes\InputSchema;
use EngineAi\Ffi\Attributes\InputTuple;
use InvalidArgumentException;
use ReflectionClass;
use ReflectionNamedType;
use ReflectionProperty;
use ReflectionType;
use ReflectionUnionType;
use RuntimeException;
use Throwable;

abstract class AiTool
{
    private array $availableSecrets = [];

    public function name(): string
    {
        return self::defaultToolName(static::class);
    }

    public function description(): string
    {
        return sprintf('Tool `%s`.', $this->name());
    }

    public function outputSchema(): ?array
    {
        return null;
    }

    final public function definition(): array
    {
        return [
            'name' => $this->name(),
            'description' => $this->description(),
            'input_schema' => $this->inputSchema(),
            'output_schema' => $this->outputSchema(),
            'execution_contract' => 'host_callback',
        ];
    }

    final public function invoke(array $toolInput, array $workflowSecrets): mixed
    {
        $this->availableSecrets = $workflowSecrets;
        $this->applyToolInput($toolInput);

        return $this->run();
    }

    abstract public function run(): mixed;

    protected function secret(string $secretName, mixed $defaultValue = null): mixed
    {
        if (array_key_exists($secretName, $this->availableSecrets)) {
            return $this->availableSecrets[$secretName];
        }

        if (func_num_args() > 1) {
            return $defaultValue;
        }

        throw new RuntimeException(sprintf('Secret `%s` is not available.', $secretName));
    }

    private function inputSchema(): array
    {
        $inputProperties = [];
        $requiredPropertyNames = [];

        foreach ($this->publicInputProperties() as $publicInputProperty) {
            $propertyName = $publicInputProperty->getName();
            $inputProperties[$propertyName] = $this->propertySchema($publicInputProperty);

            if ($this->isRequiredInputProperty($publicInputProperty)) {
                $requiredPropertyNames[] = $propertyName;
            }
        }

        $schema = [
            'type' => 'object',
            'properties' => $inputProperties,
            'additionalProperties' => false,
        ];

        if ($requiredPropertyNames !== []) {
            $schema['required'] = $requiredPropertyNames;
        }

        return $schema;
    }

    private function publicInputProperties(): array
    {
        $reflectionClass = new ReflectionClass($this);
        $publicInputProperties = [];

        foreach ($reflectionClass->getProperties(ReflectionProperty::IS_PUBLIC) as $publicProperty) {
            if ($publicProperty->isStatic()) {
                continue;
            }

            $publicInputProperties[] = $publicProperty;
        }

        return $publicInputProperties;
    }

    private function propertySchema(ReflectionProperty $publicInputProperty): array
    {
        $schemaOverrideAttribute = $publicInputProperty->getAttributes(InputSchema::class)[0] ?? null;

        if ($schemaOverrideAttribute !== null) {
            $schemaOverride = $schemaOverrideAttribute->newInstance();

            return $this->withNullablePropertyType($schemaOverride->schema, $publicInputProperty);
        }

        $arrayTypeAttribute = $publicInputProperty->getAttributes(InputArrayOf::class)[0] ?? null;

        if ($arrayTypeAttribute !== null) {
            $arrayTypeDefinition = $arrayTypeAttribute->newInstance();

            return $this->withNullablePropertyType(
                [
                    'type' => 'array',
                    'items' => $this->schemaFragmentFromTypeDefinition($arrayTypeDefinition->itemType),
                ],
                $publicInputProperty,
            );
        }

        $tupleTypeAttribute = $publicInputProperty->getAttributes(InputTuple::class)[0] ?? null;

        if ($tupleTypeAttribute !== null) {
            $tupleTypeDefinition = $tupleTypeAttribute->newInstance();
            $prefixItems = [];

            foreach ($tupleTypeDefinition->itemTypes as $tupleItemTypeDefinition) {
                $prefixItems[] = $this->schemaFragmentFromTypeDefinition($tupleItemTypeDefinition);
            }

            return $this->withNullablePropertyType(
                [
                    'type' => 'array',
                    'prefixItems' => $prefixItems,
                    'items' => $tupleTypeDefinition->allowAdditionalItems,
                ],
                $publicInputProperty,
            );
        }

        return $this->withNullablePropertyType(
            $this->schemaFromReflectionType($publicInputProperty->getType()),
            $publicInputProperty,
        );
    }

    private function withNullablePropertyType(array $schema, ReflectionProperty $publicInputProperty): array
    {
        $propertyType = $publicInputProperty->getType();

        if ($propertyType === null || !$propertyType->allowsNull()) {
            return $schema;
        }

        $typeField = $schema['type'] ?? null;

        if (is_string($typeField) && $typeField !== 'null') {
            $schema['type'] = [$typeField, 'null'];

            return $schema;
        }

        if (is_array($typeField) && !in_array('null', $typeField, true)) {
            $typeField[] = 'null';
            $schema['type'] = $typeField;
        }

        return $schema;
    }

    private function schemaFromReflectionType(?ReflectionType $reflectionType): array
    {
        if ($reflectionType === null) {
            return [];
        }

        if ($reflectionType instanceof ReflectionNamedType) {
            $schemaTypeName = self::jsonSchemaTypeName($reflectionType->getName());

            if ($schemaTypeName === null) {
                return [];
            }

            return ['type' => $schemaTypeName];
        }

        if ($reflectionType instanceof ReflectionUnionType) {
            $schemaTypeNames = [];

            foreach ($reflectionType->getTypes() as $unionType) {
                if (!$unionType instanceof ReflectionNamedType) {
                    continue;
                }

                $schemaTypeName = self::jsonSchemaTypeName($unionType->getName());

                if ($schemaTypeName === null) {
                    continue;
                }

                if (!in_array($schemaTypeName, $schemaTypeNames, true)) {
                    $schemaTypeNames[] = $schemaTypeName;
                }
            }

            if ($schemaTypeNames === []) {
                return [];
            }

            if (count($schemaTypeNames) === 1) {
                return ['type' => $schemaTypeNames[0]];
            }

            return ['type' => $schemaTypeNames];
        }

        return [];
    }

    private function schemaFragmentFromTypeDefinition(mixed $typeDefinition): array
    {
        if (is_string($typeDefinition)) {
            $schemaTypeName = self::jsonSchemaTypeName($typeDefinition) ?? $typeDefinition;

            return ['type' => $schemaTypeName];
        }

        if (is_array($typeDefinition)) {
            return $typeDefinition;
        }

        throw new InvalidArgumentException('Type definitions in tool attributes must be strings or schema arrays.');
    }

    private static function jsonSchemaTypeName(string $phpTypeName): ?string
    {
        return match ($phpTypeName) {
            'int', 'integer' => 'integer',
            'float', 'double', 'number' => 'number',
            'string' => 'string',
            'bool', 'boolean' => 'boolean',
            'array' => 'array',
            'object' => 'object',
            'null' => 'null',
            default => null,
        };
    }

    private function isRequiredInputProperty(ReflectionProperty $publicInputProperty): bool
    {
        if ($publicInputProperty->hasDefaultValue()) {
            return false;
        }

        $propertyType = $publicInputProperty->getType();

        if ($propertyType === null) {
            return false;
        }

        return !$propertyType->allowsNull();
    }

    private function applyToolInput(array $toolInput): void
    {
        foreach ($this->publicInputProperties() as $publicInputProperty) {
            $propertyName = $publicInputProperty->getName();

            if (array_key_exists($propertyName, $toolInput)) {
                try {
                    $publicInputProperty->setValue($this, $toolInput[$propertyName]);
                } catch (Throwable $throwable) {
                    throw new InvalidArgumentException(
                        sprintf('Invalid value for tool input `%s` in `%s`: %s', $propertyName, $this->name(), $throwable->getMessage()),
                        0,
                        $throwable,
                    );
                }

                continue;
            }

            if ($publicInputProperty->hasDefaultValue()) {
                continue;
            }

            $propertyType = $publicInputProperty->getType();

            if ($propertyType !== null && $propertyType->allowsNull()) {
                $publicInputProperty->setValue($this, null);

                continue;
            }

            if ($propertyType === null) {
                continue;
            }

            throw new InvalidArgumentException(sprintf('Missing required tool input `%s` for `%s`.', $propertyName, $this->name()));
        }
    }

    private static function defaultToolName(string $className): string
    {
        $classNameSegments = explode('\\', $className);
        $shortClassName = end($classNameSegments);

        if (!is_string($shortClassName) || $shortClassName === '') {
            return 'tool';
        }

        $snakeCaseName = preg_replace('/(?<!^)[A-Z]/', '_$0', $shortClassName);

        if (!is_string($snakeCaseName)) {
            return strtolower($shortClassName);
        }

        return strtolower($snakeCaseName);
    }
}
