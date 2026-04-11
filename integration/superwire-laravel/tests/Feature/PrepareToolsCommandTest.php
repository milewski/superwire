<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests\Feature;

use Superwire\Laravel\Contracts\Tool;
use Superwire\Laravel\Contracts\ToolBoundInputData;
use Superwire\Laravel\Contracts\ToolInputData;
use Superwire\Laravel\Tests\TestCase;
use Swaggest\JsonSchema\Schema;

final class PrepareToolsCommandTest extends TestCase
{
    public function testCompilesAndPublishesToolArtifactsForWorkflow(): void
    {
        $temporaryDirectory = $this->createTemporaryDirectory('superwire-prepare-tools');
        $fakeCliPath = $temporaryDirectory . DIRECTORY_SEPARATOR . 'fake-cli';
        $workflowFilePath = $temporaryDirectory . DIRECTORY_SEPARATOR . 'example.wire';
        $buildRootDirectory = $temporaryDirectory . DIRECTORY_SEPARATOR . 'build-root';
        $buildOutputDirectory = $temporaryDirectory . DIRECTORY_SEPARATOR . 'build-tools-output';

        file_put_contents($workflowFilePath, <<<'WIRE'
        agent assistant {
            model: openai("model-a")
            tools: [tool.echo_tool]
            prompt: "hello"
            output: string
        }

        output {
            value: agent.assistant
        }
        WIRE,
        );

        file_put_contents($fakeCliPath, <<<'PHP'
        #!/usr/bin/env php
        <?php

        $arguments = $_SERVER['argv'] ?? [];

        if (($arguments[1] ?? '') !== 'tools' || ($arguments[2] ?? '') !== 'build') {
            fwrite(STDERR, 'unexpected command');
            exit(1);
        }

        $outputDirectory = null;

        for ($argumentIndex = 3; $argumentIndex < count($arguments); $argumentIndex++) {
            if (($arguments[$argumentIndex] ?? '') !== '--output') {
                continue;
            }

            $outputDirectory = (string) ($arguments[$argumentIndex + 1] ?? '');
            break;
        }

        if ($outputDirectory === null || $outputDirectory === '') {
            fwrite(STDERR, 'missing output directory');
            exit(1);
        }

        if (!is_dir($outputDirectory) && !mkdir($outputDirectory, 0777, true) && !is_dir($outputDirectory)) {
            fwrite(STDERR, 'failed to create output directory');
            exit(1);
        }

        file_put_contents($outputDirectory . DIRECTORY_SEPARATOR . 'echo_tool.wasm', 'fake-wasm');
        exit(0);
        PHP,
        );

        chmod($fakeCliPath, 0o755);

        config()->set('superwire.cli.binary', $fakeCliPath);
        config()->set('superwire.cli.working_directory', $temporaryDirectory);
        config()->set('superwire.build.root_directory', $buildRootDirectory);
        config()->set('superwire.build.tools_directory', $buildOutputDirectory);
        config()->set('superwire.tools.registered_classes', [ PrepareToolsCommandTestTool::class ]);

        $this->artisan('superwire:tools:prepare', [
            '--workflow' => [ $workflowFilePath ],
        ])->assertExitCode(0);

        $publishedArtifactPath = dirname($workflowFilePath) . DIRECTORY_SEPARATOR . 'tools' . DIRECTORY_SEPARATOR . 'echo_tool.wasm';

        $this->assertFileExists($publishedArtifactPath);
    }

