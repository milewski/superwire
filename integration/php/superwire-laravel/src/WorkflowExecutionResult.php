<?php

declare(strict_types = 1);

namespace Superwire\Laravel;

final readonly class WorkflowExecutionResult
{
    /**
     * @param array<string, mixed> $context
     * @param array<string, AgentExecutionResult> $agents
     */
    public function __construct(
        public mixed $output,
        public array $agents,
        public array $context = [],
    )
    {
    }
}
