<?php

declare(strict_types = 1);

namespace Superwire\Laravel;

final class AgentExecutionResult
{
    /**
     * @param array<int, array<string, mixed>> $messages
     * @param list<AgentExecutionResult> $iterations
     */
    public function __construct(
        public readonly mixed $output,
        public readonly array $messages = [],
        public readonly array $iterations = [],
    )
    {
    }
}
