<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools\Concerns;

use BackedEnum;
use ReflectionNamedType;
use ReflectionProperty;
use Spatie\LaravelData\Contracts\BaseData;
use Spatie\LaravelData\Enums\DataTypeKind;
use Spatie\LaravelData\Support\DataConfig;
use Spatie\LaravelData\Support\DataProperty;
use Spatie\LaravelData\Support\DataPropertyType;
use Superwire\Laravel\Support\JsonSchemaFactory;
use Swaggest\JsonSchema\Schema;

trait InfersToolInputSchemas
{
    /**
     * @return array<int, array{name: string, schema: Schema, required: bool}>
     */
    protected function agentInputSchemas(): array
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

    protected function schemaForDataProperty(DataProperty $property): Schema
    {
        $schema = $this->schemaForPropertyType($property->type, $property->className, $property->name);
        $description = $this->descriptionFromProperty($property->className, $property->name);

        if ($description === null) {
            return $schema;
        }

        $schemaDefinition = JsonSchemaFactory::toArray($schema);
        $schemaDefinition[ 'description' ] = $description;

        return JsonSchemaFactory::fromArray($schemaDefinition, sprintf('tool property schema `%s`', $property->name));
    }

    protected function schemaForPropertyType(DataPropertyType $type, string $className, string $propertyName): Schema
    {
        if ($type->kind->isDataObject() && $type->dataClass !== null) {
            return $this->schemaForDataClass($type->dataClass);
        }

        if ($type->kind->isDataCollectable() && $type->iterableItemType !== null) {

            return $this->schemaFromDefinition([
                'type' => 'array',
                'items' => $this->schemaForIterableItemType($type->iterableItemType),
            ], sprintf('tool iterable property schema `%s`', $propertyName));

        }

        if ($type->acceptsType('string')) {
            return $this->schemaFromDefinition([ 'type' => 'string' ], sprintf('tool string property schema `%s`', $propertyName));
        }

        if ($type->acceptsType('int')) {
            return $this->schemaFromDefinition([ 'type' => 'integer' ], sprintf('tool integer property schema `%s`', $propertyName));
        }

        if ($type->acceptsType('float')) {
            return $this->schemaFromDefinition([ 'type' => 'number' ], sprintf('tool number property schema `%s`', $propertyName));
        }

        if ($type->acceptsType('bool')) {
            return $this->schemaFromDefinition([ 'type' => 'boolean' ], sprintf('tool boolean property schema `%s`', $propertyName));
        }

        if ($type->kind === DataTypeKind::Array) {

            $definition = [ 'type' => 'array' ];

            if ($type->iterableItemType !== null) {
                $definition[ 'items' ] = $this->schemaForIterableItemType($type->iterableItemType);
            }

            return $this->schemaFromDefinition($definition, sprintf('tool array property schema `%s`', $propertyName));

        }

        $reflectionProperty = new ReflectionProperty($className, $propertyName);
        $reflectionType = $reflectionProperty->getType();

        if ($reflectionType instanceof ReflectionNamedType && enum_exists($reflectionType->getName())) {
            return $this->schemaForEnum($reflectionType->getName());
        }

        return $this->schemaFromDefinition([ 'type' => 'string' ], sprintf('tool fallback property schema `%s`', $propertyName));
    }

    protected function schemaForDataClass(string $dataClassName): Schema
    {
        $dataClass = app(DataConfig::class)->getDataClass($dataClassName);
        $properties = [];
        $required = [];

        foreach ($dataClass->properties as $property) {

            $properties[ $property->name ] = JsonSchemaFactory::toArray($this->schemaForDataProperty($property));

            if (!$property->hasDefaultValue && !$property->type->isNullable) {
                $required[] = $property->name;
            }

        }

        return $this->schemaFromDefinition(array_filter([
            'type' => 'object',
            'properties' => $properties,
            'required' => $required,
            'additionalProperties' => false,
        ], static fn (mixed $value): bool => $value !== []), sprintf('tool data class schema `%s`', $dataClassName));
    }

    /**
     * @return array<string, mixed>
     */
    protected function schemaForIterableItemType(string $iterableItemType): array
    {
        if (is_a($iterableItemType, BaseData::class, true)) {
            return JsonSchemaFactory::toArray($this->schemaForDataClass($iterableItemType));
        }

        if (enum_exists($iterableItemType)) {
            return JsonSchemaFactory::toArray($this->schemaForEnum($iterableItemType));
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
     */
    protected function schemaForEnum(string $enumClass): Schema
    {
        $enumValues = array_map(static fn (BackedEnum $case): string|int => $case->value, $enumClass::cases());
        $schemaType = is_int($enumValues[ 0 ] ?? null) ? 'integer' : 'string';

        return $this->schemaFromDefinition([
            'type' => $schemaType,
            'enum' => $enumValues,
        ], sprintf('tool enum schema `%s`', $enumClass));
    }

    /**
     * @param array<string, mixed> $definition
     */
    protected function schemaFromDefinition(array $definition, string $name): Schema
    {
        return JsonSchemaFactory::fromArray($definition, $name);
    }
}
