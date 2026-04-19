<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Agent;

final readonly class AgentTurnRequest
{
    /**
     * @param array<int, AgentConversationMessage> $messages
     * @param array<int, AgentToolDefinition> $tools
     * @param array<string, mixed> $providerConfig
     */
    public function __construct(
        public string $provider,
        public string $model,
        public array $providerConfig,
        public array $messages,
        public array $tools,
        public bool $requireToolCall,
    )
    {
    }
}
