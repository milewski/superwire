<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Agent;

final readonly class AgentToolCall
{
    /**
     * @param array<string, mixed> $arguments
     */
    public function __construct(
        public string $id,
        public string $name,
        public array $arguments,
    )
    {
    }
}
