<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools\Concerns;

use ReflectionClass;
use ReflectionMethod;
use ReflectionNamedType;
use ReflectionParameter;
use RuntimeException;
use Spatie\LaravelData\Contracts\BaseData;
use Superwire\Laravel\Contracts\BoundInput;
use Superwire\Laravel\Contracts\ToolInput;

trait ReflectsToolSignature
{
    public static function resolveAgentInput(array $input): mixed
    {
        $agentInputClass = static::agentInputClass();

        if ($agentInputClass === null) {
            return null;
        }

        return static::resolveDataObject($agentInputClass, $input);
    }

    public static function resolveBoundInput(array $input): mixed
    {
        $boundInputClass = static::boundInputClass();

        if ($boundInputClass === null) {
            return null;
        }

        return static::resolveDataObject($boundInputClass, $input);
    }

    protected static function agentInputClass(): ?string
    {
        return static::parameterClassMatchingInterface(ToolInput::class);
    }

    protected static function boundInputClass(): ?string
    {
        return static::parameterClassMatchingInterface(BoundInput::class);
    }

    protected static function parameterClassMatchingInterface(string $interfaceName): ?string
    {
        foreach (static::executionMethodReflection()->getParameters() as $parameter) {

            $parameterClass = static::parameterClassFromReflection($parameter);

            if ($parameterClass !== null && is_a($parameterClass, $interfaceName, true)) {
                return $parameterClass;
            }

        }

        return null;
    }

    protected static function executionMethodReflection(): ReflectionMethod
    {
        $reflectionClass = new ReflectionClass(static::class);

        if ($reflectionClass->hasMethod('handle') && $reflectionClass->getMethod('handle')->getDeclaringClass()->getName() !== self::class) {
            return $reflectionClass->getMethod('handle');
        }

        throw new RuntimeException(sprintf('Tool `%s` must define `handle()`.', static::class));
    }

    protected static function parameterClassFromReflection(ReflectionParameter $parameter): ?string
    {
        $type = $parameter->getType();

        if (!$type instanceof ReflectionNamedType || $type->isBuiltin()) {
            return null;
        }

        return $type->getName();
    }

    protected static function resolveDataObject(string $className, array $payload): mixed
    {
        if (is_a($className, BaseData::class, true)) {
            return $className::from($payload);
        }

        return app()->make($className, $payload);
    }
}
