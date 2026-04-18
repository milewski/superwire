<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Agent;

final class AgentTurnResponse
{
    /**
     * @param array<int, AgentToolCall> $toolCalls
     * @param array<int, AgentToolResult> $toolResults
     */
    public function __construct(
        public readonly array $toolCalls,
        public readonly string $text = '',
        public readonly array $toolResults = [],
    )
    {
    }
}
