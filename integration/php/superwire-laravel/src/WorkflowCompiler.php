<?php

declare(strict_types = 1);

namespace Superwire\Laravel;

use Superwire\Laravel\Data\Workflow\WorkflowDefinition;

final readonly class WorkflowCompiler
{
    public function __construct(
        private WorkflowExecutor $workflowExecutor,
    )
    {
    }

    public function compile(string $workflowPath): WorkflowDefinition
    {
        return WorkflowDefinition::fromJson($this->compileToJson($workflowPath));
    }

    public function compileToJson(string $workflowPath): string
    {
        return $this->workflowExecutor->compileToJson($workflowPath);
    }
}
