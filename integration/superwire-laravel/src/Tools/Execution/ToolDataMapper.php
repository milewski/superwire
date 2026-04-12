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

            return $toolDataClassName::from($payload);

        } catch (Throwable $throwable) {

            throw new InvalidArgumentException(
                message: sprintf('failed to map %s payload into `%s`: %s', $payloadContext, $toolDataClassName, $throwable->getMessage()),
                previous: $throwable,
            );

        }
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

        throw new LogicException(sprintf(
            'unsupported non-data type `%s` in tool schema generation', $typeClassName,
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
