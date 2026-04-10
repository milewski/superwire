<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools\Execution;

use InvalidArgumentException;
use LogicException;
use ReflectionClass;
use ReflectionException;
use ReflectionNamedType;
use ReflectionParameter;
use ReflectionType;
use ReflectionUnionType;
use Superwire\Laravel\Contracts\ToolData;
use Swaggest\JsonSchema\Schema;

final class ToolDataMapper
{
    /**
     * @param class-string<ToolData> $toolDataClassName
     *
     * @throws ReflectionException
     */
    public function schemaFromToolDataClass(string $toolDataClassName): Schema
    {
        return $this->buildSchemaFromToolDataClass($toolDataClassName, []);
    }

    /**
     * @param class-string<ToolData> $toolDataClassName
     * @param array<string, mixed> $payload
     *
     * @throws ReflectionException
     */
    public function hydrateToolDataClass(string $toolDataClassName, array $payload, string $payloadContext): ToolData
    {
        $toolDataReflectionClass = new ReflectionClass($toolDataClassName);
        $constructorParameters = $this->constructorParameters($toolDataReflectionClass);
        $expectedFieldNames = array_map(
            static fn (ReflectionParameter $constructorParameter): string => $constructorParameter->getName(),
            $constructorParameters,
        );

        foreach ($payload as $fieldName => $fieldValue) {

            if (!is_string($fieldName) || !in_array($fieldName, $expectedFieldNames, true)) {

                throw new InvalidArgumentException(sprintf(
                    '%s payload contains unknown field `%s` for `%s`',
                    $payloadContext,
                    (string) $fieldName,
                    $toolDataClassName,
                ));

            }

        }

        $constructorArguments = [];

        foreach ($constructorParameters as $constructorParameter) {

            $fieldName = $constructorParameter->getName();

            if (array_key_exists($fieldName, $payload)) {

                $constructorArguments[ $fieldName ] = $this->payloadValueForType(
                    $payload[ $fieldName ],
                    $constructorParameter->getType(),
                    sprintf('%s.%s', $payloadContext, $fieldName),
                );

                continue;

            }

            if ($constructorParameter->isDefaultValueAvailable()) {

                $constructorArguments[ $fieldName ] = $constructorParameter->getDefaultValue();

                continue;

            }

            throw new InvalidArgumentException(sprintf(
                '%s payload is missing required field `%s` for `%s`',
                $payloadContext,
                $fieldName,
                $toolDataClassName,
            ));

        }

        /** @var ToolData $toolDataInstance */
        $toolDataInstance = $toolDataReflectionClass->newInstanceArgs($constructorArguments);

        return $toolDataInstance;
    }

    /**
     * @return array<string, mixed>
     *
     * @throws ReflectionException
     */
    public function extractToolDataPayload(ToolData $toolData): array
    {
        $toolDataReflectionClass = new ReflectionClass($toolData::class);
        $payload = [];

        foreach ($this->constructorParameters($toolDataReflectionClass) as $constructorParameter) {

            $propertyName = $constructorParameter->getName();

            if (!$toolDataReflectionClass->hasProperty($propertyName)) {
                continue;
            }

            $toolDataProperty = $toolDataReflectionClass->getProperty($propertyName);

            if (!$toolDataProperty->isPublic()) {
                $toolDataProperty->setAccessible(true);
            }

            $payload[ $propertyName ] = $this->payloadValueFromToolData($toolDataProperty->getValue($toolData));

        }

        return $payload;
    }

    /**
     * @param class-string<ToolData> $toolDataClassName
     * @param list<class-string<ToolData>> $visitedClassNames
     *
     * @throws ReflectionException
     */
    private function buildSchemaFromToolDataClass(string $toolDataClassName, array $visitedClassNames): Schema
    {
        if (in_array($toolDataClassName, $visitedClassNames, true)) {
            throw new LogicException(sprintf('recursive tool data schema detected for `%s`', $toolDataClassName));
        }

        $visitedClassNames[] = $toolDataClassName;
        $toolDataReflectionClass = new ReflectionClass($toolDataClassName);
        $toolDataSchema = Schema::object();
        $toolDataSchema->additionalProperties = false;
        $requiredPropertyNames = [];

        foreach ($this->constructorParameters($toolDataReflectionClass) as $constructorParameter) {

            $propertySchema = $this->schemaFromType($constructorParameter->getType(), $visitedClassNames);
            $toolDataSchema->setProperty($constructorParameter->getName(), $propertySchema);

            if (!$constructorParameter->isOptional()) {
                $requiredPropertyNames[] = $constructorParameter->getName();
            }

        }

        if ($requiredPropertyNames !== []) {
            $toolDataSchema->required = $requiredPropertyNames;
        }

        return $toolDataSchema;
    }

