<?php

declare(strict_types = 1);

namespace EngineAi\Ffi;

use BackedEnum;
use ReflectionClass;
use ReflectionNamedType;
use ReflectionParameter;
use ReflectionType;
use ReflectionUnionType;
use RuntimeException;
use TypeError;
use UnitEnum;

final class ToolPayloadHydrator
{
    /**
     * @param class-string $className
     * @param array<string, mixed> $payload
     */
    public static function hydrate(string $className, array $payload): object
    {
        if (!class_exists($className)) {
            throw new RuntimeException("Hydration class `{$className}` does not exist.");
        }

        $reflectionClass = new ReflectionClass($className);

        if ($instance = self::invokeFactory($reflectionClass, 'fromArray', $payload)) {
            return $instance;
        }

        if ($instance = self::invokeFactory($reflectionClass, 'from', $payload)) {
            return $instance;
        }

        if ($reflectionClass->isEnum()) {
            throw new RuntimeException("Enum `{$className}` cannot be used as a payload DTO class.");
        }

        $constructor = $reflectionClass->getConstructor();

        if ($constructor === null) {
            return $reflectionClass->newInstance();
        }

        $arguments = [];

        foreach ($constructor->getParameters() as $parameter) {
            $arguments[] = self::resolveParameterValue($parameter, $payload, $className);
        }

        return $reflectionClass->newInstanceArgs($arguments);
    }

    /**
     * @param array<string, mixed> $payload
     */
    private static function invokeFactory(ReflectionClass $reflectionClass, string $factoryMethod, array $payload): ?object
    {
        if (!$reflectionClass->hasMethod($factoryMethod)) {
            return null;
        }

        $method = $reflectionClass->getMethod($factoryMethod);

        if (!$method->isPublic() || !$method->isStatic()) {
            return null;
        }

        if ($method->getNumberOfRequiredParameters() > 1) {
            return null;
        }

        $instance = $method->getNumberOfParameters() === 0
            ? $method->invoke(null)
            : $method->invoke(null, $payload);

        if (!is_object($instance)) {

            throw new RuntimeException(
                "Factory {$reflectionClass->getName()}::{$factoryMethod}() must return an object.",
            );

        }

        return $instance;
    }

    /**
     * @param array<string, mixed> $payload
     */
    private static function resolveParameterValue(ReflectionParameter $parameter, array $payload, string $className): mixed
    {
        $parameterName = $parameter->getName();

        if (array_key_exists($parameterName, $payload)) {

            return self::coerceType(
                value: $payload[ $parameterName ],
                type: $parameter->getType(),
                context: "{$className}::\${$parameterName}",
            );

        }

        if ($parameter->isDefaultValueAvailable()) {
            return $parameter->getDefaultValue();
        }

        if ($parameter->allowsNull()) {
            return null;
        }

        throw new RuntimeException("Missing required payload key `{$parameterName}` for `{$className}`.");
    }

    private static function coerceType(mixed $value, ?ReflectionType $type, string $context): mixed
    {
        if ($type === null) {
            return $value;
        }

        if ($type instanceof ReflectionUnionType) {

            foreach ($type->getTypes() as $unionType) {

                try {

                    return self::coerceType($value, $unionType, $context);

                } catch (RuntimeException|TypeError) {
                }

            }

            $typeDescription = implode('|', array_map(
                static fn (ReflectionType $unionType): string => $unionType->getName(),
                $type->getTypes(),
            ));

            throw new RuntimeException("Value for `{$context}` does not match union type `{$typeDescription}`.");

        }

        if (!$type instanceof ReflectionNamedType) {
            return $value;
        }

        if ($type->allowsNull() && $value === null) {
            return null;
        }

        $typeName = $type->getName();

        if ($type->isBuiltin()) {
            return self::coerceBuiltinType($value, $typeName, $context);
        }

        if ($value instanceof $typeName) {
            return $value;
        }

        if (is_subclass_of($typeName, BackedEnum::class)) {

            if (!is_string($value) && !is_int($value)) {
                throw new RuntimeException("Value for `{$context}` must be scalar to hydrate backed enum `{$typeName}`.");
            }

            return $typeName::from($value);

        }

        if (is_subclass_of($typeName, UnitEnum::class)) {

            if (!is_string($value)) {
                throw new RuntimeException("Value for `{$context}` must be string to hydrate enum `{$typeName}`.");
            }

            foreach ($typeName::cases() as $enumCase) {

                if ($enumCase->name === $value) {
                    return $enumCase;
                }

            }

            throw new RuntimeException("Invalid enum case `{$value}` for enum `{$typeName}` at `{$context}`.");

        }

        if (!is_array($value)) {

            $receivedType = get_debug_type($value);

            throw new RuntimeException("Value for `{$context}` must be object-compatible payload array, got {$receivedType}.");

        }

        return self::hydrate($typeName, $value);
    }

    private static function coerceBuiltinType(mixed $value, string $builtinType, string $context): mixed
    {
        return match ($builtinType) {
            'mixed' => $value,
            'int' => self::assertType($value, 'int', is_int(...), $context),
            'float' => self::assertType($value, 'float', static fn (mixed $candidate): bool => is_float($candidate) || is_int($candidate), $context),
            'string' => self::assertType($value, 'string', is_string(...), $context),
            'bool' => self::assertType($value, 'bool', is_bool(...), $context),
            'array' => self::assertType($value, 'array', is_array(...), $context),
            'null' => self::assertType($value, 'null', static fn (mixed $candidate): bool => $candidate === null, $context),
            default => throw new RuntimeException("Unsupported builtin type `{$builtinType}` at `{$context}`."),
        };
    }

    private static function assertType(mixed $value, string $expectedType, callable $guard, string $context): mixed
    {
        if ($guard($value)) {
            return $value;
        }

        $receivedType = get_debug_type($value);

        throw new RuntimeException("Value for `{$context}` must be {$expectedType}, got {$receivedType}.");
    }
}
