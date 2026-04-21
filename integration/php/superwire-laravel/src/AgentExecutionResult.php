<?php

declare(strict_types = 1);

namespace Superwire\Laravel;

final readonly class AgentExecutionResult
{
    /**
     * @param array<int, array<string, mixed>> $messages
     * @param list<AgentExecutionResult> $iterations
     */
    public function __construct(
        public mixed $output,
        public array $messages = [],
        public array $iterations = [],
    )
    {
    }
}
