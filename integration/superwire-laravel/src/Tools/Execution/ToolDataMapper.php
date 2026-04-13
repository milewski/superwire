<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools\Execution;

use InvalidArgumentException;
use LogicException;
use ReflectionAttribute;
use ReflectionClass;
use ReflectionEnum;
use ReflectionException;
use ReflectionNamedType;
use ReflectionParameter;
use ReflectionProperty;
use ReflectionType;
use ReflectionUnionType;
use Spatie\LaravelData\Attributes\DataCollectionOf;
use Spatie\LaravelData\Data;
use Spatie\LaravelData\DataCollection;
use Swaggest\JsonSchema\Schema;
use Throwable;

final class ToolDataMapper
{
    /**
     * @param class-string<Data> $toolDataClassName
     *
     * @throws ReflectionException
     */
    public function schemaFromToolDataClass(string $toolDataClassName): Schema
    {
        return $this->buildSchemaFromToolDataClass($toolDataClassName, []);
    }

    /**
     * @param class-string<Data> $toolDataClassName
     * @param array<string, mixed> $payload
     */
    public function hydrateToolDataClass(string $toolDataClassName, array $payload, string $payloadContext): Data
    {
        if (!is_a($toolDataClassName, Data::class, true)) {

            throw new InvalidArgumentException(sprintf(
                'tool data class `%s` must extend `%s`',
                $toolDataClassName,
                Data::class,
            ));

        }

        try {

            return $toolDataClassName::from($this->normalizePayloadForToolDataClass($toolDataClassName, $payload));

        } catch (Throwable $throwable) {

            throw new InvalidArgumentException(
                message: sprintf('failed to map %s payload into `%s`: %s', $payloadContext, $toolDataClassName, $throwable->getMessage()),
                previous: $throwable,
            );

        }
    }

    /**
     * @param class-string<Data> $toolDataClassName
     * @param array<string, mixed> $payload
     *
     * @return array<string, mixed>
     */
    private function normalizePayloadForToolDataClass(string $toolDataClassName, array $payload): array
    {
        $toolDataReflectionClass = new ReflectionClass($toolDataClassName);

        foreach ($this->constructorParameters($toolDataReflectionClass) as $constructorParameter) {

            $parameterName = $constructorParameter->getName();

            if (!array_key_exists($parameterName, $payload)) {
                continue;
            }

            $parameterValue = $payload[ $parameterName ];
            $parameterType = $constructorParameter->getType();

            $dataClassName = $this->resolveDataTypeClassName($parameterType);

            if (is_string($dataClassName) && is_array($parameterValue)) {

                $payload[ $parameterName ] = $this->normalizePayloadForToolDataClass($dataClassName, $parameterValue);
                continue;

            }

            $dataCollectionClassName = $this->resolveDataCollectionTypeClassName($parameterType);
            $dataCollectionItemClassName = $this->resolveDataCollectionItemClassName($toolDataReflectionClass, $constructorParameter);

            if (is_string($dataCollectionClassName) && is_string($dataCollectionItemClassName) && is_array($parameterValue)) {
                $payload[ $parameterName ] = $this->normalizeDataCollectionPayload($dataCollectionItemClassName, $parameterValue);
            }

        }

        return $payload;
    }

    /**
     * @param class-string<Data> $dataCollectionItemClassName
     * @param array<int|string, mixed> $items
     *
     * @return array<int, mixed>
     */
    private function normalizeDataCollectionPayload(string $dataCollectionItemClassName, array $items): array
    {
        $normalizedItems = $items;

        if (!array_is_list($normalizedItems) && $this->isKeyValueDataClass($dataCollectionItemClassName)) {

            $normalizedItems = collect($normalizedItems)
                ->map(static fn (mixed $value, string|int $key): array => [
                    'key' => (string) $key,
                    'value' => $value,
                ])
                ->values()
                ->all();

        }

        return array_values(array_map(function (mixed $item) use ($dataCollectionItemClassName): mixed {

            if (is_array($item)) {
                return $this->normalizePayloadForToolDataClass($dataCollectionItemClassName, $item);
            }

            if ($this->isSingleValueDataClass($dataCollectionItemClassName)) {
                return [ 'value' => $item ];
            }

            return $item;

        }, $normalizedItems));
    }

    private function resolveDataTypeClassName(?ReflectionType $reflectionType): ?string
    {
        if ($reflectionType instanceof ReflectionNamedType) {

            $typeName = $reflectionType->getName();

            return $reflectionType->isBuiltin() || !is_a($typeName, Data::class, true)
                ? null
                : $typeName;

        }

        if ($reflectionType instanceof ReflectionUnionType) {

            foreach ($reflectionType->getTypes() as $unionMemberType) {

                if (!$unionMemberType instanceof ReflectionNamedType) {
                    continue;
                }

                $typeName = $unionMemberType->getName();

                if (!$unionMemberType->isBuiltin() && is_a($typeName, Data::class, true)) {
                    return $typeName;
                }

            }

        }

        return null;
    }

