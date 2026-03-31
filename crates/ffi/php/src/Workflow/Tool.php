<?php

declare(strict_types=1);

namespace EngineAi\Ffi;

use EngineAi\Ffi\Contracts\ToolBounded;
use EngineAi\Ffi\Contracts\ToolInput;
use ReflectionMethod;
use ReflectionNamedType;
use ReflectionParameter;
use ReflectionType;
use ReflectionUnionType;
use RuntimeException;

abstract class Tool
{
    public readonly string $name;

    public function __construct(?string $name = null)
    {
        $this->name = $name ?? $this->resolveToolName();
    }

    abstract public function description(): string;

    public function inputSchema(): array
    {
        $schema = ToolSchemaReflector::fromToolInput($this);

        if ($schema !== null) {
            return $schema;
        }

        if ($this->canInferEmptyInputSchema()) {
            return [
                'type' => 'object',
                'properties' => (object) [],
                'required' => [],
                'additionalProperties' => false,
            ];
        }

        throw new RuntimeException(
            'Tool input schema could not be inferred. Override inputSchema() or declare `public string $input = InputDto::class;`.',
        );
    }

    public function inputType(): ?string
    {
        return $this->resolvePayloadType(
            property: 'input',
            fallbackParameterPosition: 0,
            fallbackInterface: ToolInput::class,
        );
    }

    public function boundedType(): ?string
    {
        return $this->resolvePayloadType(
            property: 'bounded',
            fallbackParameterPosition: 1,
            fallbackInterface: ToolBounded::class,
        );
    }

    /**
     * @param array<string, mixed> $input
     * @param array<string, mixed> $bounded
     * @param array<string, mixed> $context
     */
    public function executeForTesting(array $input = [], array $bounded = [], array $context = []): mixed
    {
        $toolData = new ToolData(
            input: $input,
            bounded: $bounded,
            context: $context,
            inputType: $this->inputType(),
            boundedType: $this->boundedType(),
        );

        $executeMethod = $this->resolveExecuteMethod();
        $executeArguments = $this->resolveExecuteArguments($executeMethod, $toolData);

        return $executeMethod->invokeArgs($this, $executeArguments);
    }

    /**
     * @param array<string, mixed> $input
     * @param array<string, mixed> $bounded
     * @param array<string, mixed> $context
     */
    public function invokeForTesting(array $input = [], array $bounded = [], array $context = []): mixed
    {
        return ToolOutputNormalizer::normalize($this->executeForTesting($input, $bounded, $context));
    }

    public function invoke(ToolData $toolData): mixed
    {
        $executeMethod = $this->resolveExecuteMethod();
        $executeArguments = $this->resolveExecuteArguments($executeMethod, $toolData);

        return ToolOutputNormalizer::normalize($executeMethod->invokeArgs($this, $executeArguments));
    }

    public function outputSchema(): ?array
    {
        return ToolSchemaReflector::fromToolExecute($this);
    }

    public function toDeclaration(): array
    {
        $declaration = [
            'name' => $this->name,
            'description' => $this->description(),
            'input_schema' => $this->inputSchema(),
        ];

        $outputSchema = $this->outputSchema();

        if ($outputSchema !== null) {
            $declaration['output_schema'] = $outputSchema;
        }

        return $declaration;
    }

    private function resolveToolName(): string
    {
        $toolClassName = static::class;
        $toolNameConstant = $toolClassName . '::TOOL_NAME';

        if (\defined($toolNameConstant)) {
            $staticToolName = \trim((string) \constant($toolNameConstant));

            if ($staticToolName !== '') {
                return $staticToolName;
            }
        }

        return $this->deriveToolNameFromClassName();
    }

    private function resolvePayloadType(string $property, int $fallbackParameterPosition, string $fallbackInterface): ?string
    {
        if (!property_exists($this, $property)) {
            return $this->inferPayloadTypeFromExecuteSignature($fallbackParameterPosition, $fallbackInterface);
        }

        $type = $this->{$property};

        if (!is_string($type) || $type === '') {
            return $this->inferPayloadTypeFromExecuteSignature($fallbackParameterPosition, $fallbackInterface);
        }

        if (!class_exists($type)) {
            throw new RuntimeException("Tool `{$property}` class `{$type}` does not exist.");
        }

        return $type;
    }

    private function inferPayloadTypeFromExecuteSignature(int $position, string $interface): ?string
    {
        $executeMethod = $this->resolveExecuteMethod();
        $parameters = $executeMethod->getParameters();
        $parameter = $parameters[$position] ?? null;

        if ($parameter instanceof ReflectionParameter) {
            $payloadType = $this->resolvePayloadTypeFromParameter($parameter, $interface);

            if ($payloadType !== null) {
                return $payloadType;
            }
        }

        foreach ($parameters as $candidate) {
            if (!$candidate instanceof ReflectionParameter) {
                continue;
            }

            $payloadType = $this->resolvePayloadTypeFromParameter($candidate, $interface);

            if ($payloadType !== null) {
                return $payloadType;
            }
        }

        return null;
    }

