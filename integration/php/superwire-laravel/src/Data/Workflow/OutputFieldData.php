<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Workflow;

use Superwire\Laravel\Data\Workflow\Concerns\ValidatesPayload;

final class OutputFieldData
{
    use ValidatesPayload;

    public function __construct(
        public readonly array $workflowType,
        public readonly array $jsonSchema,
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
            jsonSchema: self::array($payload, 'json_schema'),
        );
    }
}