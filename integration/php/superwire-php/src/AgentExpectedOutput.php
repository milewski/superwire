<?php

declare(strict_types=1);

namespace Superwire\Contracts;

use Superwire\Contracts\Exception\InvalidWorkflowDefinitionException;

final class AgentExpectedOutput
{
    /**
     * @param array<string, mixed> $workflowType
     * @param array<string, mixed> $jsonSchema
     */
    public function __construct(
        public readonly array $workflowType,
        public readonly array $jsonSchema,
    ) {
    }

    /**
     * @param array<string, mixed> $contract
     */
    public static function fromContract(array $contract): self
    {
        $workflowType = $contract['workflow_type'] ?? null;
        $jsonSchema = $contract['json_schema'] ?? null;

        if (!is_array($workflowType) || !is_array($jsonSchema)) {
            throw new InvalidWorkflowDefinitionException('agent expected output contract requires `workflow_type` and `json_schema` objects');
        }

        return new self($workflowType, $jsonSchema);
    }

    public function kind(): string
    {
        $kind = $this->workflowType['kind'] ?? null;

        if (is_string($kind)) {
            return $kind;
        }

        throw new InvalidWorkflowDefinitionException('agent expected output workflow_type requires string `kind`');
    }

    public function isPlainString(): bool
    {
        return $this->kind() === 'string';
    }
}
