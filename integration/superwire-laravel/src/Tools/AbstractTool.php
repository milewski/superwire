<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools;

use Illuminate\Support\Str;
use InvalidArgumentException;
use LogicException;
use ReflectionClass;
use ReflectionMethod;
use ReflectionNamedType;
use ReflectionParameter;
use ReflectionType;
use ReflectionUnionType;
use Superwire\Laravel\Contracts\Tool;
use Superwire\Laravel\Contracts\ToolBoundInputData;
use Superwire\Laravel\Contracts\ToolData;
use Superwire\Laravel\Contracts\ToolInputData;
use Superwire\Laravel\Contracts\ToolOutputData;
use Superwire\Laravel\Tools\Data\EmptyToolBoundInputData;
use Superwire\Laravel\Tools\Data\EmptyToolInputData;
use Superwire\Laravel\Tools\Execution\ToolExecutionSignature;
use Superwire\Laravel\Tools\Execution\ToolExecutionSignatureRegistry;
use Superwire\Laravel\Tools\Execution\ToolHandleParameter;
use Superwire\Laravel\Tools\Execution\ToolHandleParameterKind;
use Swaggest\JsonSchema\Schema;

abstract class AbstractTool implements Tool
{
    private static ?ToolExecutionSignatureRegistry $executionSignatures = null;

    public static function name(): string
    {
        return Str::snake(class_basename(static::class));
    }

    public static function description(): string
    {
        return sprintf('Proxy tool for %s', static::class);
    }

    public static function endpointName(): string
    {
        return static::name();
    }

    /**
     * @return class-string<ToolInputData>
     */
    final public static function agentInputClass(): string
    {
        return static::executionSignature()->agentInputClass;
    }

    /**
     * @return class-string<ToolBoundInputData>
     */
    final public static function boundInputClass(): string
    {
        return static::executionSignature()->boundInputClass;
    }

    /**
     * @return class-string<ToolOutputData>
     */
    final public static function outputClass(): string
    {
        return static::executionSignature()->outputClass;
    }

    final public static function inputSchema(): Schema
    {
        return static::schemaFromToolDataClass(static::agentInputClass(), []);
    }

    final public static function boundInputSchema(): Schema
    {
        return static::schemaFromToolDataClass(static::boundInputClass(), []);
    }

    final public static function outputSchema(): Schema
    {
        return static::schemaFromToolDataClass(static::outputClass(), []);
    }

    /**
     * @param array<string, mixed> $agentInput
     * @param array<string, mixed> $boundInput
     * @return array<string, mixed>
     */
    final public function execute(array $agentInput, array $boundInput): array
    {
        $executionSignature = static::executionSignature();

        $agentInputData = static::hydrateToolDataClass(
            $executionSignature->agentInputClass,
            $agentInput,
            'agent input',
        );

        $boundInputData = static::hydrateToolDataClass(
            $executionSignature->boundInputClass,
            $boundInput,
            'bound input',
        );

        if (!method_exists($this, 'handle')) {

            throw new LogicException(sprintf(
                'tool `%s` must define protected method handle(<input>, <bound>): <output>',
                static::class,
            ));

        }

        $handleArguments = [];

        foreach ($executionSignature->handleParameters() as $handleParameter) {
            $handleArguments[] = match ($handleParameter->kind) {
                ToolHandleParameterKind::AgentInput => $agentInputData,
                ToolHandleParameterKind::BoundInput => $boundInputData,
                ToolHandleParameterKind::Container => app($handleParameter->className),
            };
        }

        $toolOutput = $this->handle(...$handleArguments);

        if (!$toolOutput instanceof ToolOutputData) {

            throw new InvalidArgumentException(sprintf(
                'tool `%s` handle method must return `%s`, received `%s`',
                static::class,
                ToolOutputData::class,
                get_debug_type($toolOutput),
            ));

        }

        if (!$toolOutput instanceof $executionSignature->outputClass) {

            throw new InvalidArgumentException(sprintf(
                'tool `%s` handle method must return `%s`, received `%s`',
                static::class,
                $executionSignature->outputClass,
                $toolOutput::class,
            ));

        }

        return static::extractToolDataPayload($toolOutput);
    }

    /**
     * Each tool implementation must define:
     *
     * `protected function handle(MyInput $agentInput, MyBoundInput $boundInput): MyOutput`
     *
     * where DTO classes implement ToolInputData, ToolBoundInputData, and ToolOutputData.
     */

