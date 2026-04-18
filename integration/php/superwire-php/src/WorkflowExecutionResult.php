<?php

declare(strict_types=1);

namespace Superwire\Contracts;

final class WorkflowExecutionResult
{
    /**
     * @param array<string, mixed> $output
     * @param array<string, mixed> $agentOutputs
     * @param array<string, mixed> $agentContexts
     */
    public function __construct(
        public readonly array $output,
        public readonly array $agentOutputs,
        public readonly array $agentContexts,
    ) {
    }
}
