<?php

declare(strict_types=1);

namespace Superwire\Contracts\Support;

use Superwire\Contracts\Exception\InvalidWorkflowDefinitionException;
use Superwire\Contracts\WorkflowDefinition;

final class JsonWorkflowDecoder
{
    /**
     * @param array<string, mixed> $payload
     */
    public function decodeFromArray(array $payload): WorkflowDefinition
    {
        return WorkflowDefinition::fromArray($payload);
    }

    public function decodeFromJson(string $jsonPayload): WorkflowDefinition
    {
        $decodedPayload = json_decode($jsonPayload, true);

        if (!is_array($decodedPayload)) {
            throw new InvalidWorkflowDefinitionException('workflow json payload must decode into an object');
        }

        return WorkflowDefinition::fromArray($decodedPayload);
    }
}
