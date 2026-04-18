<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Workflow;

final class WorkflowExecutionResult
{
    /**
     * @param array<string, mixed> $output
     * @param array<string, mixed> $agentOutputs
     * @param array<string, mixed> $agentContexts
     * @param array<string, array{metadata: array<string, mixed>}> $agentMetadata
     * @param list<array<string, mixed>> $executionHistory
     */
    public function __construct(
        public readonly array $output,
        public readonly array $agentOutputs,
        public readonly array $agentContexts,
        public readonly array $agentMetadata = [],
        public readonly array $executionHistory = [],
    ) {
    }
}
