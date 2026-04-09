<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data;

final readonly class ToolInvocationRequest
{
    /**
     * @param array<string, mixed> $agentInput
     * @param array<string, mixed> $boundInput
     */
    public function __construct(
        public string $toolName,
        public array $agentInput,
        public array $boundInput,
    )
    {
    }
}
