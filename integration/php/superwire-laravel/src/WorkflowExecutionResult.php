<?php

declare(strict_types = 1);

namespace Superwire\Laravel;

final class WorkflowExecutionResult
{
    /**
     * @param array<string, mixed> $output
     * @param array<string, AgentExecutionResult> $agents
     */
    public function __construct(
        public readonly array $output,
        public readonly array $agents,
    ) {
    }
}
