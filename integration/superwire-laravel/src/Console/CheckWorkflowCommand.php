<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Console;

use Illuminate\Console\Command;
use Superwire\Laravel\Exceptions\WorkflowExecutionException;
use Superwire\Laravel\Execution\WorkflowExecutor;

final class CheckWorkflowCommand extends Command
{
    protected $signature = 'superwire:workflow:check
        {workflow : Path to workflow file}';

    protected $description = 'Check whether a Superwire workflow is valid and runtime-compilable';

    public function handle(WorkflowExecutor $workflowExecutor): int
    {
        $workflowFilePath = (string) $this->argument('workflow');

        try {

            $workflowExecutor->check($workflowFilePath);

        } catch (WorkflowExecutionException $workflowExecutionException) {

            $this->error($workflowExecutionException->getMessage());

            return self::FAILURE;

        }

        $this->info(sprintf('Workflow `%s` is valid.', $workflowFilePath));

        return self::SUCCESS;
    }
}
