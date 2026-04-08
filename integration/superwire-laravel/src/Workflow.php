<?php

namespace Superwire\Laravel;

use Superwire\Laravel\Support\PendingWorkflow;

final class Workflow
{
    public static function fromFile(string $workflowFilePath): PendingWorkflow
    {
        return Support\Workflow::fromFile($workflowFilePath);
    }
}
