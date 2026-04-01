<?php

declare(strict_types = 1);

namespace EngineAi\Ffi;

use BackedEnum;
use EngineAi\Ffi\Attributes\Description;
use ReflectionClass;
use ReflectionMethod;
use ReflectionNamedType;
use ReflectionProperty;
use ReflectionType;
use ReflectionUnionType;
use UnitEnum;

final class ToolSchemaReflector
{
    private const DATA_COLLECTION_OF_ATTRIBUTE = 'Spatie\\LaravelData\\Attributes\\DataCollectionOf';
    private const MAP_OUTPUT_NAME_ATTRIBUTE = 'Spatie\\LaravelData\\Attributes\\MapOutputName';
    private const SNAKE_CASE_MAPPER = 'Spatie\\LaravelData\\Mappers\\SnakeCaseMapper';

    public static function fromToolInput(Tool $tool): ?array
    {
        $inputType = $tool->inputType();

        if ($inputType === null) {
            return null;
        }

        return self::classToSchema($inputType, []);
    }

    public static function fromToolExecute(Tool $tool): ?array
    {
        $executeMethod = new ReflectionMethod($tool, 'execute');

        return self::fromType($executeMethod->getReturnType());
    }

    private static function fromType(?ReflectionType $type, array $visitedClasses = []): ?array
    {
        if ($type === null) {
            return null;
        }

        if ($type instanceof ReflectionUnionType) {

            $variants = [];

            foreach ($type->getTypes() as $unionType) {

                $variant = self::fromType($unionType, $visitedClasses);

                if ($variant === null) {
                    return null;
                }

                $variants[] = $variant;

            }

            return [
                'anyOf' => $variants,
            ];

        }

        if (!$type instanceof ReflectionNamedType) {
            return null;
        }

        $namedType = $type->getName();

        if ($namedType === 'mixed') {
            return null;
        }

        $schema = self::namedTypeToSchema($type, $visitedClasses);

        if ($schema === null) {
            return null;
        }

        if ($type->allowsNull() && $namedType !== 'null') {

            return [
                'anyOf' => [
                    $schema,
                    [ 'type' => 'null' ],
                ],
            ];

        }

        return $schema;
    }

    private static function namedTypeToSchema(ReflectionNamedType $type, array $visitedClasses): ?array
    {
        if (!$type->isBuiltin()) {
            return self::classToSchema($type->getName(), $visitedClasses);
        }

        return match ($type->getName()) {
            'string' => Schema::string(),
            'int' => Schema::integer(),
            'float' => Schema::number(),
            'bool' => Schema::boolean(),
            'array' => Schema::array([]),
            'null' => [ 'type' => 'null' ],
            default => null,
        };
    }

    private static function classToSchema(string $className, array $visitedClasses): ?array
    {
        if (in_array($className, $visitedClasses, true)) {
            return [ 'type' => 'object' ];
        }

        $reflectionClass = new ReflectionClass($className);
        $visitedClasses[] = $className;

        if ($reflectionClass->isSubclassOf(BackedEnum::class)) {
            return self::backedEnumToSchema($reflectionClass);
        }

        if ($reflectionClass->isSubclassOf(UnitEnum::class)) {
            return self::unitEnumToSchema($reflectionClass);
        }

        if ($reflectionClass->hasMethod('outputSchema')) {

            $schemaMethod = $reflectionClass->getMethod('outputSchema');

            if ($schemaMethod->isStatic() && $schemaMethod->isPublic() && $schemaMethod->getNumberOfRequiredParameters() === 0) {

                $schema = $schemaMethod->invoke(null);

                if (is_array($schema)) {
                    return $schema;
                }

            }

        }

        if ($reflectionClass->hasMethod('schema')) {

            $schemaMethod = $reflectionClass->getMethod('schema');

            if ($schemaMethod->isStatic() && $schemaMethod->isPublic() && $schemaMethod->getNumberOfRequiredParameters() === 0) {

                $schema = $schemaMethod->invoke(null);

                if (is_array($schema)) {
                    return $schema;
                }

            }

        }

        return self::objectPropertiesToSchema($reflectionClass, $visitedClasses);
    }

