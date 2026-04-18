<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Agent;

final class AgentTurnRequest
{
    /**
     * @param array<int, AgentConversationMessage> $messages
     * @param array<int, AgentToolDefinition> $tools
     * @param array<string, mixed> $providerConfig
     */
    public function __construct(
        public readonly string $provider,
        public readonly string $model,
        public readonly array $providerConfig,
        public readonly array $messages,
        public readonly array $tools,
        public readonly bool $requireToolCall,
    ) {
    }
}