    private function resolvePayloadTypeFromParameter(ReflectionParameter $parameter, string $interface): ?string
    {
        $type = $parameter->getType();

        if ($type === null) {
            return null;
        }

        if ($type instanceof ReflectionUnionType) {
            foreach ($type->getTypes() as $unionType) {
                $payloadType = $this->resolvePayloadTypeNameFromNamedType($unionType, $interface);

                if ($payloadType !== null) {
                    return $payloadType;
                }
            }

            return null;
        }

        return $this->resolvePayloadTypeNameFromNamedType($type, $interface);
    }

    private function resolvePayloadTypeNameFromNamedType(ReflectionType $type, string $interface): ?string
    {
        if (!$type instanceof ReflectionNamedType || $type->isBuiltin()) {
            return null;
        }

        $typeName = $type->getName();

        if ($typeName === ToolData::class || $typeName === ToolValueBag::class) {
            return null;
        }

        if (!class_exists($typeName) || !is_subclass_of($typeName, $interface)) {
            return null;
        }

        return $typeName;
    }

    private function deriveToolNameFromClassName(): string
    {
        $fullyQualifiedClassName = static::class;
        $classNameSegments = \explode('\\', $fullyQualifiedClassName);
        $shortClassName = \trim((string) \end($classNameSegments));

        if ($shortClassName === '') {
            throw new RuntimeException('Tool name could not be inferred from class name.');
        }

        $normalizedName = \preg_replace('/([a-z0-9])([A-Z])/', '$1_$2', $shortClassName);
        $normalizedName = \preg_replace('/[-\s]+/', '_', (string) $normalizedName);
        $normalizedName = \strtolower((string) $normalizedName);

        if ($normalizedName === '') {
            throw new RuntimeException('Tool name normalization produced an empty value.');
        }

        return $normalizedName;
    }

    private function resolveExecuteMethod(): ReflectionMethod
    {
        if (!method_exists($this, 'execute')) {
            throw new RuntimeException('Tool class must define an `execute` method.');
        }

        $executeMethod = new ReflectionMethod($this, 'execute');

        if ($executeMethod->getDeclaringClass()->getName() === self::class) {
            throw new RuntimeException('Tool class must override the `execute` method.');
        }

        return $executeMethod;
    }

    private function canInferEmptyInputSchema(): bool
    {
        $executeMethod = $this->resolveExecuteMethod();
        $parameters = $executeMethod->getParameters();

        if ($parameters === []) {
            return true;
        }

        if (count($parameters) === 1 && $this->parameterAcceptsType($parameters[0], ToolData::class)) {
            return false;
        }

        foreach ($parameters as $position => $parameter) {
            $parameterName = strtolower($parameter->getName());

            if ($parameterName === 'bounded' || $parameterName === 'context') {
                continue;
            }

            if ($position === 1 || $position === 2) {
                continue;
            }

            return false;
        }

        return true;
    }

    /**
     * @return array<int, mixed>
     */
    private function resolveExecuteArguments(ReflectionMethod $executeMethod, ToolData $toolData): array
    {
        $parameters = $executeMethod->getParameters();

        if (count($parameters) === 1 && $this->parameterAcceptsType($parameters[0], ToolData::class)) {
            return [ $toolData ];
        }

        $arguments = [];

        foreach ($parameters as $position => $parameter) {
            $value = $this->resolveParameterValue($parameter, $position, $toolData);

            if ($value === null && $parameter->isDefaultValueAvailable()) {
                continue;
            }

            $arguments[] = $value;
        }

        return $arguments;
    }

    private function resolveParameterValue(ReflectionParameter $parameter, int $position, ToolData $toolData): mixed
    {
        $parameterName = strtolower($parameter->getName());

        return match (true) {
            $parameterName === 'data',
            $parameterName === 'tooldata' => $toolData,

            $parameterName === 'input' => $toolData->input,
            $parameterName === 'bounded' => $toolData->bounded,
            $parameterName === 'context' => $toolData->context,

            $position === 0 => $toolData->input,
            $position === 1 => $toolData->bounded,
            $position === 2 => $toolData->context,

            default => null,
        };
    }

    private function parameterAcceptsType(ReflectionParameter $parameter, string $class): bool
    {
        return $this->typeAcceptsClass($parameter->getType(), $class);
    }

    private function typeAcceptsClass(?ReflectionType $type, string $class): bool
    {
        if ($type === null) {
            return false;
        }

        if ($type instanceof ReflectionUnionType) {
            foreach ($type->getTypes() as $unionType) {
                if ($this->typeAcceptsClass($unionType, $class)) {
                    return true;
                }
            }

            return false;
        }

        if (!$type instanceof ReflectionNamedType) {
            return false;
        }

        if ($type->isBuiltin()) {
            return false;
        }

        $typeName = $type->getName();

        return $typeName === $class || is_subclass_of($class, $typeName);
    }
}
