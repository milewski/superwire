<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Execution;

use Illuminate\Contracts\Config\Repository;
use Illuminate\Support\Facades\Concurrency;
use JsonException;
use RuntimeException;
use Superwire\Laravel\Data\ToolBuildRequest;
use Superwire\Laravel\Data\ToolBuildResult;
use Superwire\Laravel\Exceptions\ToolBuildException;
use Superwire\Laravel\Execution\Compiler\ToolClassValidator;
use Superwire\Laravel\Execution\Compiler\ToolEndpointResolver;
use Superwire\Laravel\Execution\Compiler\ToolModuleSourceGenerator;
use Superwire\Laravel\Execution\Compiler\ToolModuleTemplateRenderer;
use Superwire\Laravel\Execution\Compiler\ToolNameFormatter;
use Superwire\Laravel\Execution\Compiler\ToolSchemaPayloadSerializer;
use Symfony\Component\Process\Process;

final readonly class ToolCompiler
{
    public function __construct(private Repository $config)
    {
    }

    /**
     * @throws JsonException
     */
    public function build(ToolBuildRequest $toolBuildRequest): ToolBuildResult
    {
        $toolClassValidator = new ToolClassValidator();
        $toolNameFormatter = new ToolNameFormatter();
        $toolSchemaPayloadSerializer = new ToolSchemaPayloadSerializer();
        $toolEndpointResolver = new ToolEndpointResolver($this->config);
        $toolModuleTemplateRenderer = new ToolModuleTemplateRenderer(__DIR__ . '/../../resources/templates/tool_module.rs.tpl');
        $toolModuleSourceGenerator = new ToolModuleSourceGenerator(
            $toolNameFormatter,
            $toolEndpointResolver,
            $toolSchemaPayloadSerializer,
            $toolModuleTemplateRenderer,
        );

        $validatedToolClasses = $toolClassValidator->validate($toolBuildRequest->toolClasses);
        $buildRootDirectory = (string) $this->config->get('superwire.build.root_directory', storage_path('app/superwire'));
        $toolOutputDirectory = (string) $this->config->get('superwire.build.tools_directory', base_path('tools'));
        $toolSourcesDirectory = $buildRootDirectory . '/tool-sources/src';
        $toolSourcesManifestPath = $buildRootDirectory . '/tool-sources/Cargo.toml';
        $toolSourcesLibPath = $toolSourcesDirectory . '/lib.rs';

        if (!is_dir($toolSourcesDirectory) && !mkdir($toolSourcesDirectory, 0o777, true) && !is_dir($toolSourcesDirectory)) {
            throw new ToolBuildException(sprintf('failed to create tool sources directory %s', $toolSourcesDirectory));
        }

        if (!is_dir($toolOutputDirectory) && !mkdir($toolOutputDirectory, 0o777, true) && !is_dir($toolOutputDirectory)) {
            throw new ToolBuildException(sprintf('failed to create tool output directory %s', $toolOutputDirectory));
        }

        file_put_contents($toolSourcesManifestPath, $this->toolSourcesCargoManifest());

        $moduleNames = [];
        $toolRegistryMap = [];
        $toolSourcePathByToolName = [];

        foreach ($validatedToolClasses as $toolClass) {

            $toolName = $toolClass::name();
            $moduleName = $toolNameFormatter->moduleName($toolName);
            $moduleNames[] = $moduleName;
            $toolRegistryMap[ $toolName ] = [
                'class' => $toolClass,
                'description' => $toolClass::description(),
                'endpoint_name' => $toolClass::endpointName(),
                'input_schema' => $toolSchemaPayloadSerializer->payload($toolClass::inputSchema()),
                'bound_input_schema' => $toolSchemaPayloadSerializer->payload($toolClass::boundInputSchema()),
                'output_schema' => $toolSchemaPayloadSerializer->payload($toolClass::outputSchema()),
            ];

            $modulePath = sprintf('%s/%s.rs', $toolSourcesDirectory, $moduleName);
            file_put_contents($modulePath, $toolModuleSourceGenerator->generate($toolClass));
            $toolSourcePathByToolName[ $toolName ] = $modulePath;

        }

        $libSourceLines = array_map(
            static fn (string $moduleName): string => sprintf('pub mod %s;', $moduleName),
            $moduleNames,
        );

        file_put_contents($toolSourcesLibPath, implode(PHP_EOL, $libSourceLines) . PHP_EOL);

        $registryManifestPath = $buildRootDirectory . '/tool-registry.json';
        file_put_contents($registryManifestPath, json_encode([ 'tools' => $toolRegistryMap ], JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR));

        $this->buildToolModulesInParallel($toolSourcePathByToolName, $toolOutputDirectory);

        return new ToolBuildResult(array_keys($toolRegistryMap), $toolOutputDirectory);
    }

    /**
     * @param array<string, string> $toolSourcePathByToolName
     */
    private function buildToolModulesInParallel(array $toolSourcePathByToolName, string $toolOutputDirectory): void
    {
        $cliBinary = (string) $this->config->get('superwire.cli.binary', 'cli');
        $workingDirectory = (string) $this->config->get('superwire.cli.working_directory', base_path());
        $timeoutSeconds = (float) $this->config->get('superwire.cli.timeout_seconds', 120);
        $buildTasks = [];

        foreach ($toolSourcePathByToolName as $toolName => $toolSourcePath) {

            $toolOutputPath = $toolOutputDirectory . DIRECTORY_SEPARATOR . $toolName . '.wasm';
            $command = [
                $cliBinary,
                'tools',
                'build',
                $toolSourcePath,
                '--output',
                $toolOutputPath,
            ];

            $buildTasks[] = static function () use ($command, $workingDirectory, $timeoutSeconds, $toolName): array {

                $process = new Process(
                    command: $command,
                    cwd: $workingDirectory,
                    env: null,
                    input: null,
                    timeout: $timeoutSeconds,
                );

                $process->run();

                return [
                    'tool_name' => $toolName,
                    'command' => $command,
                    'success' => $process->isSuccessful(),
                    'error_output' => trim($process->getErrorOutput()),
                    'standard_output' => trim($process->getOutput()),
                ];

            };

        }

        try {

            $buildResults = Concurrency::driver('fork')->run($buildTasks);

        } catch (RuntimeException) {

            $buildResults = array_map(static fn (callable $buildTask): array => $buildTask(), $buildTasks);

        }

        foreach ($buildResults as $buildResult) {

            if (($buildResult[ 'success' ] ?? false) === true) {
                continue;
            }

            $toolName = is_string($buildResult[ 'tool_name' ] ?? null) ? $buildResult[ 'tool_name' ] : 'unknown';
            $command = is_array($buildResult[ 'command' ] ?? null) ? $buildResult[ 'command' ] : [];
            $errorOutput = is_string($buildResult[ 'error_output' ] ?? null) ? $buildResult[ 'error_output' ] : '';
            $standardOutput = is_string($buildResult[ 'standard_output' ] ?? null) ? $buildResult[ 'standard_output' ] : '';
            $failureOutput = $errorOutput !== '' ? $errorOutput : $standardOutput;

            throw new ToolBuildException(sprintf(
                'failed to build tool `%s` using `%s`: %s',
                $toolName,
                implode(' ', $command),
                $failureOutput,
            ));

        }
    }

    private function toolSourcesCargoManifest(): string
    {
        return <<<'TOML'
        [package]
        name = "superwire_php_tools"
        version = "0.1.0"
        edition = "2021"

        [lib]
        path = "src/lib.rs"
        TOML;
    }
}
