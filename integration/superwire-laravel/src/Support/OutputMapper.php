<?php

namespace Superwire\Laravel\Support;

use ReflectionClass;
use ReflectionException;
use ReflectionNamedType;
use Superwire\Laravel\Exceptions\WorkflowExecutionException;

final class OutputMapper
{
    /**
     * @param array<string, mixed> $payload
     * @param class-string $outputClassName
     *
     * @throws ReflectionException
     */
    public function mapToClass(array $payload, string $outputClassName): object
    {
        if (method_exists($outputClassName, 'fromArray')) {
            return $outputClassName::fromArray($payload);
        }

        $outputClassReflection = new ReflectionClass($outputClassName);
        $constructorReflection = $outputClassReflection->getConstructor();

        if ($constructorReflection === null || $constructorReflection->getNumberOfParameters() === 0) {
            $instance = $outputClassReflection->newInstance();

            foreach ($payload as $fieldName => $fieldValue) {
                if (!$outputClassReflection->hasProperty($fieldName)) {
                    continue;
                }

                $propertyReflection = $outputClassReflection->getProperty($fieldName);
                $propertyReflection->setAccessible(true);
                $propertyReflection->setValue($instance, $fieldValue);
            }

            return $instance;
        }

        $constructorArguments = [];

        foreach ($constructorReflection->getParameters() as $parameterReflection) {
            $parameterName = $parameterReflection->getName();

            if (array_key_exists($parameterName, $payload)) {
                $constructorArguments[] = $payload[ $parameterName ];

                continue;
            }

            if ($parameterReflection->isDefaultValueAvailable()) {
                $constructorArguments[] = $parameterReflection->getDefaultValue();

                continue;
            }

            $parameterType = $parameterReflection->getType();

            if ($parameterType instanceof ReflectionNamedType && $parameterType->allowsNull()) {
                $constructorArguments[] = null;

                continue;
            }

            throw new WorkflowExecutionException(sprintf(
                'failed to map workflow output into %s: missing required field `%s`',
                $outputClassName,
                $parameterName,
            ));
        }

        return $outputClassReflection->newInstanceArgs($constructorArguments);
    }
}
