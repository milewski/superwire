<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Contracts;

use Superwire\Contracts\Workflow\WorkflowDefinition;
use Superwire\Contracts\Workflow\WorkflowExecutionResult;

interface WorkflowRunnerInterface
{
    /**
     * @param array<string, mixed> $input
     * @param array<string, mixed> $secrets
     */
    public function run(WorkflowDefinition $workflowDefinition, array $input = [], array $secrets = []): WorkflowExecutionResult;
}
