<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Agent;

final class AgentToolCall
{
    /**
     * @param array<string, mixed> $arguments
     */
    public function __construct(
        public readonly string $id,
        public readonly string $name,
        public readonly array $arguments,
    ) {
    }
}
