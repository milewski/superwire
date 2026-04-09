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
        $inputJson = $this->encodePayloadAsJsonObject($workflowExecutionRequest->inputs, 'input payload');
        $inputPayloadFilePath = $this->createTemporaryPayloadFile($inputJson, 'superwire-input-');

        $command = [
            (string) $this->config->get('superwire.cli.binary', 'cli'),
            'workflow',
            'run',
            $workflowExecutionRequest->workflowFilePath,
            '--input-file',
            $inputPayloadFilePath,
        ];

        $secretsPayloadFilePath = null;

        if ($workflowExecutionRequest->secrets !== []) {
            $secretsJson = $this->encodePayloadAsJsonObject($workflowExecutionRequest->secrets, 'secrets payload');
            $secretsPayloadFilePath = $this->createTemporaryPayloadFile($secretsJson, 'superwire-secrets-');
            $command[] = '--secrets-file';
            $command[] = $secretsPayloadFilePath;
        }

        $process = new Process(
            $command,
            (string) $this->config->get('superwire.cli.working_directory', base_path()),
            [
                'SUPERWIRE_INTERNAL_TOKEN' => (string) $this->config->get('superwire.runtime.internal_token', ''),
                'SUPERWIRE_ERROR_FORMAT' => 'json',
            ],
            null,
            (float) $this->config->get('superwire.cli.timeout_seconds', 120),
        );

        try {
            $process->run();

            if (!$process->isSuccessful()) {
                throw $this->mapFailedProcessToException($command, $process);
            }

            $decodedOutput = json_decode($process->getOutput(), true);

            if (!is_array($decodedOutput)) {
                throw new WorkflowExecutionException('workflow output must be a JSON object');
            }

            return new WorkflowExecutionResult($decodedOutput);
        } finally {
            @unlink($inputPayloadFilePath);

            if ($secretsPayloadFilePath !== null) {
                @unlink($secretsPayloadFilePath);
            }
        }
    }

    /**
     * @param array<string, mixed> $payload
     */
    private function encodePayloadAsJsonObject(array $payload, string $payloadLabel): string
    {
        if ($payload === []) {
            return '{}';
        }

        if (array_is_list($payload)) {
            throw new WorkflowExecutionException(sprintf('%s must be an associative array', $payloadLabel));
        }

        return json_encode($payload, JSON_THROW_ON_ERROR);
    }

    private function createTemporaryPayloadFile(string $payloadJson, string $prefix): string
    {
        $temporaryPayloadFilePath = tempnam(sys_get_temp_dir(), $prefix);

        if ($temporaryPayloadFilePath === false) {
            throw new WorkflowExecutionException('failed to create temporary payload file');
        }

        @chmod($temporaryPayloadFilePath, 0600);

        if (file_put_contents($temporaryPayloadFilePath, $payloadJson) === false) {
            @unlink($temporaryPayloadFilePath);

            throw new WorkflowExecutionException('failed to write temporary payload file');
        }

        return $temporaryPayloadFilePath;
    }

    /**
     * @param array<int, string> $command
     */
    private function mapFailedProcessToException(array $command, Process $process): WorkflowExecutionException
    {
        $errorOutput = $process->getErrorOutput();
        $standardOutput = $process->getOutput();
        $cliOutput = trim($errorOutput) !== '' ? trim($errorOutput) : trim($standardOutput);
        $decodedPayload = json_decode($cliOutput, true);

        if (is_array($decodedPayload) && isset($decodedPayload['message']) && is_string($decodedPayload['message'])) {
            return new WorkflowExecutionException(
                message: sprintf(
                    "failed to execute workflow command `%s`: %s",
                    implode(' ', $command),
                    $decodedPayload['message'],
                ),
                command: $command,
                errorPayload: $decodedPayload,
                rawCliOutput: $cliOutput,
            );
        }

        return new WorkflowExecutionException(
            message: sprintf(
                "failed to execute workflow command `%s`: %s",
                implode(' ', $command),
                $cliOutput,
            ),
            command: $command,
            rawCliOutput: $cliOutput,
        );
    }
}
