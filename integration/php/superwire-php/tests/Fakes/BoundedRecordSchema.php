<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Tests\Fakes;

use RuntimeException;
use Swaggest\JsonSchema\Exception;
use Swaggest\JsonSchema\Schema;

final class BoundedRecordSchema
{
    public static function schema(): Schema
    {
        return Schema::object()
            ->setProperty('workspace_id', Schema::integer())
            ->setProperty('record_id', Schema::integer())
            ->setRequired([ 'workspace_id', 'record_id' ])
            ->setAdditionalProperties(false);
    }

    /**
     * @return array<string, mixed>
     */
    public static function json_schema(): array
    {
        $encodedSchema = json_encode(self::schema(), JSON_THROW_ON_ERROR);
        $decodedSchema = json_decode($encodedSchema, true);

        if (!is_array($decodedSchema)) {
            throw new RuntimeException('invalid schema export from fluent schema object');
        }

        return $decodedSchema;
    }

    /**
     * @param array<string, mixed> $arguments
     */
    public static function validate(array $arguments): void
    {
        try {

            self::schema()->in((object) $arguments);

        } catch (Exception $schemaException) {

            throw new RuntimeException('invalid schema: ' . $schemaException->getMessage(), previous: $schemaException);

        }
    }
}
