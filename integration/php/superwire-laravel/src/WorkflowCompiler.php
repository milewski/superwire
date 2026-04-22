<?php

declare(strict_types = 1);

namespace Superwire\Laravel;

use Illuminate\Support\Facades\File;
use Illuminate\Support\Str;
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

    public function compileSource(string $workflowSource, ?string $workflowPath = null): WorkflowDefinition
    {
        return WorkflowDefinition::fromJson($this->compileSourceToJson($workflowSource, $workflowPath));
    }

    public function compileSourceToJson(string $workflowSource, ?string $workflowPath = null): string
    {
        $tempFilePath = $this->createTempWorkflowFile($workflowSource, $workflowPath);

        try {
            return $this->compileToJson($tempFilePath);
        } finally {
            File::delete($tempFilePath);
        }
    }

    private function createTempWorkflowFile(string $workflowSource, ?string $workflowPath = null): string
    {
        $fileName = $workflowPath !== null && str_ends_with($workflowPath, '.wire')
            ? basename($workflowPath)
            : 'workflow-' . Str::uuid()->toString() . '.wire';

        $tempDirectory = storage_path('framework/cache/superwire-inline-workflows');

        File::ensureDirectoryExists($tempDirectory);

        $tempFilePath = $tempDirectory . DIRECTORY_SEPARATOR . $fileName;

        File::put($tempFilePath, $workflowSource);

        return $tempFilePath;
    }
}
