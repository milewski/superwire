<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools\Execution;

use LogicException;
use ReflectionMethod;
use ReflectionNamedType;
use Superwire\Laravel\Contracts\ToolBoundInputData;
use Superwire\Laravel\Contracts\ToolInputData;
use Superwire\Laravel\Contracts\ToolOutputData;
use Superwire\Laravel\Tools\Data\EmptyToolBoundInputData;
use Superwire\Laravel\Tools\Data\EmptyToolInputData;

final class ToolExecutionSignatureFactory
{
    public function build(string $toolClassName): ToolExecutionSignature
    {
        if (!method_exists($toolClassName, 'handle')) {

            throw new LogicException(sprintf(
                'tool `%s` must define protected method handle(<input>, <bound>): <output>',
                $toolClassName,
            ));

        }

        $handleMethod = new ReflectionMethod($toolClassName, 'handle');

        if ($handleMethod->isPrivate()) {

            throw new LogicException(sprintf(
                'tool `%s` handle method cannot be private',
                $toolClassName,
            ));

        }

        [
            'agent_input_class' => $agentInputClass,
            'bound_input_class' => $boundInputClass,
            'handle_parameters' => $handleParameters,
        ] = $this->resolveHandleParameters($handleMethod, $toolClassName);

        $outputClass = $this->toolDataClassFromReturnType($handleMethod, ToolOutputData::class, $toolClassName);

        return new ToolExecutionSignature(
            agentInputClass: $agentInputClass,
            boundInputClass: $boundInputClass,
            outputClass: $outputClass,
            handleParameters: $handleParameters,
        );
    }

    /**
     * @return array{agent_input_class: class-string<ToolInputData>, bound_input_class: class-string<ToolBoundInputData>, handle_parameters: list<ToolHandleParameter>}
     */
    private function resolveHandleParameters(ReflectionMethod $handleMethod, string $toolClassName): array
    {
        $agentInputClass = null;
        $boundInputClass = null;
        $resolvedHandleParameters = [];

        foreach ($handleMethod->getParameters() as $handleParameter) {

            $parameterType = $handleParameter->getType();

            if (!$parameterType instanceof ReflectionNamedType || $parameterType->isBuiltin()) {

                throw new LogicException(sprintf(
                    'tool `%s` handle parameter `%s` must be a class type; scalar and union types are not supported',
                    $toolClassName,
                    $handleParameter->getName(),
                ));

            }

            $parameterClassName = $parameterType->getName();

            if (is_a($parameterClassName, ToolInputData::class, true)) {

                if ($agentInputClass !== null) {

                    throw new LogicException(sprintf(
                        'tool `%s` handle method can define only one `%s` parameter',
                        $toolClassName,
                        ToolInputData::class,
                    ));

                }

                $agentInputClass = $parameterClassName;
                $resolvedHandleParameters[] = ToolHandleParameter::agentInput($parameterClassName);

                continue;

            }

            if (is_a($parameterClassName, ToolBoundInputData::class, true)) {

                if ($boundInputClass !== null) {

                    throw new LogicException(sprintf(
                        'tool `%s` handle method can define only one `%s` parameter',
                        $toolClassName,
                        ToolBoundInputData::class,
                    ));

                }

                $boundInputClass = $parameterClassName;
                $resolvedHandleParameters[] = ToolHandleParameter::boundInput($parameterClassName);

                continue;

            }

            $resolvedHandleParameters[] = ToolHandleParameter::container($parameterClassName);

        }

        return [
            'agent_input_class' => $agentInputClass ?? EmptyToolInputData::class,
            'bound_input_class' => $boundInputClass ?? EmptyToolBoundInputData::class,
            'handle_parameters' => $resolvedHandleParameters,
        ];
    }

    /**
     * @param class-string $expectedInterfaceClass
     * @return class-string
     */
    private function toolDataClassFromReturnType(
        ReflectionMethod $reflectionMethod,
        string $expectedInterfaceClass,
        string $toolClassName,
    ): string {
        $returnType = $reflectionMethod->getReturnType();

        if (!$returnType instanceof ReflectionNamedType || $returnType->isBuiltin()) {

            throw new LogicException(sprintf(
                'tool `%s` handle return type must be a class implementing `%s`',
                $toolClassName,
                $expectedInterfaceClass,
            ));

        }

        $returnClassName = $returnType->getName();

        if (!is_a($returnClassName, $expectedInterfaceClass, true)) {

            throw new LogicException(sprintf(
                'tool `%s` handle return type must implement `%s`, found `%s`',
                $toolClassName,
                $expectedInterfaceClass,
                $returnClassName,
            ));

        }

        return $returnClassName;
    }
}