    /**
     * @param list<class-string<ToolData>> $visitedClassNames
     */
    private function schemaFromType(?ReflectionType $reflectionType, array $visitedClassNames): Schema
    {
        if ($reflectionType === null) {
            return Schema::create();
        }

        if ($reflectionType instanceof ReflectionNamedType) {
            return $this->schemaFromNamedType($reflectionType, $visitedClassNames);
        }

        if ($reflectionType instanceof ReflectionUnionType) {

            $unionTypeSchema = Schema::create();
            $unionSchemas = [];

            foreach ($reflectionType->getTypes() as $unionMemberType) {

                if ($unionMemberType instanceof ReflectionNamedType && $unionMemberType->getName() === 'null') {

                    $nullSchema = Schema::create();
                    $nullSchema->type = 'null';
                    $unionSchemas[] = $nullSchema;

                    continue;

                }

                if (!$unionMemberType instanceof ReflectionNamedType) {
                    throw new LogicException('unsupported reflection union member type');
                }

                $unionSchemas[] = $this->schemaFromNamedType($unionMemberType, $visitedClassNames);

            }

            $unionTypeSchema->anyOf = $unionSchemas;

            return $unionTypeSchema;

        }

        throw new LogicException('unsupported reflection type for tool schema generation');
    }

    /**
     * @param list<class-string<ToolData>> $visitedClassNames
     *
     * @throws ReflectionException
     */
    private function schemaFromNamedType(ReflectionNamedType $namedType, array $visitedClassNames): Schema
    {
        if ($namedType->isBuiltin()) {
            return $this->schemaFromBuiltinType($namedType->getName());
        }

        $typeClassName = $namedType->getName();

        if (is_a($typeClassName, ToolData::class, true)) {
            return $this->buildSchemaFromToolDataClass($typeClassName, $visitedClassNames);
        }

        throw new LogicException(sprintf(
            'unsupported non-tool-data type `%s` in tool schema generation',
            $typeClassName,
        ));
    }

    private function schemaFromBuiltinType(string $builtinTypeName): Schema
    {
        return match ($builtinTypeName) {
            'string' => Schema::string(),
            'int' => Schema::integer(),
            'float' => Schema::number(),
            'bool' => Schema::boolean(),
            'array' => $this->arraySchema(),
            'object' => Schema::object(),
            'mixed' => Schema::create(),
            default => throw new LogicException(sprintf('unsupported builtin type `%s` for tool schema generation', $builtinTypeName)),
        };
    }

    private function arraySchema(): Schema
    {
        $arraySchema = Schema::create();
        $arraySchema->type = 'array';

        return $arraySchema;
    }

    private function payloadValueForType(mixed $payloadValue, ?ReflectionType $reflectionType, string $payloadPath): mixed
    {
        if ($reflectionType === null) {
            return $payloadValue;
        }

        if ($reflectionType instanceof ReflectionNamedType) {
            return $this->payloadValueForNamedType($payloadValue, $reflectionType, $payloadPath);
        }

        if ($reflectionType instanceof ReflectionUnionType) {

            foreach ($reflectionType->getTypes() as $unionMemberType) {

                if (!$unionMemberType instanceof ReflectionNamedType) {
                    continue;
                }

                if ($unionMemberType->getName() === 'null' && $payloadValue === null) {
                    return null;
                }

                if ($unionMemberType->getName() === 'null') {
                    continue;
                }

                try {

                    return $this->payloadValueForNamedType($payloadValue, $unionMemberType, $payloadPath);

                } catch (InvalidArgumentException $invalidArgumentException) {

                    continue;

                }

            }

            throw new InvalidArgumentException(sprintf(
                'field `%s` does not match any supported union type',
                $payloadPath,
            ));

        }

        throw new InvalidArgumentException(sprintf('unsupported reflection type for `%s`', $payloadPath));
    }