    public function testDiscoversToolClassesFromProjectWhenNotConfigured(): void
    {
        $temporaryDirectory = $this->createTemporaryDirectory('superwire-prepare-tools-discovery');
        $fakeCliPath = $temporaryDirectory . DIRECTORY_SEPARATOR . 'fake-cli';
        $workflowFilePath = $temporaryDirectory . DIRECTORY_SEPARATOR . 'project_summarizer.wire';
        $toolClassFilePath = $temporaryDirectory . DIRECTORY_SEPARATOR . 'app' . DIRECTORY_SEPARATOR . 'Superwire' . DIRECTORY_SEPARATOR . 'Tools' . DIRECTORY_SEPARATOR . 'DiscoveredTool.php';
        $buildRootDirectory = $temporaryDirectory . DIRECTORY_SEPARATOR . 'build-root';
        $buildOutputDirectory = $temporaryDirectory . DIRECTORY_SEPARATOR . 'build-tools-output';

        mkdir(dirname($toolClassFilePath), 0o777, true);

        file_put_contents($temporaryDirectory . DIRECTORY_SEPARATOR . '.phpstorm.meta.php', <<<'PHP'
        <?php

        namespace PHPSTORM_META;

        override(\App\container(0), map([
            '' => '@',
        ]));
        PHP,
        );

        file_put_contents($toolClassFilePath, <<<'PHP'
        <?php

        declare(strict_types = 1);

        namespace App\Superwire\Tools;

        use Superwire\Laravel\Contracts\Tool;
        use Superwire\Laravel\Contracts\ToolBoundInputData;
        use Superwire\Laravel\Contracts\ToolInputData;
        use Swaggest\JsonSchema\Schema;

        final class DiscoveredTool implements Tool
        {
            public static function name(): string
            {
                return 'discovered_tool';
            }

            public static function description(): string
            {
                return 'Discovered tool';
            }

            public static function endpointName(): string
            {
                return 'discovered_tool';
            }

            public static function agentInputClass(): string
            {
                return DiscoveredToolAgentInput::class;
            }

            public static function boundInputClass(): string
            {
                return DiscoveredToolBoundInput::class;
            }

            public static function outputClass(): string
            {
                return DiscoveredToolOutput::class;
            }

            public static function inputSchema(): Schema
            {
                return Schema::object();
            }

            public static function boundInputSchema(): Schema
            {
                return Schema::object();
            }

            public static function outputSchema(): Schema
            {
                return Schema::object();
            }

            public static function resolveAgentInput(array $agentInputPayload): ToolInputData
            {
                return new DiscoveredToolAgentInput();
            }

            public static function resolveBoundInput(array $boundInputPayload): ToolBoundInputData
            {
                return new DiscoveredToolBoundInput();
            }

            public function execute(ToolInputData $agentInput, ToolBoundInputData $boundInput): array
            {
                return [];
            }
        }

        final class DiscoveredToolAgentInput implements ToolInputData
        {
        }

        final class DiscoveredToolBoundInput implements ToolBoundInputData
        {
        }

        final class DiscoveredToolOutput
        {
        }
        PHP,
        );

        file_put_contents($workflowFilePath, <<<'WIRE'
        agent assistant {
            model: openai("model-a")
            tools: [tool.discovered_tool]
            prompt: "hello"
            output: string
        }

        output {
            value: agent.assistant
        }
        WIRE,
        );

        file_put_contents($fakeCliPath, <<<'PHP'
        #!/usr/bin/env php
        <?php

        $arguments = $_SERVER['argv'] ?? [];

        if (($arguments[1] ?? '') !== 'tools' || ($arguments[2] ?? '') !== 'build') {
            fwrite(STDERR, 'unexpected command');
            exit(1);
        }

        $outputDirectory = null;

        for ($argumentIndex = 3; $argumentIndex < count($arguments); $argumentIndex++) {
            if (($arguments[$argumentIndex] ?? '') !== '--output') {
                continue;
            }

            $outputDirectory = (string) ($arguments[$argumentIndex + 1] ?? '');
            break;
        }

        if ($outputDirectory === null || $outputDirectory === '') {
            fwrite(STDERR, 'missing output directory');
            exit(1);
        }

        if (!is_dir($outputDirectory) && !mkdir($outputDirectory, 0777, true) && !is_dir($outputDirectory)) {
            fwrite(STDERR, 'failed to create output directory');
            exit(1);
        }

        file_put_contents($outputDirectory . DIRECTORY_SEPARATOR . 'discovered_tool.wasm', 'fake-wasm');
        exit(0);
        PHP,
        );

        chmod($fakeCliPath, 0o755);

        config()->set('superwire.cli.binary', $fakeCliPath);
        config()->set('superwire.cli.working_directory', $temporaryDirectory);
        config()->set('superwire.build.root_directory', $buildRootDirectory);
        config()->set('superwire.build.tools_directory', $buildOutputDirectory);
        config()->set('superwire.tools.registered_classes', []);

        $this->artisan('superwire:tools:prepare', [
            'scan-path' => $temporaryDirectory,
            '--workflow' => [ $workflowFilePath ],
        ])->assertExitCode(0);

        $publishedArtifactPath = dirname($workflowFilePath) . DIRECTORY_SEPARATOR . 'tools' . DIRECTORY_SEPARATOR . 'discovered_tool.wasm';

        $this->assertFileExists($publishedArtifactPath);
    }
}

final class PrepareToolsCommandTestTool implements Tool
{
    public static function name(): string
    {
        return 'echo_tool';
    }

    public static function description(): string
    {
        return 'Test tool for command preparation';
    }

    public static function endpointName(): string
    {
        return 'echo_tool';
    }

    public static function agentInputClass(): string
    {
        return PrepareToolsCommandTestToolInput::class;
    }

    public static function boundInputClass(): string
    {
        return PrepareToolsCommandTestToolBoundInput::class;
    }

    public static function outputClass(): string
    {
        return PrepareToolsCommandTestToolOutput::class;
    }

    public static function inputSchema(): Schema
    {
        return Schema::object();
    }

    public static function boundInputSchema(): Schema
    {
        return Schema::object();
    }

    public static function outputSchema(): Schema
    {
        return Schema::object();
    }

    public static function resolveAgentInput(array $agentInputPayload): ToolInputData
    {
        return new PrepareToolsCommandTestToolInput();
    }

    public static function resolveBoundInput(array $boundInputPayload): ToolBoundInputData
    {
        return new PrepareToolsCommandTestToolBoundInput();
    }

    public function execute(ToolInputData $agentInput, ToolBoundInputData $boundInput): array
    {
        return [];
    }
}

final class PrepareToolsCommandTestToolInput implements ToolInputData
{
}

final class PrepareToolsCommandTestToolBoundInput implements ToolBoundInputData
{
}

final class PrepareToolsCommandTestToolOutput
{
}
