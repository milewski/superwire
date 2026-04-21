<?php

declare(strict_types = 1);

namespace Superwire\Laravel;

final readonly class WorkflowExecutionResult
{
    /**
     * @param array<string, mixed> $output
     * @param array<string, AgentExecutionResult> $agents
     */
    public function __construct(
        public array $output,
        public array $agents,
    )
    {
    }
}
