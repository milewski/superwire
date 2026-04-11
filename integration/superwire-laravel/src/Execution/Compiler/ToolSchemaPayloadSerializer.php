<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Execution\Compiler;

use JsonException;
use Superwire\Laravel\Exceptions\ToolBuildException;
use Swaggest\JsonSchema\Schema;

final class ToolSchemaPayloadSerializer
{
    /**
     * @throws JsonException
     * @return array<string, mixed>
     */
    public function payload(Schema $schema): array
    {
        $serializedSchema = json_encode($schema, JSON_THROW_ON_ERROR);
        $decodedSchema = json_decode($serializedSchema, true, 512, JSON_THROW_ON_ERROR);

        if (!is_array($decodedSchema)) {
            throw new ToolBuildException('tool schema must serialize to a json object');
        }

        return $decodedSchema;
    }

    /**
     * @throws JsonException
     */
    public function escapedJsonString(Schema $schema): string
    {
        $serializedSchema = json_encode($schema, JSON_THROW_ON_ERROR);

        return addcslashes($serializedSchema, '\\"');
    }

    public function escapedString(string $value): string
    {
        return addcslashes($value, '\\"');
    }
}
