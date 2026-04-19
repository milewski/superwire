<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Agent;

final readonly class AgentTurnResponse
{
    /**
     * @param array<int, AgentToolCall> $toolCalls
     * @param array<int, AgentToolResult> $toolResults
     */
    public function __construct(
        public array $toolCalls,
        public string $text = '',
        public array $toolResults = [],
    )
    {
    }
}
