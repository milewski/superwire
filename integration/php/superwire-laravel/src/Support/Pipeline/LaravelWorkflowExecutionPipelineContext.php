<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Support\Pipeline;

use Superwire\Contracts\Workflow\WorkflowDefinition;

final class LaravelWorkflowExecutionPipelineContext
{
    /**
     * @var list<list<string>>
     */
    public array $executionBatches = [];

    /**
     * @var array<string, mixed>
     */
    public array $agentOutputs = [];

    /**
     * @var array<string, mixed>
     */
    public array $agentContexts = [];

    /**
     * @var array<string, array<string, mixed>>
     */
    public array $agentMetadata = [];

    /**
     * @var list<array<string, mixed>>
     */
    public array $executionHistory = [];

    /**
     * @var array<string, mixed>
     */
    public array $resolvedOutput = [];

    /**
     * @param array<string, mixed> $input
     * @param array<string, mixed> $secrets
     */
    public function __construct(
        public readonly WorkflowDefinition $workflowDefinition,
        public readonly array $input,
        public readonly array $secrets,
    ) {
    }
}
