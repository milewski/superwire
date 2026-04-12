<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests\Feature;

use Superwire\Laravel\Tests\TestCase;

final class CheckWorkflowCommandTest extends TestCase
{
    public function testChecksWorkflowSuccessfullyThroughConfiguredCli(): void
    {
        $temporaryDirectory = $this->createTemporaryDirectory('superwire-check-workflow-success');
        $fakeCliPath = $temporaryDirectory . DIRECTORY_SEPARATOR . 'fake-cli';
        $workflowFilePath = $temporaryDirectory . DIRECTORY_SEPARATOR . 'example.wire';

        file_put_contents($workflowFilePath, 'output { ok: boolean }');

        file_put_contents($fakeCliPath, <<<'PHP'
        #!/usr/bin/env php
        <?php

        $arguments = $_SERVER['argv'] ?? [];

        if (($arguments[1] ?? '') !== 'workflow' || ($arguments[2] ?? '') !== 'check') {
            fwrite(STDERR, 'unexpected command');
            exit(1);
        }

        $workflowFilePath = (string) ($arguments[3] ?? '');

        if ($workflowFilePath === '' || !is_file($workflowFilePath)) {
            fwrite(STDERR, 'missing workflow file path');
            exit(1);
        }

        exit(0);
        PHP,
        );

        chmod($fakeCliPath, 0o755);

        config()->set('superwire.cli.binary', $fakeCliPath);
        config()->set('superwire.cli.working_directory', $temporaryDirectory);

        $this->artisan('superwire:workflow:check', [
            'workflow' => $workflowFilePath,
        ])
            ->expectsOutput(sprintf('Workflow `%s` is valid.', $workflowFilePath))
            ->assertExitCode(0);
    }

    public function testFailsWhenCliWorkflowCheckReturnsError(): void
    {
        $temporaryDirectory = $this->createTemporaryDirectory('superwire-check-workflow-failure');
        $fakeCliPath = $temporaryDirectory . DIRECTORY_SEPARATOR . 'fake-cli';
        $workflowFilePath = $temporaryDirectory . DIRECTORY_SEPARATOR . 'example.wire';

        file_put_contents($workflowFilePath, 'output { ok: boolean }');

        file_put_contents($fakeCliPath, <<<'PHP'
        #!/usr/bin/env php
        <?php

        fwrite(STDERR, json_encode([
            'code' => 'invalid_input',
            'message' => 'workflow output block is missing',
            'details' => null,
        ], JSON_THROW_ON_ERROR));

        exit(1);
        PHP,
        );

        chmod($fakeCliPath, 0o755);

        config()->set('superwire.cli.binary', $fakeCliPath);
        config()->set('superwire.cli.working_directory', $temporaryDirectory);

        $expectedErrorMessage = sprintf(
            'failed to execute workflow command `%s`: %s',
            implode(' ', [ $fakeCliPath, 'workflow', 'check', $workflowFilePath ]),
            'workflow output block is missing',
        );

        $this->artisan('superwire:workflow:check', [
            'workflow' => $workflowFilePath,
        ])
            ->expectsOutput($expectedErrorMessage)
            ->assertExitCode(1);
    }
}
