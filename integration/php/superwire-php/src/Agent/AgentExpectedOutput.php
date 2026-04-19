<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Agent;

use JsonException;
use Superwire\Contracts\Exception\InvalidWorkflowDefinitionException;
use Swaggest\JsonSchema\InvalidValue;
use Swaggest\JsonSchema\Schema;

final class AgentExpectedOutput
{
    /**
     * @param array<string, mixed> $workflowType
     */
    public function __construct(
        public readonly array $workflowType,
        public readonly Schema $jsonSchema,
    )
    {
    }

    /**
     * @param array<string, mixed> $contract
     */
    public static function fromContract(array $contract): self
    {
        $workflowType = $contract[ 'workflow_type' ] ?? null;
        $jsonSchema = $contract[ 'json_schema' ] ?? null;

        if (!is_array($workflowType) || !is_array($jsonSchema)) {
            throw new InvalidWorkflowDefinitionException('agent expected output contract requires `workflow_type` and `json_schema` objects');
        }

        return new self(
            workflowType: $workflowType,
            jsonSchema: self::schemaFromArray($jsonSchema),
        );
    }

    public function kind(): string
    {
        $kind = $this->workflowType[ 'kind' ] ?? null;

        if (is_string($kind)) {
            return $kind;
        }

        throw new InvalidWorkflowDefinitionException('agent expected output workflow_type requires string `kind`');
    }

    public function isPlainString(): bool
    {
        return $this->kind() === 'string';
    }

    /**
     * @param array<string, mixed> $schemaPayload
     */
    private static function schemaFromArray(array $schemaPayload): Schema
    {
        try {

            return Schema::import(json_decode(json_encode($schemaPayload, JSON_THROW_ON_ERROR), false, 512, JSON_THROW_ON_ERROR));

        } catch (InvalidValue|JsonException $error) {

            throw new InvalidWorkflowDefinitionException(
                'agent expected output `json_schema` must be a valid JSON Schema object',
                previous: $error,
            );

        }
    }
}
