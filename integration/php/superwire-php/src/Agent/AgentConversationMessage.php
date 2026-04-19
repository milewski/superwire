<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Agent;

final readonly class AgentConversationMessage
{
    public function __construct(
        public ConversationRole $role,
        public array $payload,
    )
    {
    }
}
