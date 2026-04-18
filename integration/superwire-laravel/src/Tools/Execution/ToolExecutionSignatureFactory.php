<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools\Execution;

use LogicException;
use ReflectionMethod;
use ReflectionNamedType;
use Spatie\LaravelData\Data;
use Superwire\Laravel\Contracts\ToolBoundInputData;
use Superwire\Laravel\Contracts\ToolInputData;
use Superwire\Laravel\Contracts\WitDefinedTool;
use Superwire\Laravel\Tools\Data\EmptyToolBoundInputData;
use Superwire\Laravel\Tools\Data\EmptyToolInputData;
use Superwire\Laravel\Wit\Schema\WitSchemaRecordKind;

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

        $outputClass = $this->toolDataClassFromReturnType($handleMethod, $toolClassName);

        $this->assertWitToolUsesGeneratedTypes(
            toolClassName: $toolClassName,
            agentInputClass: $agentInputClass,
            boundInputClass: $boundInputClass,
            outputClass: $outputClass,
        );

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

                if (!is_a($parameterClassName, Data::class, true)) {

                    throw new LogicException(sprintf(
                        'tool `%s` handle parameter `%s` must extend `%s` because it implements `%s`',
                        $toolClassName,
                        $parameterClassName,
                        Data::class,
                        ToolInputData::class,
                    ));

                }

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

                if (!is_a($parameterClassName, Data::class, true)) {

                    throw new LogicException(sprintf(
                        'tool `%s` handle parameter `%s` must extend `%s` because it implements `%s`',
                        $toolClassName,
                        $parameterClassName,
                        Data::class,
                        ToolBoundInputData::class,
                    ));

                }

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

    private function toolDataClassFromReturnType(
        ReflectionMethod $reflectionMethod,
        string $toolClassName,
    ): string {
        $returnType = $reflectionMethod->getReturnType();

        if (!$returnType instanceof ReflectionNamedType || $returnType->isBuiltin()) {

            throw new LogicException(sprintf(
                'tool `%s` handle return type must be a class extending `%s`',
                $toolClassName,
                Data::class,
            ));

        }

        $returnClassName = $returnType->getName();

        if (!is_a($returnClassName, Data::class, true)) {

            throw new LogicException(sprintf(
                'tool `%s` handle return type `%s` must extend `%s`',
                $toolClassName,
                $returnClassName,
                Data::class,
            ));

        }

        return $returnClassName;
    }

    /**
     * @param class-string $toolClassName
     * @param class-string<ToolInputData> $agentInputClass
     * @param class-string<ToolBoundInputData> $boundInputClass
     * @param class-string<Data> $outputClass
     */
    private function assertWitToolUsesGeneratedTypes(
        string $toolClassName,
        string $agentInputClass,
        string $boundInputClass,
        string $outputClass,
    ): void {
        if (!is_subclass_of($toolClassName, WitDefinedTool::class)) {
            return;
        }

        if (!method_exists($toolClassName, 'generatedTypeClassName')) {

            throw new LogicException(sprintf(
                'WIT tool `%s` must extend `%s` so generated types can be enforced',
                $toolClassName,
                'Superwire\\Laravel\\Tools\\AbstractWitTool',
            ));

        }

        $expectsAgentInput = method_exists($toolClassName, 'definesRecord')
            ? $toolClassName::definesRecord(WitSchemaRecordKind::AgentInput)
            : true;
        $expectsBoundInput = method_exists($toolClassName, 'definesRecord')
            ? $toolClassName::definesRecord(WitSchemaRecordKind::BoundInput)
            : true;

        $expectedAgentInputClass = $expectsAgentInput
            ? $toolClassName::generatedTypeClassName('AgentInput')
            : EmptyToolInputData::class;
        $expectedBoundInputClass = $expectsBoundInput
            ? $toolClassName::generatedTypeClassName('BoundInput')
            : EmptyToolBoundInputData::class;
        $expectedOutputClass = $toolClassName::generatedTypeClassName('Output');

        if ($agentInputClass !== $expectedAgentInputClass) {

            throw new LogicException(sprintf(
                'WIT tool `%s` must use generated agent input `%s` in handle signature; received `%s`',
                $toolClassName,
                $expectedAgentInputClass,
                $agentInputClass,
            ));

        }

        if ($boundInputClass !== $expectedBoundInputClass) {

            throw new LogicException(sprintf(
                'WIT tool `%s` must use generated bound input `%s` in handle signature; received `%s`',
                $toolClassName,
                $expectedBoundInputClass,
                $boundInputClass,
            ));

        }

        if ($outputClass !== $expectedOutputClass) {

            throw new LogicException(sprintf(
                'WIT tool `%s` must return generated output `%s` from handle; received `%s`',
                $toolClassName,
                $expectedOutputClass,
                $outputClass,
            ));

        }
    }
}