    private static function objectPropertiesToSchema(ReflectionClass $reflectionClass, array $visitedClasses): ?array
    {
        $properties = $reflectionClass->getProperties(ReflectionProperty::IS_PUBLIC);
        $classDescription = self::reflectorDescription($reflectionClass);

        if ($properties === []) {

            $schema = [
                'type' => 'object',
                'properties' => [],
                'required' => [],
                'additionalProperties' => false,
            ];

            if ($classDescription !== null) {
                $schema[ 'description' ] = $classDescription;
            }

            return $schema;

        }

        $schemaProperties = [];
        $requiredProperties = [];
        $mapOutputToSnakeCase = self::classUsesSnakeCaseOutput($reflectionClass);

        foreach ($properties as $property) {

            if ($property->isStatic()) {
                continue;
            }

            $propertyType = $property->getType();
            $propertySchema = self::dataCollectionPropertyToSchema($property, $visitedClasses)
                ?? self::fromType($propertyType, $visitedClasses);

            if ($propertySchema === null) {
                return null;
            }

            $propertyDescription = self::reflectorDescription($property);

            if ($propertyDescription !== null && !array_key_exists('description', $propertySchema)) {
                $propertySchema[ 'description' ] = $propertyDescription;
            }

            $propertySchemaName = $mapOutputToSnakeCase
                ? self::snakeCase($property->getName())
                : $property->getName();

            $schemaProperties[ $propertySchemaName ] = $propertySchema;

            if (
                $propertyType !== null
                && !$propertyType->allowsNull()
                && !$property->hasDefaultValue()
            ) {
                $requiredProperties[] = $propertySchemaName;
            }

        }

        $schema = [
            'type' => 'object',
            'properties' => $schemaProperties,
            'required' => $requiredProperties,
            'additionalProperties' => false,
        ];

        if ($classDescription !== null) {
            $schema[ 'description' ] = $classDescription;
        }

        return $schema;
    }

    private static function classUsesSnakeCaseOutput(ReflectionClass $reflectionClass): bool
    {
        for ($currentClass = $reflectionClass; $currentClass !== false; $currentClass = $currentClass->getParentClass()) {

            $mapOutputAttribute = $currentClass->getAttributes(self::MAP_OUTPUT_NAME_ATTRIBUTE)[ 0 ] ?? null;

            if ($mapOutputAttribute === null) {
                continue;
            }

            $attributeArguments = $mapOutputAttribute->getArguments();
            $mapper = $attributeArguments[ 0 ]
                ?? $attributeArguments[ 'output' ]
                ?? $attributeArguments[ 'mapper' ]
                ?? null;

            return $mapper === self::SNAKE_CASE_MAPPER;

        }

        return false;
    }

    private static function snakeCase(string $value): string
    {
        return strtolower((string) preg_replace('/([a-z0-9])([A-Z])/', '$1_$2', $value));
    }

    private static function dataCollectionPropertyToSchema(ReflectionProperty $property, array $visitedClasses): ?array
    {
        $collectionAttribute = $property->getAttributes(self::DATA_COLLECTION_OF_ATTRIBUTE)[ 0 ] ?? null;

        if ($collectionAttribute === null) {
            return null;
        }

        $collectionAttributeArguments = $collectionAttribute->getArguments();

        $itemClass = $collectionAttributeArguments[ 0 ]
            ?? $collectionAttributeArguments[ 'class' ]
            ?? null;

        if (!is_string($itemClass) || $itemClass === '' || !class_exists($itemClass)) {
            return Schema::array([]);
        }

        $itemSchema = self::classToSchema($itemClass, $visitedClasses);

        if ($itemSchema === null) {
            return Schema::array([]);
        }

        return Schema::array($itemSchema);
    }

    private static function backedEnumToSchema(ReflectionClass $reflectionClass): array
    {
        $enumCases = $reflectionClass->getCases();

        if ($enumCases === []) {
            return [ 'type' => 'string', 'enum' => [] ];
        }

        $firstCase = $enumCases[ 0 ]->getValue();
        $firstCaseValue = $firstCase instanceof BackedEnum ? $firstCase->value : null;
        $enumType = is_int($firstCaseValue) ? 'integer' : 'string';

        return [
            'type' => $enumType,
            'enum' => array_map(
                static fn ($case) => $case->getValue()->value,
                $enumCases,
            ),
        ];
    }

    private static function unitEnumToSchema(ReflectionClass $reflectionClass): array
    {
        return [
            'type' => 'string',
            'enum' => array_map(
                static fn ($case) => $case->getValue()->name,
                $reflectionClass->getCases(),
            ),
        ];
    }

    private static function reflectorDescription(ReflectionClass|ReflectionProperty $reflector): ?string
    {
        $descriptionAttribute = $reflector->getAttributes(Description::class)[ 0 ] ?? null;

        if ($descriptionAttribute === null) {
            return null;
        }

        $description = trim($descriptionAttribute->newInstance()->value);

        return $description === '' ? null : $description;
    }
}
