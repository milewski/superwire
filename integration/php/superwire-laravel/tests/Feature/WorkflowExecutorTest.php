<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests\Feature;

use Illuminate\Support\Facades\File;
use Superwire\Laravel\Exceptions\WorkflowExecutionException;
use Superwire\Laravel\Tests\TestCase;
use Superwire\Laravel\WorkflowExecutor;

final class WorkflowExecutorTest extends TestCase
{
    public function test_formats_workflow_file_using_cli_path(): void
    {
        $scriptPath = storage_path('framework/testing/fake-superwire-cli');
        $workflowPath = storage_path('framework/testing/format-me.wire');

        File::ensureDirectoryExists(dirname($scriptPath));
        File::put($scriptPath, <<<'BASH'
        #!/usr/bin/env sh
        if [ "$1" = "fmt" ]; then
          printf 'formatted workflow\n' > "$2"
          exit 0
        fi
        exit 1
        BASH);
        chmod($scriptPath, 0o755);
        File::put($workflowPath, "original workflow\n");

        config()->set('superwire.cli.path', $scriptPath);

        app(WorkflowExecutor::class)->format($workflowPath);

        $this->assertSame("formatted workflow\n", File::get($workflowPath));

        File::delete([ $scriptPath, $workflowPath ]);
    }

    public function test_checks_workflow_file_using_cli_path(): void
    {
        $scriptPath = storage_path('framework/testing/fake-superwire-cli');
        $workflowPath = storage_path('framework/testing/check-me.wire');

        File::ensureDirectoryExists(dirname($scriptPath));
        File::put($scriptPath, <<<'BASH'
        #!/usr/bin/env sh
        if [ "$1" = "workflow" ] && [ "$2" = "check" ]; then
          printf 'workflow is valid\n'
          exit 0
        fi
        exit 1
        BASH);
        chmod($scriptPath, 0o755);
        File::put($workflowPath, "output { ok: true }\n");

        config()->set('superwire.cli.path', $scriptPath);

        $this->assertSame("workflow is valid\n", app(WorkflowExecutor::class)->check($workflowPath));

        File::delete([ $scriptPath, $workflowPath ]);
    }

    public function test_throws_workflow_execution_exception_when_cli_command_fails(): void
    {
        $scriptPath = storage_path('framework/testing/fake-superwire-cli');
        $workflowPath = storage_path('framework/testing/invalid.wire');

        File::ensureDirectoryExists(dirname($scriptPath));
        File::put($scriptPath, <<<'BASH'
        #!/usr/bin/env sh
        printf 'invalid workflow\n' >&2
        exit 2
        BASH);
        chmod($scriptPath, 0o755);
        File::put($workflowPath, "bad workflow\n");

        config()->set('superwire.cli.path', $scriptPath);

        $this->expectException(WorkflowExecutionException::class);
        $this->expectExceptionMessage('invalid workflow');

        try {
            app(WorkflowExecutor::class)->check($workflowPath);
        } finally {
            File::delete([ $scriptPath, $workflowPath ]);
        }
    }
}