    private function resolveDataCollectionTypeClassName(?ReflectionType $reflectionType): ?string
    {
        if ($reflectionType instanceof ReflectionNamedType) {

            $typeName = $reflectionType->getName();

            return $reflectionType->isBuiltin() || !is_a($typeName, DataCollection::class, true)
                ? null
                : $typeName;

        }

        if ($reflectionType instanceof ReflectionUnionType) {

            foreach ($reflectionType->getTypes() as $unionMemberType) {

                if (!$unionMemberType instanceof ReflectionNamedType) {
                    continue;
                }

                $typeName = $unionMemberType->getName();

                if (!$unionMemberType->isBuiltin() && is_a($typeName, DataCollection::class, true)) {
                    return $typeName;
                }

            }

        }

        return null;
    }

    /**
     * @param class-string<Data> $className
     */
    private function isSingleValueDataClass(string $className): bool
    {
        $reflectionClass = new ReflectionClass($className);
        $constructor = $reflectionClass->getConstructor();

        if ($constructor === null) {
            return false;
        }

        $parameters = $constructor->getParameters();

        return count($parameters) === 1 && $parameters[ 0 ]->getName() === 'value';
    }

    /**
     * @param class-string<Data> $className
     */
    private function isKeyValueDataClass(string $className): bool
    {
        $reflectionClass = new ReflectionClass($className);
        $constructor = $reflectionClass->getConstructor();

        if ($constructor === null) {
            return false;
        }

        $parameterNames = array_map(
            static fn (ReflectionParameter $reflectionParameter): string => $reflectionParameter->getName(),
            $constructor->getParameters(),
        );

        return $parameterNames === [ 'key', 'value' ];
    }

    /**
     * @return array<string, mixed>
     */
    public function extractToolDataPayload(Data $toolData): array
    {
        if (!$toolData instanceof Data) {

            throw new InvalidArgumentException(sprintf(
                'tool data object `%s` must extend `%s`', $toolData::class, Data::class,
            ));

        }

        $payload = $toolData->toArray();

        if (!is_array($payload)) {

            throw new InvalidArgumentException(sprintf(
                'tool data object `%s` must serialize into array payload', $toolData::class,
            ));

        }

        return $payload;
    }

    /**
     * @param class-string<Data> $toolDataClassName
     * @param list<class-string<Data>> $visitedClassNames
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
        $toolDataSchema->description = $this->resolveClassDescription($toolDataReflectionClass);
        $requiredPropertyNames = [];

        foreach ($this->constructorParameters($toolDataReflectionClass) as $constructorParameter) {

            $propertySchema = $this->schemaFromType(
                reflectionType: $constructorParameter->getType(),
                visitedClassNames: $visitedClassNames,
                dataCollectionItemClassName: $this->resolveDataCollectionItemClassName(
                    toolDataReflectionClass: $toolDataReflectionClass,
                    constructorParameter: $constructorParameter,
                ),
            );

            $propertySchema->description = $this->resolveParameterDescription(
                toolDataReflectionClass: $toolDataReflectionClass,
                constructorParameter: $constructorParameter,
            );

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
     * @param list<class-string<Data>> $visitedClassNames
     *
     * @throws ReflectionException
     */
    private function schemaFromType(
        ?ReflectionType $reflectionType,
        array $visitedClassNames,
        ?string $dataCollectionItemClassName,
    ): Schema
    {
        if ($reflectionType === null) {
            return Schema::create();
        }

        if ($reflectionType instanceof ReflectionNamedType) {
            return $this->schemaFromNamedType($reflectionType, $visitedClassNames, $dataCollectionItemClassName);
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

                $unionSchemas[] = $this->schemaFromNamedType(
                    $unionMemberType,
                    $visitedClassNames,
                    $dataCollectionItemClassName,
                );

            }

            $unionTypeSchema->anyOf = $unionSchemas;

            return $unionTypeSchema;

        }

