<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools\Concerns;

use ReflectionClass;
use ReflectionProperty;

trait ResolvesToolDescriptions
{
    protected static function descriptionFromClassAttributes(): ?string
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

    protected function descriptionFromProperty(string $className, string $propertyName): ?string
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
