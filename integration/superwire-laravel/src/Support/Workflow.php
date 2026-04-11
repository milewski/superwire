<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Support;

use Superwire\Laravel\Execution\WorkflowExecutor;

final class Workflow
{
    public static function fromFile(string $workflowFilePath): PendingWorkflow
    {
        $workflowExecutor = app(WorkflowExecutor::class);
        $outputMapper = app(OutputMapper::class);

        return new PendingWorkflow($workflowFilePath, $workflowExecutor, $outputMapper);
    }
}
