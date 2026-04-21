<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools;

use BackedEnum;
use Illuminate\Support\Str;
use Prism\Prism\Schema\RawSchema;
use Prism\Prism\Tool;
use ReflectionClass;
use ReflectionMethod;
use ReflectionNamedType;
use ReflectionParameter;
use ReflectionProperty;
use RuntimeException;
use Spatie\LaravelData\Contracts\BaseData;
use Spatie\LaravelData\Enums\DataTypeKind;
use Spatie\LaravelData\Support\DataConfig;
use Spatie\LaravelData\Support\DataProperty;
use Spatie\LaravelData\Support\DataPropertyType;
use Superwire\Laravel\Contracts\ToolBoundInputData;
use Superwire\Laravel\Contracts\ToolInputData;

abstract class AbstractTool implements WorkflowTool
{
    public function name(): string
    {
        return Str::snake(class_basename(static::class));
    }

    public static function description(): string
    {
        $description = static::descriptionFromClassAttributes();

        if ($description !== null) {
            return $description;
        }

        return sprintf('Use `%s` to complete this action.', Str::headline(class_basename(static::class)));
    }

    public function toPrismTool(array $boundArguments = []): Tool
    {
        $tool = new Tool();

        $tool
            ->as($this->name())
            ->for(static::description())
            ->withoutErrorHandling();

        foreach ($this->agentInputSchemas() as $parameterSchema) {
            $tool->withParameter(new RawSchema($parameterSchema[ 'name' ], $parameterSchema[ 'schema' ]), $parameterSchema[ 'required' ]);
        }

        return $tool->using(function (...$agentArguments) use ($boundArguments): string {

            $result = $this->execute(
                agentInput: static::resolveAgentInput($agentArguments),
                boundInput: static::resolveBoundInput($boundArguments),
            );

            return json_encode($result, JSON_THROW_ON_ERROR);

        });
    }

    public function execute(mixed $agentInput = null, mixed $boundInput = null): mixed
    {
        $executionMethod = $this->executionMethod();
        $arguments = [];

        foreach ($executionMethod->getParameters() as $parameter) {

            $parameterClass = $this->parameterClassName($parameter);

            if ($parameterClass !== null && is_a($parameterClass, ToolInputData::class, true)) {

                $arguments[] = $agentInput;

                continue;

            }

            if ($parameterClass !== null && is_a($parameterClass, ToolBoundInputData::class, true)) {

                $arguments[] = $boundInput;

                continue;

            }

            throw new RuntimeException(sprintf(
                'Tool `%s` has unsupported execution parameter `%s`. Use %s or %s implementations only.',
                $this->name(),
                $parameter->getName(),
                ToolInputData::class,
                ToolBoundInputData::class,
            ));

        }

        $result = $this->{$executionMethod->getName()}(...$arguments);

        return $this->normalizeExecutionResult($result);
    }

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

    public static function outputClass(): ?string
    {
        $returnType = static::executionMethodReflection()->getReturnType();

        if (!$returnType instanceof ReflectionNamedType || $returnType->isBuiltin()) {
            return null;
        }

        return $returnType->getName();
    }

    protected function success(array $payload): WorkflowToolResult
    {
        return WorkflowToolResult::success($payload);
    }

    protected function fail(string $reason, array $context = []): WorkflowToolResult
    {
        return WorkflowToolResult::fail($reason, $context);
    }

    /**
     * @return array<int, array{name: string, schema: array<string, mixed>, required: bool}>
     */
    private function agentInputSchemas(): array
    {
        $agentInputClass = static::agentInputClass();

        if ($agentInputClass === null || !is_a($agentInputClass, BaseData::class, true)) {
            return [];
        }

        $dataClass = app(DataConfig::class)->getDataClass($agentInputClass);
        $schemas = [];

        foreach ($dataClass->properties as $property) {

            $schemas[] = [
                'name' => $property->name,
                'schema' => $this->schemaForDataProperty($property),
                'required' => !$property->hasDefaultValue && !$property->type->isNullable,
            ];

        }

        return $schemas;
    }

    /**
     * @return array<string, mixed>
     */
    private function schemaForDataProperty(DataProperty $property): array
    {
        $schema = $this->schemaForPropertyType($property->type, $property->className, $property->name);
        $description = $this->descriptionFromProperty($property->className, $property->name);

        if ($description !== null) {
            $schema[ 'description' ] = $description;
        }

        return $schema;
    }

    /**
     * @return array<string, mixed>
     */
    private function schemaForPropertyType(DataPropertyType $type, string $className, string $propertyName): array
    {
        if ($type->kind->isDataObject() && $type->dataClass !== null) {
            return $this->schemaForDataClass($type->dataClass);
        }

        if ($type->kind->isDataCollectable() && $type->iterableItemType !== null) {

            return [
                'type' => 'array',
                'items' => $this->schemaForIterableItemType($type->iterableItemType),
            ];

        }

        if ($type->acceptsType('string')) {
            return [ 'type' => 'string' ];
        }

        if ($type->acceptsType('int')) {
            return [ 'type' => 'integer' ];
        }

        if ($type->acceptsType('float')) {
            return [ 'type' => 'number' ];
        }

        if ($type->acceptsType('bool')) {
            return [ 'type' => 'boolean' ];
        }

        if ($type->kind === DataTypeKind::Array) {
            return [ 'type' => 'array' ];
        }

        $reflectionProperty = new ReflectionProperty($className, $propertyName);
        $reflectionType = $reflectionProperty->getType();

        if ($reflectionType instanceof ReflectionNamedType && enum_exists($reflectionType->getName())) {
            return $this->schemaForEnum($reflectionType->getName());
        }

        return [ 'type' => 'string' ];
    }

