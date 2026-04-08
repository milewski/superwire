<?php

namespace Superwire\Laravel\Execution;

use Illuminate\Contracts\Config\Repository;
use Superwire\Laravel\Data\WorkflowExecutionRequest;
use Superwire\Laravel\Data\WorkflowExecutionResult;
use Superwire\Laravel\Exceptions\WorkflowExecutionException;
use Symfony\Component\Process\Process;

final class WorkflowExecutor
{
    public function __construct(private readonly Repository $config)
    {
    }

    public function execute(WorkflowExecutionRequest $workflowExecutionRequest): WorkflowExecutionResult
    {
        $inputJson = json_encode($workflowExecutionRequest->inputs, JSON_THROW_ON_ERROR);
        $command = [
            (string) $this->config->get('superwire.cli.binary', 'cli'),
            'workflow',
            'run',
            $workflowExecutionRequest->workflowFilePath,
            '--input-json',
            $inputJson,
        ];

        if ($workflowExecutionRequest->secrets !== []) {
            $secretsJson = json_encode($workflowExecutionRequest->secrets, JSON_THROW_ON_ERROR);
            $command[] = '--secrets-json';
            $command[] = $secretsJson;
        }

        $process = new Process(
            $command,
            (string) $this->config->get('superwire.cli.working_directory', base_path()),
            [
                'SUPERWIRE_INTERNAL_TOKEN' => (string) $this->config->get('superwire.runtime.internal_token', ''),
            ],
            null,
            (float) $this->config->get('superwire.cli.timeout_seconds', 120),
        );

        $process->run();

        if (!$process->isSuccessful()) {
            throw new WorkflowExecutionException(sprintf(
                "failed to execute workflow command `%s`: %s",
                implode(' ', $command),
                trim($process->getErrorOutput()) !== '' ? trim($process->getErrorOutput()) : trim($process->getOutput()),
            ));
        }

        $decodedOutput = json_decode($process->getOutput(), true);

        if (!is_array($decodedOutput)) {
            throw new WorkflowExecutionException('workflow output must be a JSON object');
        }

        return new WorkflowExecutionResult($decodedOutput);
    }
}
