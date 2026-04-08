<?php

namespace Superwire\Laravel\Tests\Concerns;

use JsonException;
use Superwire\Laravel\Contracts\Tool;
use Swaggest\JsonSchema\Schema;
use Throwable;

trait AssertsToolSchemas
{
    /**
     * @param class-string<Tool> $toolClassName
     * @param array<string, mixed> $outputPayload
     */
    protected function assertOutputMatchesSchema(
        string $toolClassName,
        array $outputPayload,
        bool $allowUndeclaredProperties = false,
    ): void
    {
        if (!is_subclass_of($toolClassName, Tool::class)) {
            $this->fail(sprintf('`%s` must implement `%s` to assert output schema', $toolClassName, Tool::class));
        }

        $this->assertPayloadMatchesSchema(
            $toolClassName::outputSchema(),
            $outputPayload,
            sprintf('output payload for `%s`', $toolClassName),
            $allowUndeclaredProperties,
        );
    }

    /**
     * @param array<string, mixed> $payload
     */
    protected function assertPayloadMatchesSchema(
        Schema $schema,
        array $payload,
        string $payloadContext = 'payload',
        bool $allowUndeclaredProperties = false,
    ): void
    {
        $schemaPayload = $this->schemaPayload($schema);

        if (!$allowUndeclaredProperties) {
            $this->assertNoUndeclaredProperties($schemaPayload, $payload, '$');
        }

        try {
            $schema->in($this->schemaValidationValue($payload));
        } catch (Throwable $throwable) {
            $this->fail(sprintf('%s does not match schema: %s', $payloadContext, $throwable->getMessage()));
        }

        $this->addToAssertionCount(1);
    }

    /**
     * @param array<string, mixed> $schemaPayload
     * @param mixed $payload
     */
    private function assertNoUndeclaredProperties(array $schemaPayload, mixed $payload, string $jsonPath): void
    {
        if (is_array($payload) && !$this->isListArray($payload) && $this->schemaRepresentsObject($schemaPayload)) {
            $propertySchemas = $this->schemaObjectProperties($schemaPayload);

            foreach ($payload as $propertyName => $propertyValue) {
                if (!array_key_exists($propertyName, $propertySchemas)) {
                    $this->fail(sprintf('unexpected property `%s` found at `%s`', $propertyName, $jsonPath));
                }

                $propertySchema = $propertySchemas[$propertyName];

                if (is_array($propertySchema)) {
                    $this->assertNoUndeclaredProperties($propertySchema, $propertyValue, $jsonPath . '.' . $propertyName);
                }
            }
        }

        if (is_array($payload) && $this->isListArray($payload) && isset($schemaPayload['items']) && is_array($schemaPayload['items'])) {
            foreach ($payload as $itemIndex => $itemPayload) {
                $this->assertNoUndeclaredProperties($schemaPayload['items'], $itemPayload, $jsonPath . '[' . $itemIndex . ']');
            }
        }
    }

    /**
     * @param array<string, mixed> $schemaPayload
     */
    private function schemaRepresentsObject(array $schemaPayload): bool
    {
        if (isset($schemaPayload['properties']) && is_array($schemaPayload['properties'])) {
            return true;
        }

        if (!array_key_exists('type', $schemaPayload)) {
            return false;
        }

        $schemaType = $schemaPayload['type'];

        if (is_string($schemaType)) {
            return $schemaType === 'object';
        }

        if (is_array($schemaType)) {
            return in_array('object', $schemaType, true);
        }

        return false;
    }

    /**
     * @param array<string, mixed> $schemaPayload
     * @return array<string, array<string, mixed>>
     */
    private function schemaObjectProperties(array $schemaPayload): array
    {
        if (!isset($schemaPayload['properties']) || !is_array($schemaPayload['properties'])) {
            return [];
        }

        $propertySchemas = [];

        foreach ($schemaPayload['properties'] as $propertyName => $propertySchema) {
            if (!is_string($propertyName) || !is_array($propertySchema)) {
                continue;
            }

            $propertySchemas[$propertyName] = $propertySchema;
        }

        return $propertySchemas;
    }

    /**
     * @param array<mixed> $value
     */
    private function isListArray(array $value): bool
    {
        return array_is_list($value);
    }

    /**
     * @return array<string, mixed>
     */
    private function schemaPayload(Schema $schema): array
    {
        try {
            $encodedSchema = json_encode($schema, JSON_THROW_ON_ERROR);
            $decodedSchema = json_decode($encodedSchema, true, 512, JSON_THROW_ON_ERROR);
        } catch (JsonException $jsonException) {
            $this->fail(sprintf('failed to serialize schema: %s', $jsonException->getMessage()));
        }

        if (!is_array($decodedSchema)) {
            $this->fail('schema must serialize to a JSON object');
        }

        return $decodedSchema;
    }

    /**
     * @param array<string, mixed> $payload
     * @return mixed
     */
    private function schemaValidationValue(array $payload): mixed
    {
        try {
            return json_decode(json_encode($payload, JSON_THROW_ON_ERROR), false, 512, JSON_THROW_ON_ERROR);
        } catch (JsonException $jsonException) {
            $this->fail(sprintf('failed to serialize payload for schema validation: %s', $jsonException->getMessage()));
        }
    }
}
