<?php

declare(strict_types = 1);

namespace Superwire\Laravel;

use RuntimeException;
use Superwire\Laravel\Data\Workflow\WorkflowDefinition;

final readonly class WorkflowCompiler
{
    public function __construct(
        private string $cliPath,
    )
    {
    }

    public function compile(string $workflowPath): WorkflowDefinition
    {
        return WorkflowDefinition::fromJson($this->compileToJson($workflowPath));
    }

    public function compileToJson(string $workflowPath): string
    {
        if (!is_file($this->cliPath)) {
            throw new RuntimeException(sprintf('Superwire CLI was not found at %s.', $this->cliPath));
        }

        if (!is_file($workflowPath)) {
            throw new RuntimeException(sprintf('Workflow file was not found at %s.', $workflowPath));
        }

        $command = sprintf(
            '%s workflow to-json %s 2>&1',
            escapeshellarg($this->cliPath),
            escapeshellarg($workflowPath),
        );

        exec($command, $outputLines, $exitCode);

        $output = implode("\n", $outputLines);

        if ($exitCode !== 0) {
            throw new RuntimeException(sprintf('Superwire CLI failed to compile %s: %s', $workflowPath, $output));
        }

        return $output;
    }
}
