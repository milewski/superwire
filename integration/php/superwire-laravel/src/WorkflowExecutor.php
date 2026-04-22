<?php

declare(strict_types = 1);

namespace Superwire\Laravel;

use Illuminate\Support\Facades\Process;
use RuntimeException;
use Superwire\Laravel\Exceptions\WorkflowExecutionException;

final readonly class WorkflowExecutor
{
    public function __construct(
        private string $cliPath,
    )
    {
    }

    public function compileToJson(string $workflowPath): string
    {
        $this->assertWorkflowFileExists($workflowPath);

        return $this->run(
            arguments: [ 'workflow', 'to-json', $workflowPath ],
            failureContext: sprintf('compile %s', $workflowPath),
        );
    }

    public function check(string $workflowPath): string
    {
        $this->assertWorkflowFileExists($workflowPath);

        return $this->run(
            arguments: [ 'workflow', 'check', $workflowPath ],
            failureContext: sprintf('check %s', $workflowPath),
        );
    }

    public function format(string $targetPath): string
    {
        if (!file_exists($targetPath)) {
            throw new RuntimeException(sprintf('Workflow target was not found at %s.', $targetPath));
        }

        return $this->run(
            arguments: [ 'fmt', $targetPath ],
            failureContext: sprintf('format %s', $targetPath),
        );
    }

    /**
     * @param list<string> $arguments
     */
    private function run(array $arguments, string $failureContext): string
    {
        $this->assertCliExists();

        $process = Process::path(base_path())
            ->timeout(30)
            ->run(array_merge([ $this->cliPath ], $arguments));

        if (!$process->successful()) {

            $message = trim($process->errorOutput() . PHP_EOL . $process->output());

            throw new WorkflowExecutionException(sprintf(
                'Superwire CLI failed to %s: %s',
                $failureContext,
                $message,
            ));

        }

        return $process->output();
    }

    private function assertCliExists(): void
    {
        if (!is_file($this->cliPath)) {
            throw new RuntimeException(sprintf('Superwire CLI was not found at %s.', $this->cliPath));
        }
    }

    private function assertWorkflowFileExists(string $workflowPath): void
    {
        if (!is_file($workflowPath)) {
            throw new RuntimeException(sprintf('Workflow file was not found at %s.', $workflowPath));
        }
    }
}