    /**
     * @return array<string, mixed>
     */
    private function schemaForDataClass(string $dataClassName): array
    {
        $dataClass = app(DataConfig::class)->getDataClass($dataClassName);
        $properties = [];
        $required = [];

        foreach ($dataClass->properties as $property) {

            $properties[ $property->name ] = $this->schemaForDataProperty($property);

            if (!$property->hasDefaultValue && !$property->type->isNullable) {
                $required[] = $property->name;
            }

        }

        return array_filter([
            'type' => 'object',
            'properties' => $properties,
            'required' => $required,
            'additionalProperties' => false,
        ], static fn (mixed $value): bool => $value !== []);
    }

    /**
     * @return array<string, mixed>
     */
    private function schemaForIterableItemType(string $iterableItemType): array
    {
        if (is_a($iterableItemType, BaseData::class, true)) {
            return $this->schemaForDataClass($iterableItemType);
        }

        if (enum_exists($iterableItemType)) {
            return $this->schemaForEnum($iterableItemType);
        }

        return match ($iterableItemType) {
            'string' => [ 'type' => 'string' ],
            'int' => [ 'type' => 'integer' ],
            'float' => [ 'type' => 'number' ],
            'bool' => [ 'type' => 'boolean' ],
            default => [ 'type' => 'string' ],
        };
    }

    /**
     * @param class-string<BackedEnum> $enumClass
     * @return array<string, mixed>
     */
    private function schemaForEnum(string $enumClass): array
    {
        $enumValues = array_map(static fn (BackedEnum $case): string|int => $case->value, $enumClass::cases());
        $schemaType = is_int($enumValues[ 0 ] ?? null) ? 'integer' : 'string';

        return [
            'type' => $schemaType,
            'enum' => $enumValues,
        ];
    }

    private function normalizeExecutionResult(mixed $result): mixed
    {
        if ($result instanceof WorkflowToolResult) {

            if (!$result->isSuccess()) {
                throw new RuntimeException((string) $result->reason());
            }

            return $result->payload ?? [];

        }

        if ($result instanceof BaseData && method_exists($result, 'toArray')) {
            return $result->toArray();
        }

        return $result;
    }

    private static function agentInputClass(): ?string
    {
        return static::parameterClassMatchingInterface(ToolInputData::class);
    }

    private static function boundInputClass(): ?string
    {
        return static::parameterClassMatchingInterface(ToolBoundInputData::class);
    }

    private static function parameterClassMatchingInterface(string $interfaceName): ?string
    {
        foreach (static::executionMethodReflection()->getParameters() as $parameter) {

            $parameterClass = static::parameterClassFromReflection($parameter);

            if ($parameterClass !== null && is_a($parameterClass, $interfaceName, true)) {
                return $parameterClass;
            }

        }

        return null;
    }

    private static function executionMethodReflection(): ReflectionMethod
    {
        $reflectionClass = new ReflectionClass(static::class);

        if ($reflectionClass->hasMethod('invoke') && $reflectionClass->getMethod('invoke')->getDeclaringClass()->getName() !== self::class) {
            return $reflectionClass->getMethod('invoke');
        }

        if ($reflectionClass->hasMethod('handle') && $reflectionClass->getMethod('handle')->getDeclaringClass()->getName() !== self::class) {
            return $reflectionClass->getMethod('handle');
        }

        throw new RuntimeException(sprintf('Tool `%s` must define `invoke()` or `handle()`.', static::class));
    }

    private function executionMethod(): ReflectionMethod
    {
        return static::executionMethodReflection();
    }

    private function parameterClassName(ReflectionParameter $parameter): ?string
    {
        return static::parameterClassFromReflection($parameter);
    }

    private static function parameterClassFromReflection(ReflectionParameter $parameter): ?string
    {
        $type = $parameter->getType();

        if (!$type instanceof ReflectionNamedType || $type->isBuiltin()) {
            return null;
        }

        return $type->getName();
    }

    private static function resolveDataObject(string $className, array $payload): mixed
    {
        if (is_a($className, BaseData::class, true)) {
            return $className::from($payload);
        }

        return app()->make($className, $payload);
    }

    private static function descriptionFromClassAttributes(): ?string
    {
        $reflectionClass = new ReflectionClass(static::class);

        foreach ($reflectionClass->getAttributes() as $attribute) {

            $instance = $attribute->newInstance();

            if (property_exists($instance, 'text') && is_string($instance->text)) {
                return $instance->text;
            }

        }

        return null;
    }

    private function descriptionFromProperty(string $className, string $propertyName): ?string
    {
        $reflectionProperty = new ReflectionProperty($className, $propertyName);

        foreach ($reflectionProperty->getAttributes() as $attribute) {

            $instance = $attribute->newInstance();

            if (property_exists($instance, 'text') && is_string($instance->text)) {
                return $instance->text;
            }

        }

        return null;
    }
}
