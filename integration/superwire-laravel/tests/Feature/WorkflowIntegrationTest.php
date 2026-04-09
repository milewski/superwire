<?php

namespace Superwire\Laravel\Tests\Feature;

use Superwire\Laravel\Exceptions\WorkflowExecutionException;
use Superwire\Laravel\Tests\TestCase;
use Superwire\Laravel\Workflow;

final class WorkflowIntegrationTest extends TestCase
{
    public function testRunsWorkflowWithInputsAndSecretsThroughCliExecutor(): void
    {
        $temporaryDirectory = $this->createTemporaryDirectory('superwire-workflow');
        $fakeCliPath = $temporaryDirectory . DIRECTORY_SEPARATOR . 'fake-cli';
        $workflowFilePath = $temporaryDirectory . DIRECTORY_SEPARATOR . 'example.wire';

        file_put_contents($workflowFilePath, "output { ok: boolean }");

        file_put_contents($fakeCliPath, <<<'PHP'
#!/usr/bin/env php
<?php

$arguments = $_SERVER['argv'] ?? [];

if (($arguments[1] ?? '') !== 'workflow' || ($arguments[2] ?? '') !== 'run') {
    fwrite(STDERR, 'unexpected command');
    exit(1);
}

$workflowFilePath = (string) ($arguments[3] ?? '');
$inputJson = '{}';
$secretsJson = '{}';

for ($argumentIndex = 4; $argumentIndex < count($arguments); $argumentIndex++) {
    $argumentName = $arguments[$argumentIndex];

    if ($argumentName === '--input-file') {
        $inputFilePath = (string) ($arguments[$argumentIndex + 1] ?? '');
        $inputJson = is_file($inputFilePath) ? (string) file_get_contents($inputFilePath) : '{}';
        $argumentIndex++;
        continue;
    }

    if ($argumentName === '--secrets-file') {
        $secretsFilePath = (string) ($arguments[$argumentIndex + 1] ?? '');
        $secretsJson = is_file($secretsFilePath) ? (string) file_get_contents($secretsFilePath) : '{}';
        $argumentIndex++;
        continue;
    }
}

$inputPayload = json_decode($inputJson, true);
$secretsPayload = json_decode($secretsJson, true);

if (!is_array($inputPayload) || !is_array($secretsPayload)) {
    fwrite(STDERR, 'invalid json payload');
    exit(1);
}

echo json_encode([
    'workflow_file_path' => $workflowFilePath,
    'inputs' => $inputPayload,
    'secrets' => $secretsPayload,
    'internal_token' => getenv('SUPERWIRE_INTERNAL_TOKEN') ?: '',
], JSON_THROW_ON_ERROR);
PHP,
        );

        chmod($fakeCliPath, 0755);

        config()->set('superwire.cli.binary', $fakeCliPath);
        config()->set('superwire.cli.working_directory', $temporaryDirectory);

        $workflowOutput = Workflow::fromFile($workflowFilePath)
            ->withInputs([ 'city' => 'Lisbon' ])
            ->withSecrets([ 'api_key' => 'secret-test-key' ])
            ->run();

        $this->assertSame($workflowFilePath, $workflowOutput[ 'workflow_file_path' ]);
        $this->assertSame([ 'city' => 'Lisbon' ], $workflowOutput[ 'inputs' ]);
        $this->assertSame([ 'api_key' => 'secret-test-key' ], $workflowOutput[ 'secrets' ]);
        $this->assertSame('test-internal-token', $workflowOutput[ 'internal_token' ]);
    }

    public function testSendsEmptyInputsAsJsonObjectWhenNotProvided(): void
    {
        $temporaryDirectory = $this->createTemporaryDirectory('superwire-workflow-empty-inputs');
        $fakeCliPath = $temporaryDirectory . DIRECTORY_SEPARATOR . 'fake-cli';
        $workflowFilePath = $temporaryDirectory . DIRECTORY_SEPARATOR . 'example.wire';

        file_put_contents($workflowFilePath, "output { ok: boolean }");

        file_put_contents($fakeCliPath, <<<'PHP'
#!/usr/bin/env php
<?php

$arguments = $_SERVER['argv'] ?? [];

if (($arguments[1] ?? '') !== 'workflow' || ($arguments[2] ?? '') !== 'run') {
    fwrite(STDERR, 'unexpected command');
    exit(1);
}

$inputFilePath = null;

for ($argumentIndex = 4; $argumentIndex < count($arguments); $argumentIndex++) {
    $argumentName = $arguments[$argumentIndex];

    if ($argumentName === '--input-file') {
        $inputFilePath = (string) ($arguments[$argumentIndex + 1] ?? '');
        break;
    }
}

if ($inputFilePath === null || !is_file($inputFilePath)) {
    fwrite(STDERR, 'missing --input-file argument');
    exit(1);
}

$inputJson = (string) file_get_contents($inputFilePath);

echo json_encode([
    'input_json' => $inputJson,
], JSON_THROW_ON_ERROR);
PHP,
        );

        chmod($fakeCliPath, 0755);

        config()->set('superwire.cli.binary', $fakeCliPath);
        config()->set('superwire.cli.working_directory', $temporaryDirectory);

        $workflowOutput = Workflow::fromFile($workflowFilePath)->run();

        $this->assertSame('{}', $workflowOutput[ 'input_json' ]);
    }

    public function testMapsJsonCliErrorPayloadIntoWorkflowExecutionException(): void
    {
        $temporaryDirectory = $this->createTemporaryDirectory('superwire-workflow-json-error');
        $fakeCliPath = $temporaryDirectory . DIRECTORY_SEPARATOR . 'fake-cli';
        $workflowFilePath = $temporaryDirectory . DIRECTORY_SEPARATOR . 'example.wire';

        file_put_contents($workflowFilePath, "output { ok: boolean }");

        file_put_contents($fakeCliPath, <<<'PHP'
#!/usr/bin/env php
<?php

fwrite(STDERR, json_encode([
    'code' => 'internal_error',
    'message' => 'agent execution failed for `summarizer`: Agent failed to complete the task: test reason',
    'details' => [
        'type' => 'workflow_runtime_error',
        'kind' => 'agent_execution_failed',
        'agent_name' => 'summarizer',
        'context' => [
            'messages' => [
                [
                    'kind' => 'user',
                    'content' => 'hello',
                ],
            ],
            'total_tokens' => 12,
            'input_tokens' => 10,
            'output_tokens' => 2,
        ],
    ],
], JSON_THROW_ON_ERROR));

exit(1);
PHP,
        );

        chmod($fakeCliPath, 0755);

        config()->set('superwire.cli.binary', $fakeCliPath);
        config()->set('superwire.cli.working_directory', $temporaryDirectory);

        try {
            Workflow::fromFile($workflowFilePath)->run();
            $this->fail('Workflow execution should fail with WorkflowExecutionException.');
        } catch (WorkflowExecutionException $workflowExecutionException) {
            $errorPayload = $workflowExecutionException->errorPayload();

            $this->assertIsArray($errorPayload);
            $this->assertSame('internal_error', $errorPayload[ 'code' ]);
            $this->assertSame('summarizer', $errorPayload[ 'details' ][ 'agent_name' ]);

            $context = $workflowExecutionException->context();

            $this->assertIsArray($context);
            $this->assertSame(12, $context[ 'total_tokens' ]);
            $this->assertSame('hello', $context[ 'messages' ][0][ 'content' ]);
        }
    }
}
