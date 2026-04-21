<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Workflow;

use Superwire\Laravel\Data\Concerns\ValidatesPayload;
use Superwire\Laravel\Support\JsonSchemaFactory;
use Swaggest\JsonSchema\Schema;

final class WorkflowValueDefinition
{
    use ValidatesPayload;

    /**
     * @param array<string, mixed> $workflowType
     */
    public function __construct(
        public readonly array $workflowType,
        public readonly Schema $jsonSchema,
    ) {
    }

    /**
     * @param array<string, mixed> $payload
     */
    public static function fromArray(array $payload): self
    {
        return new self(
            workflowType: self::array($payload, 'workflow_type'),
            jsonSchema: JsonSchemaFactory::fromArray(self::array($payload, 'json_schema'), 'workflow value definition'),
        );
    }

    /**
     * @param array<string, mixed> $values
     */
    public function validateValues(array $values, string $name): void
    {
        JsonSchemaFactory::validate($this->jsonSchema, $values, $name);
    }
}
