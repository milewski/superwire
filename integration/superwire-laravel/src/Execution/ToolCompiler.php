<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Execution;

use Illuminate\Contracts\Config\Repository;
use Illuminate\Support\Facades\Concurrency;
use Illuminate\Support\Facades\Process;
use JsonException;
use RuntimeException;
use Superwire\Laravel\Contracts\WitDefinedTool;
use Superwire\Laravel\Data\ToolBuildRequest;
use Superwire\Laravel\Data\ToolBuildResult;
use Superwire\Laravel\Exceptions\ToolBuildException;
use Superwire\Laravel\Execution\Compiler\ToolClassValidator;
use Superwire\Laravel\Execution\Compiler\ToolEndpointResolver;
use Superwire\Laravel\Execution\Compiler\ToolModuleSourceGenerator;
use Superwire\Laravel\Execution\Compiler\ToolModuleTemplateRenderer;
use Superwire\Laravel\Execution\Compiler\ToolNameFormatter;
use Superwire\Laravel\Execution\Compiler\ToolSchemaPayloadSerializer;
use Superwire\Laravel\Wit\WitPhpToolTypesGenerator;
use Superwire\Laravel\Wit\WitToolSchemaParser;

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
        $witToolSchemaParser = new WitToolSchemaParser();
        $witPhpToolTypesGenerator = new WitPhpToolTypesGenerator();
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
        $toolSourcesWitDirectory = $buildRootDirectory . '/tool-sources/wit';
        $toolSourcesManifestPath = $buildRootDirectory . '/tool-sources/Cargo.toml';
        $toolSourcesLibPath = $toolSourcesDirectory . '/lib.rs';

        if (!is_dir($toolSourcesDirectory) && !mkdir($toolSourcesDirectory, 0o777, true) && !is_dir($toolSourcesDirectory)) {
            throw new ToolBuildException(sprintf('failed to create tool sources directory %s', $toolSourcesDirectory));
        }

        if (!is_dir($toolSourcesWitDirectory) && !mkdir($toolSourcesWitDirectory, 0o777, true) && !is_dir($toolSourcesWitDirectory)) {
            throw new ToolBuildException(sprintf('failed to create tool wit directory %s', $toolSourcesWitDirectory));
        }

        if (!is_dir($toolOutputDirectory) && !mkdir($toolOutputDirectory, 0o777, true) && !is_dir($toolOutputDirectory)) {
            throw new ToolBuildException(sprintf('failed to create tool output directory %s', $toolOutputDirectory));
        }

        file_put_contents($toolSourcesManifestPath, $this->toolSourcesCargoManifest());

        $moduleNames = [];
        $toolRegistryMap = [];
        $toolSourcePathByToolName = [];

        foreach ($validatedToolClasses as $toolClass) {

            if (is_subclass_of($toolClass, WitDefinedTool::class)) {

                /** @var class-string<WitDefinedTool> $witDefinedToolClass */
                $witDefinedToolClass = $toolClass;
                $parsedWitSchema = $witToolSchemaParser->parseFile($witDefinedToolClass::witPath());
                $witPhpToolTypesGenerator->generateForToolClass($witDefinedToolClass, $parsedWitSchema);

            }

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

            if (is_subclass_of($toolClass, WitDefinedTool::class)) {

                /** @var class-string<WitDefinedTool> $witDefinedToolClass */
                $witDefinedToolClass = $toolClass;
                $sourceWitPath = $witDefinedToolClass::witPath();
                $targetWitPath = sprintf('%s/%s.wit', $toolSourcesWitDirectory, $moduleName);

                if (!copy($sourceWitPath, $targetWitPath)) {
                    throw new ToolBuildException(sprintf('failed to copy WIT file from %s to %s', $sourceWitPath, $targetWitPath));
                }

            }

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
        $timeoutSeconds = (int) $this->config->get('superwire.cli.timeout_seconds', 120);
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

            $buildTasks[] = static function () use ($command, $workingDirectory, $timeoutSeconds, $toolName): string {

                $processResult = Process::path($workingDirectory)
                    ->timeout($timeoutSeconds)
                    ->run($command);

                return json_encode([
                    'tool_name' => $toolName,
                    'command' => $command,
                    'success' => $processResult->successful(),
                    'error_output' => trim($processResult->errorOutput()),
                    'standard_output' => trim($processResult->output()),
                ], JSON_THROW_ON_ERROR);

            };

        }

        try {

            $buildResults = Concurrency::driver('fork')->run($buildTasks);

        } catch (RuntimeException) {

            $buildResults = array_map(static fn (callable $buildTask): string => $buildTask(), $buildTasks);

        }

        foreach ($buildResults as $encodedBuildResult) {

            $buildResult = is_string($encodedBuildResult) ? json_decode($encodedBuildResult, true) : null;

            if (is_array($buildResult) && (($buildResult[ 'success' ] ?? false) === true)) {
                continue;
            }

            $toolName = is_array($buildResult) && is_string($buildResult[ 'tool_name' ] ?? null) ? $buildResult[ 'tool_name' ] : 'unknown';
            $command = is_array($buildResult) && is_array($buildResult[ 'command' ] ?? null) ? $buildResult[ 'command' ] : [];
            $errorOutput = is_array($buildResult) && is_string($buildResult[ 'error_output' ] ?? null) ? $buildResult[ 'error_output' ] : '';
            $standardOutput = is_array($buildResult) && is_string($buildResult[ 'standard_output' ] ?? null) ? $buildResult[ 'standard_output' ] : '';
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
        $templatePath = __DIR__ . '/../../resources/templates/tool_sources.Cargo.toml.tpl';

        if (!is_file($templatePath)) {
            throw new ToolBuildException(sprintf('tool sources Cargo template not found at %s', $templatePath));
        }

        $templateSource = file_get_contents($templatePath);

        if ($templateSource === false) {
            throw new ToolBuildException(sprintf('failed to read tool sources Cargo template at %s', $templatePath));
        }

        return $templateSource;
    }
}
