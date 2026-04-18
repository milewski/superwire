<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Agent;

final class AgentConversationMessage
{
    /**
     * @param array<string, mixed> $payload
     */
    public function __construct(
        public readonly string $role,
        public readonly array $payload,
    ) {
    }
}
