<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Support;

use InvalidArgumentException;
use JsonException;
use stdClass;
use Swaggest\JsonSchema\InvalidValue;
use Swaggest\JsonSchema\Schema;
use Throwable;

final class JsonSchemaFactory
{
    /**
     * @param array<string, mixed> $definition
     */
    public static function fromArray(array $definition, string $name): Schema
    {
        try {

            return Schema::import(self::toObject($definition));

        } catch (Throwable $throwable) {

            throw new InvalidArgumentException(sprintf('Invalid json schema for %s.', $name), previous: $throwable);

        }
    }

    /**
     * @return array<string, mixed>
     */
    public static function toArray(Schema $schema): array
    {
        try {

            $encodedSchema = json_encode($schema, JSON_THROW_ON_ERROR);

            return json_decode($encodedSchema, true, flags: JSON_THROW_ON_ERROR);

        } catch (JsonException $jsonException) {

            throw new InvalidArgumentException('Unable to serialize json schema.', previous: $jsonException);

        }
    }

    /**
     * @param array<string, mixed> $values
     */
    public static function validate(Schema $schema, array $values, string $name): void
    {
        try {

            $schema->in(self::toObject($values));

        } catch (InvalidValue $invalidValue) {

            throw new InvalidArgumentException(sprintf('%s is invalid: %s', $name, $invalidValue->getMessage()), previous: $invalidValue);

        } catch (Throwable $throwable) {

            throw new InvalidArgumentException(sprintf('Unable to validate %s.', $name), previous: $throwable);

        }
    }

    private static function toObject(mixed $value): mixed
    {
        if (!is_array($value)) {
            return $value;
        }

        if (array_is_list($value)) {
            return array_map(self::toObject(...), $value);
        }

        $objectValue = new stdClass();

        foreach ($value as $key => $nestedValue) {
            $objectValue->{$key} = self::toObject($nestedValue);
        }

        return $objectValue;
    }
}
