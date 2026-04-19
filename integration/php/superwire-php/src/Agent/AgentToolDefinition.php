<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Agent;

final class AgentToolDefinition
{
    /**
     * @param array<string, mixed> $parametersSchema
     */
    public function __construct(
        public readonly string $name,
        public readonly string $description,
        public readonly array $parametersSchema,
        public readonly bool $strict = true,
    )
    {
    }
}
