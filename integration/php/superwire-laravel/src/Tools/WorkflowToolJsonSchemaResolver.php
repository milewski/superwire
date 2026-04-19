<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools;

use ReflectionClass;
use ReflectionIntersectionType;
use ReflectionNamedType;
use ReflectionParameter;
use ReflectionType;
use ReflectionUnionType;
use RuntimeException;
use UnitEnum;

final class WorkflowToolJsonSchemaResolver
{
    /**
     * @param class-string<WorkflowToolArguments> $dataClass
     * @return array<string, mixed>
     */
    public static function forDataClass(string $dataClass): array
    {
        return (new self())->resolveDataClassSchema($dataClass, []);
    }

    /**
     * @param class-string $dataClass
     * @param list<class-string> $seenClasses
     * @return array<string, mixed>
     */
    private function resolveDataClassSchema(string $dataClass, array $seenClasses): array
    {
        if (in_array($dataClass, $seenClasses, true)) {
            return [
                'type' => 'object',
                'additionalProperties' => true,
            ];
        }

        if (!class_exists($dataClass)) {
            throw new RuntimeException("workflow tool argument class `{$dataClass}` does not exist");
        }

        $reflectionClass = new ReflectionClass($dataClass);
        $constructor = $reflectionClass->getConstructor();
        $properties = [];
        $required = [];
        $nextSeenClasses = [ ...$seenClasses, $dataClass ];

        if ($constructor !== null) {

            foreach ($constructor->getParameters() as $parameter) {

                $properties[ $parameter->getName() ] = $this->resolveTypeSchema($parameter, $parameter->getType(), $nextSeenClasses);

                if (!$parameter->isOptional() && !$parameter->allowsNull()) {
                    $required[] = $parameter->getName();
                }

            }

        }

        $schema = [
            'type' => 'object',
            'properties' => $properties,
            'additionalProperties' => false,
        ];

        if ($required !== []) {
            $schema[ 'required' ] = $required;
        }

        return $schema;
    }

    /**
     * @param list<class-string> $seenClasses
     * @return array<string, mixed>
     */
    private function resolveTypeSchema(ReflectionParameter $parameter, ?ReflectionType $reflectionType, array $seenClasses): array
    {
        if ($reflectionType === null) {
            return [];
        }

        if ($reflectionType instanceof ReflectionIntersectionType) {
            return [];
        }

        if ($reflectionType instanceof ReflectionUnionType) {

            $schemas = [];
            $allowsNull = false;

            foreach ($reflectionType->getTypes() as $unionedType) {

                if ($unionedType->getName() === 'null') {

                    $allowsNull = true;

                    continue;

                }

                $schemas[] = $this->resolveNamedTypeSchema($unionedType, $seenClasses);

            }

            if ($schemas === []) {
                return [];
            }

            if (count($schemas) === 1 && !$allowsNull) {
                return $schemas[ 0 ];
            }

            $normalizedSchemas = [
                ...$schemas,
            ];

            if ($allowsNull || $parameter->allowsNull()) {
                $normalizedSchemas[] = [ 'type' => 'null' ];
            }

            return [ 'anyOf' => $normalizedSchemas ];

        }

        $resolvedSchema = $this->resolveNamedTypeSchema($reflectionType, $seenClasses);

        if ($parameter->allowsNull()) {

            return [
                'anyOf' => [
                    $resolvedSchema,
                    [ 'type' => 'null' ],
                ],
            ];

        }

        return $resolvedSchema;
    }

    /**
     * @param list<class-string> $seenClasses
     * @return array<string, mixed>
     */
    private function resolveNamedTypeSchema(ReflectionNamedType $namedType, array $seenClasses): array
    {
        $typeName = $namedType->getName();

        if ($namedType->isBuiltin()) {
            return $this->resolveBuiltinTypeSchema($typeName);
        }

        if (enum_exists($typeName) && is_subclass_of($typeName, UnitEnum::class)) {
            return $this->resolveEnumSchema($typeName);
        }

        if (is_a($typeName, \DateTimeInterface::class, true)) {
            return [
                'type' => 'string',
                'format' => 'date-time',
            ];
        }

        if (is_subclass_of($typeName, WorkflowToolArguments::class)) {
            return $this->resolveDataClassSchema($typeName, $seenClasses);
        }

        return [
            'type' => 'object',
            'additionalProperties' => true,
        ];
    }

    /**
     * @return array<string, mixed>
     */
    private function resolveBuiltinTypeSchema(string $typeName): array
    {
        return match ($typeName) {
            'string' => [ 'type' => 'string' ],
            'int' => [ 'type' => 'integer' ],
            'float' => [ 'type' => 'number' ],
            'bool' => [ 'type' => 'boolean' ],
            'array' => [ 'type' => 'array', 'items' => [] ],
            'mixed' => [],
            default => [
                'type' => 'object',
                'additionalProperties' => true,
            ],
        };
    }

    /**
     * @param class-string<UnitEnum> $enumClass
     * @return array<string, mixed>
     */
    private function resolveEnumSchema(string $enumClass): array
    {
        $enumCases = $enumClass::cases();

        if ($enumCases === []) {
            return [];
        }

        $firstValue = $enumCases[ 0 ] instanceof \BackedEnum ? $enumCases[ 0 ]->value : $enumCases[ 0 ]->name;
        $schemaType = is_int($firstValue) ? 'integer' : 'string';

        return [
            'type' => $schemaType,
            'enum' => array_map(
                static fn (UnitEnum $enumCase): string|int => $enumCase instanceof \BackedEnum ? $enumCase->value : $enumCase->name,
                $enumCases,
            ),
        ];
    }
}