        throw new LogicException('unsupported reflection type for tool schema generation');
    }

    /**
     * @param list<class-string<Data>> $visitedClassNames
     *
     * @throws ReflectionException
     */
    private function schemaFromNamedType(
        ReflectionNamedType $namedType,
        array $visitedClassNames,
        ?string $dataCollectionItemClassName,
    ): Schema
    {
        if ($namedType->isBuiltin()) {
            return $this->schemaFromBuiltinType($namedType->getName());
        }

        $typeClassName = $namedType->getName();

        if (is_a($typeClassName, Data::class, true)) {
            return $this->buildSchemaFromToolDataClass($typeClassName, $visitedClassNames);
        }

        if (is_a($typeClassName, DataCollection::class, true)) {

            if (!is_string($dataCollectionItemClassName) || $dataCollectionItemClassName === '') {

                throw new LogicException(sprintf(
                    'unsupported data collection type `%s` without %s attribute',
                    $typeClassName,
                    DataCollectionOf::class,
                ));

            }

            return $this->arraySchema(
                $this->buildSchemaFromToolDataClass($dataCollectionItemClassName, $visitedClassNames),
            );

        }

        if (enum_exists($typeClassName)) {
            return $this->schemaFromEnumType($typeClassName);
        }

        throw new LogicException(sprintf(
            'unsupported non-data type `%s` in tool schema generation', $typeClassName,
        ));
    }

    /**
     * @param class-string $enumClassName
     */
    private function schemaFromEnumType(string $enumClassName): Schema
    {
        $enumReflection = new ReflectionEnum($enumClassName);
        $enumSchema = Schema::create();

        if ($enumReflection->isBacked()) {

            $backingType = $enumReflection->getBackingType()?->getName();
            $enumSchema->type = $backingType === 'int' ? 'integer' : 'string';
            $enumSchema->enum = array_map(
                static fn ($case) => $case->getBackingValue(),
                $enumReflection->getCases(),
            );

            return $enumSchema;

        }

        $enumSchema->type = 'string';
        $enumSchema->enum = array_map(
            static fn ($case) => $case->getName(),
            $enumReflection->getCases(),
        );

        return $enumSchema;
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

    private function arraySchema(?Schema $itemsSchema = null): Schema
    {
        $arraySchema = Schema::create();
        $arraySchema->type = 'array';

        if ($itemsSchema !== null) {
            $arraySchema->items = $itemsSchema;
        }

        return $arraySchema;
    }

    /**
     * @param ReflectionClass<Data> $toolDataReflectionClass
     * @return class-string<Data>|null
     */
    private function resolveDataCollectionItemClassName(
        ReflectionClass $toolDataReflectionClass,
        ReflectionParameter $constructorParameter,
    ): ?string
    {
        if (!$toolDataReflectionClass->hasProperty($constructorParameter->getName())) {
            return null;
        }

        $property = $toolDataReflectionClass->getProperty($constructorParameter->getName());
        $attributes = $property->getAttributes(DataCollectionOf::class);

        if ($attributes === []) {
            return null;
        }

        /** @var DataCollectionOf $dataCollectionOf */
        $dataCollectionOf = $attributes[ 0 ]->newInstance();

        return $dataCollectionOf->class;
    }

    /**
     * @param ReflectionClass<Data> $toolDataReflectionClass
     */
    private function resolveClassDescription(ReflectionClass $toolDataReflectionClass): ?string
    {
        return $this->extractDescriptionFromAttributes($toolDataReflectionClass->getAttributes());
    }

    /**
     * @param ReflectionClass<Data> $toolDataReflectionClass
     */
    private function resolveParameterDescription(
        ReflectionClass $toolDataReflectionClass,
        ReflectionParameter $constructorParameter,
    ): ?string {
        $parameterDescription = $this->extractDescriptionFromAttributes($constructorParameter->getAttributes());

        if (is_string($parameterDescription)) {
            return $parameterDescription;
        }

        $property = $this->findPropertyForConstructorParameter($toolDataReflectionClass, $constructorParameter);

        if (!$property instanceof ReflectionProperty) {
            return null;
        }

        return $this->extractDescriptionFromAttributes($property->getAttributes());
    }

    /**
     * @param ReflectionClass<Data> $toolDataReflectionClass
     */
    private function findPropertyForConstructorParameter(
        ReflectionClass $toolDataReflectionClass,
        ReflectionParameter $constructorParameter,
    ): ?ReflectionProperty {
        if (!$toolDataReflectionClass->hasProperty($constructorParameter->getName())) {
            return null;
        }

        return $toolDataReflectionClass->getProperty($constructorParameter->getName());
    }

    /**
     * @param array<int, ReflectionAttribute<object>> $attributes
     */
    private function extractDescriptionFromAttributes(array $attributes): ?string
    {
        foreach ($attributes as $attribute) {

            if (class_basename($attribute->getName()) !== 'Description') {
                continue;
            }

            $arguments = $attribute->getArguments();

            if (isset($arguments[ 0 ]) && is_string($arguments[ 0 ]) && $arguments[ 0 ] !== '') {
                return $arguments[ 0 ];
            }

            $attributeInstance = $attribute->newInstance();

            foreach ([ 'text', 'description', 'value' ] as $propertyName) {

                $propertyValue = data_get($attributeInstance, $propertyName);

                if (is_string($propertyValue) && $propertyValue !== '') {
                    return $propertyValue;
                }

            }

        }

        return null;
    }

    /**
     * @param ReflectionClass<Data> $toolDataReflectionClass
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
