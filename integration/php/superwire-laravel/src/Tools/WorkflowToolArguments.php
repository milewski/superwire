<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools;

use JsonException;
use ReflectionClass;
use ReflectionIntersectionType;
use ReflectionNamedType;
use ReflectionParameter;
use ReflectionUnionType;
use RuntimeException;
use Spatie\LaravelData\Data;
use Swaggest\JsonSchema\InvalidValue;
use Swaggest\JsonSchema\Schema;

abstract class WorkflowToolArguments extends Data
{
    /**
     * @param array<string, mixed> $payload
     */
    public static function fromPayload(array $payload): static
    {
        $reflectionClass = new ReflectionClass(static::class);
        $constructor = $reflectionClass->getConstructor();

        if ($constructor === null) {
            return new static();
        }

        $resolvedArguments = [];

        foreach ($constructor->getParameters() as $parameter) {

            $parameterName = $parameter->getName();
            $parameterExists = array_key_exists($parameterName, $payload);

            if (!$parameterExists && $parameter->isDefaultValueAvailable()) {

                $resolvedArguments[] = $parameter->getDefaultValue();

                continue;

            }

            if (!$parameterExists && $parameter->allowsNull()) {

                $resolvedArguments[] = null;

                continue;

            }

            if (!$parameterExists) {
                throw new RuntimeException("missing required argument `{$parameterName}` for `" . static::class . '`');
            }

            $resolvedArguments[] = self::resolveParameterValue($parameter, $payload[ $parameterName ]);

        }

        return new static(...$resolvedArguments);
    }

    /**
     * @return array<string, mixed>
     */
    public static function jsonSchema(): array
    {
        return WorkflowToolJsonSchemaResolver::forDataClass(static::class);
    }

    public static function schema(): Schema
    {
        try {

            return Schema::import(json_decode(json_encode(static::jsonSchema(), JSON_THROW_ON_ERROR), false, 512, JSON_THROW_ON_ERROR));

        } catch (InvalidValue|JsonException $error) {

            throw new RuntimeException(
                sprintf('failed to resolve json schema for `%s`: %s', static::class, $error->getMessage()),
                previous: $error,
            );

        }
    }

    private static function resolveParameterValue(ReflectionParameter $parameter, mixed $rawValue): mixed
    {
        $reflectionType = $parameter->getType();

        if ($reflectionType === null) {
            return $rawValue;
        }

        if ($reflectionType instanceof ReflectionIntersectionType) {
            return $rawValue;
        }

        if ($reflectionType instanceof ReflectionUnionType) {

            foreach ($reflectionType->getTypes() as $unionedType) {

                if ($unionedType->getName() === 'null' && $rawValue === null) {
                    return null;
                }

                try {

                    return self::resolveNamedTypeValue($unionedType, $rawValue);

                } catch (RuntimeException) {
                }

            }

            throw new RuntimeException("argument `{$parameter->getName()}` has invalid union value");

        }

        return self::resolveNamedTypeValue($reflectionType, $rawValue);
    }

    private static function resolveNamedTypeValue(ReflectionNamedType $namedType, mixed $rawValue): mixed
    {
        if ($namedType->isBuiltin()) {
            return self::resolveBuiltinValue($namedType->getName(), $rawValue);
        }

        $className = $namedType->getName();

        if (is_subclass_of($className, WorkflowToolArguments::class)) {

            if (!is_array($rawValue)) {
                throw new RuntimeException("argument `{$className}` must be an object payload");
            }

            return $className::fromPayload($rawValue);

        }

        if (enum_exists($className)) {
            return $className::from($rawValue);
        }

        return $rawValue;
    }

    private static function resolveBuiltinValue(string $typeName, mixed $rawValue): mixed
    {
        return match ($typeName) {
            'int' => (int) $rawValue,
            'float' => (float) $rawValue,
            'string' => (string) $rawValue,
            'bool' => (bool) $rawValue,
            'array' => is_array($rawValue) ? $rawValue : [ $rawValue ],
            default => $rawValue,
        };
    }
}