    private static function executionSignature(): ToolExecutionSignature
    {
        return static::executionSignatureRegistry()->remember(
            static::class,
            static fn (): ToolExecutionSignature => static::buildExecutionSignature(static::class),
        );
    }

    private static function executionSignatureRegistry(): ToolExecutionSignatureRegistry
    {
        if (self::$executionSignatures === null) {
            self::$executionSignatures = new ToolExecutionSignatureRegistry();
        }

        return self::$executionSignatures;
    }

    private static function buildExecutionSignature(string $toolClassName): ToolExecutionSignature
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
        ] = static::resolveHandleParameters($handleMethod, $toolClassName);

        $outputClass = static::toolDataClassFromReturnType($handleMethod, ToolOutputData::class, $toolClassName);

        return new ToolExecutionSignature(
            agentInputClass: $agentInputClass,
            boundInputClass: $boundInputClass,
            outputClass: $outputClass,
            handleParameters: $handleParameters,
        );
    }

    /**
     * @param class-string<ToolData> $toolDataClassName
     * @param list<class-string<ToolData>> $visitedClassNames
     */
    private static function schemaFromToolDataClass(string $toolDataClassName, array $visitedClassNames): Schema
    {
        if (in_array($toolDataClassName, $visitedClassNames, true)) {
            throw new LogicException(sprintf('recursive tool data schema detected for `%s`', $toolDataClassName));
        }

        $visitedClassNames[] = $toolDataClassName;
        $toolDataReflectionClass = new ReflectionClass($toolDataClassName);
        $toolDataSchema = Schema::object();
        $toolDataSchema->additionalProperties = false;
        $requiredPropertyNames = [];

        foreach (static::constructorParameters($toolDataReflectionClass) as $constructorParameter) {

            $propertySchema = static::schemaFromType($constructorParameter->getType(), $visitedClassNames);
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
     * @param class-string<ToolData> $toolDataClassName
     * @param array<string, mixed> $payload
     */
    private static function hydrateToolDataClass(string $toolDataClassName, array $payload, string $payloadContext): ToolData
    {
        $toolDataReflectionClass = new ReflectionClass($toolDataClassName);
        $constructorParameters = static::constructorParameters($toolDataReflectionClass);
        $expectedFieldNames = array_map(
            static fn (ReflectionParameter $constructorParameter): string => $constructorParameter->getName(),
            $constructorParameters,
        );

        foreach ($payload as $fieldName => $fieldValue) {

            if (!is_string($fieldName) || !in_array($fieldName, $expectedFieldNames, true)) {

                throw new InvalidArgumentException(sprintf(
                    '%s payload contains unknown field `%s` for `%s`',
                    $payloadContext,
                    (string) $fieldName,
                    $toolDataClassName,
                ));

            }

        }

        $constructorArguments = [];

        foreach ($constructorParameters as $constructorParameter) {

            $fieldName = $constructorParameter->getName();

            if (array_key_exists($fieldName, $payload)) {

                $constructorArguments[ $fieldName ] = static::payloadValueForType(
                    $payload[ $fieldName ],
                    $constructorParameter->getType(),
                    sprintf('%s.%s', $payloadContext, $fieldName),
                );

                continue;

            }

            if ($constructorParameter->isDefaultValueAvailable()) {

                $constructorArguments[ $fieldName ] = $constructorParameter->getDefaultValue();

                continue;

            }

            throw new InvalidArgumentException(sprintf(
                '%s payload is missing required field `%s` for `%s`',
                $payloadContext,
                $fieldName,
                $toolDataClassName,
            ));

        }

        /** @var ToolData $toolDataInstance */
        $toolDataInstance = $toolDataReflectionClass->newInstanceArgs($constructorArguments);

        return $toolDataInstance;
    }

    /**
     * @return array<string, mixed>
     */
    private static function extractToolDataPayload(ToolData $toolData): array
    {
        $toolDataReflectionClass = new ReflectionClass($toolData::class);
        $payload = [];

        foreach (static::constructorParameters($toolDataReflectionClass) as $constructorParameter) {

            $propertyName = $constructorParameter->getName();

            if (!$toolDataReflectionClass->hasProperty($propertyName)) {
                continue;
            }

            $toolDataProperty = $toolDataReflectionClass->getProperty($propertyName);

            if (!$toolDataProperty->isPublic()) {
                $toolDataProperty->setAccessible(true);
            }

            $payload[ $propertyName ] = static::payloadValueFromToolData($toolDataProperty->getValue($toolData));

        }

        return $payload;
    }

    /**
     * @param list<class-string<ToolData>> $visitedClassNames
     */
    private static function schemaFromType(?ReflectionType $reflectionType, array $visitedClassNames): Schema
    {
        if ($reflectionType === null) {
            return Schema::create();
        }

        if ($reflectionType instanceof ReflectionNamedType) {
            return static::schemaFromNamedType($reflectionType, $visitedClassNames);
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

                $unionSchemas[] = static::schemaFromNamedType($unionMemberType, $visitedClassNames);

            }

            $unionTypeSchema->anyOf = $unionSchemas;

            return $unionTypeSchema;

        }

        throw new LogicException('unsupported reflection type for tool schema generation');
    }

    /**
     * @param list<class-string<ToolData>> $visitedClassNames
     */
    private static function schemaFromNamedType(ReflectionNamedType $namedType, array $visitedClassNames): Schema
    {
        if ($namedType->isBuiltin()) {
            return static::schemaFromBuiltinType($namedType->getName());
        }

        $typeClassName = $namedType->getName();

        if (is_a($typeClassName, ToolData::class, true)) {
            return static::schemaFromToolDataClass($typeClassName, $visitedClassNames);
        }

        throw new LogicException(sprintf(
            'unsupported non-tool-data type `%s` in tool schema generation',
            $typeClassName,
        ));
    }

    private static function schemaFromBuiltinType(string $builtinTypeName): Schema
    {
        return match ($builtinTypeName) {
            'string' => Schema::string(),
            'int' => Schema::integer(),
            'float' => Schema::number(),
            'bool' => Schema::boolean(),
            'array' => static::arraySchema(),
            'object' => Schema::object(),
            'mixed' => Schema::create(),
            default => throw new LogicException(sprintf('unsupported builtin type `%s` for tool schema generation', $builtinTypeName)),
        };
    }

    private static function arraySchema(): Schema
    {
        $arraySchema = Schema::create();
        $arraySchema->type = 'array';

        return $arraySchema;
    }

    private static function payloadValueForType(mixed $payloadValue, ?ReflectionType $reflectionType, string $payloadPath): mixed
    {
        if ($reflectionType === null) {
            return $payloadValue;
        }

        if ($reflectionType instanceof ReflectionNamedType) {
            return static::payloadValueForNamedType($payloadValue, $reflectionType, $payloadPath);
        }

        if ($reflectionType instanceof ReflectionUnionType) {

            foreach ($reflectionType->getTypes() as $unionMemberType) {

                if (!$unionMemberType instanceof ReflectionNamedType) {
                    continue;
                }

                if ($unionMemberType->getName() === 'null' && $payloadValue === null) {
                    return null;
                }

                if ($unionMemberType->getName() === 'null') {
                    continue;
                }

                try {

                    return static::payloadValueForNamedType($payloadValue, $unionMemberType, $payloadPath);

                } catch (InvalidArgumentException $invalidArgumentException) {

                    continue;

                }

            }

            throw new InvalidArgumentException(sprintf(
                'field `%s` does not match any supported union type',
                $payloadPath,
            ));

        }

        throw new InvalidArgumentException(sprintf('unsupported reflection type for `%s`', $payloadPath));
    }

    private static function payloadValueForNamedType(mixed $payloadValue, ReflectionNamedType $namedType, string $payloadPath): mixed
    {
        if ($payloadValue === null && $namedType->allowsNull()) {
            return null;
        }

        if ($namedType->isBuiltin()) {
            return static::payloadValueForBuiltinType($payloadValue, $namedType->getName(), $payloadPath);
        }

        $typeClassName = $namedType->getName();

        if (is_a($typeClassName, ToolData::class, true)) {

            if (!is_array($payloadValue)) {

                throw new InvalidArgumentException(sprintf(
                    'field `%s` must be an object payload for `%s`, received `%s`',
                    $payloadPath,
                    $typeClassName,
                    get_debug_type($payloadValue),
                ));

            }

            return static::hydrateToolDataClass($typeClassName, $payloadValue, $payloadPath);

        }

        throw new InvalidArgumentException(sprintf(
            'field `%s` has unsupported type `%s`',
            $payloadPath,
            $typeClassName,
        ));
    }

    private static function payloadValueForBuiltinType(mixed $payloadValue, string $builtinTypeName, string $payloadPath): mixed
    {
        return match ($builtinTypeName) {
            'string' => static::assertPayloadType($payloadValue, 'string', $payloadPath),
            'int' => static::assertPayloadType($payloadValue, 'integer', $payloadPath),
            'float' => static::assertNumericPayloadType($payloadValue, $payloadPath),
            'bool' => static::assertPayloadType($payloadValue, 'boolean', $payloadPath),
            'array' => static::assertPayloadType($payloadValue, 'array', $payloadPath),
            'object' => static::assertPayloadType($payloadValue, 'object', $payloadPath),
            'mixed' => $payloadValue,
            default => throw new InvalidArgumentException(sprintf(
                'field `%s` has unsupported builtin type `%s`',
                $payloadPath,
                $builtinTypeName,
            )),
        };
    }

    private static function assertPayloadType(mixed $payloadValue, string $expectedTypeName, string $payloadPath): mixed
    {
        $hasExpectedType = match ($expectedTypeName) {
            'string' => is_string($payloadValue),
            'integer' => is_int($payloadValue),
            'boolean' => is_bool($payloadValue),
            'array' => is_array($payloadValue),
            'object' => is_object($payloadValue),
            default => false,
        };

        if ($hasExpectedType) {
            return $payloadValue;
        }

        throw new InvalidArgumentException(sprintf(
            'field `%s` must be %s, received `%s`',
            $payloadPath,
            $expectedTypeName,
            get_debug_type($payloadValue),
        ));
    }

    private static function assertNumericPayloadType(mixed $payloadValue, string $payloadPath): float
    {
        if (is_float($payloadValue)) {
            return $payloadValue;
        }

        if (is_int($payloadValue)) {
            return (float) $payloadValue;
        }

        throw new InvalidArgumentException(sprintf(
            'field `%s` must be number, received `%s`',
            $payloadPath,
            get_debug_type($payloadValue),
        ));
    }

    private static function payloadValueFromToolData(mixed $toolDataValue): mixed
    {
        if ($toolDataValue instanceof ToolData) {
            return static::extractToolDataPayload($toolDataValue);
        }

        if (is_array($toolDataValue)) {

            $normalizedArray = [];

            foreach ($toolDataValue as $arrayKey => $arrayValue) {
                $normalizedArray[ $arrayKey ] = static::payloadValueFromToolData($arrayValue);
            }

            return $normalizedArray;

        }

        if (is_string($toolDataValue) || is_int($toolDataValue) || is_float($toolDataValue) || is_bool($toolDataValue) || $toolDataValue === null) {
            return $toolDataValue;
        }

        throw new InvalidArgumentException(sprintf(
            'tool output contains unsupported value `%s`; only scalar, null, array, and tool data objects are supported',
            get_debug_type($toolDataValue),
        ));
    }

    /**
     * @param ReflectionClass<ToolData> $toolDataReflectionClass
     * @return list<ReflectionParameter>
     */
    private static function constructorParameters(ReflectionClass $toolDataReflectionClass): array
    {
        $constructor = $toolDataReflectionClass->getConstructor();

        if ($constructor === null) {
            return [];
        }

        return $constructor->getParameters();
    }

    /**
     * @return array{agent_input_class: class-string<ToolInputData>, bound_input_class: class-string<ToolBoundInputData>, handle_parameters: list<ToolHandleParameter>}
     */
    private static function resolveHandleParameters(ReflectionMethod $handleMethod, string $toolClassName): array
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
     * @param class-string<ToolData> $expectedInterfaceClass
     * @return class-string
     */
    private static function toolDataClassFromParameter(
        ReflectionParameter $reflectionParameter,
        string $expectedInterfaceClass,
        string $parameterLabel,
        string $toolClassName,
    ): string {
        $parameterType = $reflectionParameter->getType();

        if (!$parameterType instanceof ReflectionNamedType || $parameterType->isBuiltin()) {

            throw new LogicException(sprintf(
                'tool `%s` handle %s parameter `%s` must be a class implementing `%s`',
                $toolClassName,
                $parameterLabel,
                $reflectionParameter->getName(),
                $expectedInterfaceClass,
            ));

        }

        $parameterClassName = $parameterType->getName();

        if (!is_a($parameterClassName, $expectedInterfaceClass, true)) {

            throw new LogicException(sprintf(
                'tool `%s` handle %s parameter `%s` must implement `%s`, found `%s`',
                $toolClassName,
                $parameterLabel,
                $reflectionParameter->getName(),
                $expectedInterfaceClass,
                $parameterClassName,
            ));

        }

        return $parameterClassName;
    }

    /**
     * @param class-string<ToolData> $expectedInterfaceClass
     * @return class-string
     */
    private static function toolDataClassFromReturnType(
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
