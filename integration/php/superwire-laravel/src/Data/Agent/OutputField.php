<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Agent;

use Superwire\Laravel\Data\Concerns\ValidatesPayload;
use Superwire\Laravel\Support\JsonSchemaFactory;
use Swaggest\JsonSchema\Schema;

final class OutputField
{
    use ValidatesPayload;

    public function __construct(
        public readonly array $workflowType,
        public readonly Schema $jsonSchema,
    )
    {
    }

    /**
     * @param array<string, mixed> $payload
     */
    public static function fromArray(array $payload): self
    {
        return new self(
            workflowType: self::array($payload, 'workflow_type'),
            jsonSchema: JsonSchemaFactory::fromArray(self::array($payload, 'json_schema'), 'output field data'),
        );
    }
}