    private function payloadValueForNamedType(mixed $payloadValue, ReflectionNamedType $namedType, string $payloadPath): mixed
    {
        if ($payloadValue === null && $namedType->allowsNull()) {
            return null;
        }

        if ($namedType->isBuiltin()) {
            return $this->payloadValueForBuiltinType($payloadValue, $namedType->getName(), $payloadPath);
        }

        $typeClassName = $namedType->getName();

        if (is_a($typeClassName, ToolData::class, true)) {

            if (!is_array($payloadValue)) {

                throw new InvalidArgumentException(sprintf(
                    'field `%s` must be an object payload for `%s`, received `%s`',
                    $payloadPath,
                    $typeClassName,
                    get_debug_type($payloadValue),
                ));

            }

            return $this->hydrateToolDataClass($typeClassName, $payloadValue, $payloadPath);

        }

        throw new InvalidArgumentException(sprintf(
            'field `%s` has unsupported type `%s`',
            $payloadPath,
            $typeClassName,
        ));
    }

    private function payloadValueForBuiltinType(mixed $payloadValue, string $builtinTypeName, string $payloadPath): mixed
    {
        return match ($builtinTypeName) {
            'string' => $this->assertPayloadType($payloadValue, 'string', $payloadPath),
            'int' => $this->assertPayloadType($payloadValue, 'integer', $payloadPath),
            'float' => $this->assertNumericPayloadType($payloadValue, $payloadPath),
            'bool' => $this->assertPayloadType($payloadValue, 'boolean', $payloadPath),
            'array' => $this->assertPayloadType($payloadValue, 'array', $payloadPath),
            'object' => $this->assertPayloadType($payloadValue, 'object', $payloadPath),
            'mixed' => $payloadValue,
            default => throw new InvalidArgumentException(sprintf(
                'field `%s` has unsupported builtin type `%s`',
                $payloadPath,
                $builtinTypeName,
            )),
        };
    }

    private function assertPayloadType(mixed $payloadValue, string $expectedTypeName, string $payloadPath): mixed
    {
        $hasExpectedType = match ($expectedTypeName) {
            'string' => is_string($payloadValue),
            'integer' => is_int($payloadValue),
            'boolean' => is_bool($payloadValue),
            'array' => is_array($payloadValue),
            'object' => is_object($payloadValue),
            default => false,
        };

        if ($hasExpectedType) {
            return $payloadValue;
        }

        throw new InvalidArgumentException(sprintf(
            'field `%s` must be %s, received `%s`',
            $payloadPath,
            $expectedTypeName,
            get_debug_type($payloadValue),
        ));
    }

    private function assertNumericPayloadType(mixed $payloadValue, string $payloadPath): float
    {
        if (is_float($payloadValue)) {
            return $payloadValue;
        }

        if (is_int($payloadValue)) {
            return (float) $payloadValue;
        }

        throw new InvalidArgumentException(sprintf(
            'field `%s` must be number, received `%s`',
            $payloadPath,
            get_debug_type($payloadValue),
        ));
    }

    private function payloadValueFromToolData(mixed $toolDataValue): mixed
    {
        if ($toolDataValue instanceof ToolData) {
            return $this->extractToolDataPayload($toolDataValue);
        }

        if (is_array($toolDataValue)) {

            $normalizedArray = [];

            foreach ($toolDataValue as $arrayKey => $arrayValue) {
                $normalizedArray[ $arrayKey ] = $this->payloadValueFromToolData($arrayValue);
            }

            return $normalizedArray;

        }

        if (is_string($toolDataValue) || is_int($toolDataValue) || is_float($toolDataValue) || is_bool($toolDataValue) || $toolDataValue === null) {
            return $toolDataValue;
        }

        throw new InvalidArgumentException(sprintf(
            'tool output contains unsupported value `%s`; only scalar, null, array, and tool data objects are supported',
            get_debug_type($toolDataValue),
        ));
    }

    /**
     * @param ReflectionClass<ToolData> $toolDataReflectionClass
     * @return list<ReflectionParameter>
     */
    private function constructorParameters(ReflectionClass $toolDataReflectionClass): array
    {
        $constructor = $toolDataReflectionClass->getConstructor();

        if ($constructor === null) {
            return [];
        }

        return $constructor->getParameters();
    }
}
