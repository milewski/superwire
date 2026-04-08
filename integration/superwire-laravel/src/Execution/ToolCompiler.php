<?php

namespace Superwire\Laravel\Execution;

use Illuminate\Contracts\Config\Repository;
use JsonException;
use Superwire\Laravel\Contracts\Tool;
use Superwire\Laravel\Data\ToolBuildRequest;
use Superwire\Laravel\Data\ToolBuildResult;
use Superwire\Laravel\Exceptions\InvalidToolClassException;
use Superwire\Laravel\Exceptions\ToolBuildException;
use Swaggest\JsonSchema\Schema;
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
        $validatedToolClasses = $this->validatedToolClasses($toolBuildRequest->toolClasses);
        $buildRootDirectory = (string) $this->config->get('superwire.build.root_directory', storage_path('app/superwire'));
        $toolOutputDirectory = (string) $this->config->get('superwire.build.tools_directory', base_path('tools'));
        $toolSourcesDirectory = $buildRootDirectory . '/tool-sources/src';
        $toolSourcesManifestPath = $buildRootDirectory . '/tool-sources/Cargo.toml';
        $toolSourcesLibPath = $toolSourcesDirectory . '/lib.rs';

        if (!is_dir($toolSourcesDirectory) && !mkdir($toolSourcesDirectory, 0777, true) && !is_dir($toolSourcesDirectory)) {
            throw new ToolBuildException(sprintf('failed to create tool sources directory %s', $toolSourcesDirectory));
        }

        if (!is_dir($toolOutputDirectory) && !mkdir($toolOutputDirectory, 0777, true) && !is_dir($toolOutputDirectory)) {
            throw new ToolBuildException(sprintf('failed to create tool output directory %s', $toolOutputDirectory));
        }

        $wasmToolSdkPath = (string) $this->config->get('superwire.build.wasm_tool_sdk_path', '');

        if ($wasmToolSdkPath === '') {
            throw new ToolBuildException('missing superwire.build.wasm_tool_sdk_path configuration');
        }

        file_put_contents($toolSourcesManifestPath, $this->toolSourcesCargoManifest($wasmToolSdkPath));

        $moduleNames = [];
        $toolRegistryMap = [];

        foreach ($validatedToolClasses as $toolClass) {
            $toolName = $toolClass::name();
            $moduleName = $this->moduleName($toolName);
            $moduleNames[] = $moduleName;
            $toolRegistryMap[ $toolName ] = [
                'class' => $toolClass,
                'description' => $toolClass::description(),
                'endpoint_name' => $toolClass::endpointName(),
                'input_schema' => $this->schemaPayload($toolClass::inputSchema()),
                'bound_input_schema' => $this->schemaPayload($toolClass::boundInputSchema()),
                'output_schema' => $this->schemaPayload($toolClass::outputSchema()),
            ];

            $modulePath = sprintf('%s/%s.rs', $toolSourcesDirectory, $moduleName);
            file_put_contents($modulePath, $this->toolModuleSource($toolClass));
        }

        $libSourceLines = array_map(
            static fn (string $moduleName): string => sprintf('pub mod %s;', $moduleName),
            $moduleNames,
        );

        file_put_contents($toolSourcesLibPath, implode(PHP_EOL, $libSourceLines) . PHP_EOL);

        $registryManifestPath = $buildRootDirectory . '/tool-registry.json';
        file_put_contents($registryManifestPath, json_encode([ 'tools' => $toolRegistryMap ], JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR));

        $command = [
            (string) $this->config->get('superwire.cli.binary', 'cli'),
            'tools',
            'build',
            $buildRootDirectory,
            '--output',
            $toolOutputDirectory,
        ];

        $process = new Process(
            command: $command,
            cwd: (string) $this->config->get('superwire.cli.working_directory', base_path()),
            env: null,
            input: null,
            timeout: (float) $this->config->get('superwire.cli.timeout_seconds', 120),
        );

        $process->run();

        if (!$process->isSuccessful()) {

            throw new ToolBuildException(sprintf(
                "failed to build tool wasm modules using `%s`: %s",
                implode(' ', $command),
                trim($process->getErrorOutput()) !== '' ? trim($process->getErrorOutput()) : trim($process->getOutput()),
            ));

        }

        return new ToolBuildResult(array_keys($toolRegistryMap), $toolOutputDirectory);
    }

    /**
     * @param list<class-string> $toolClasses
     * @return list<class-string<Tool>>
     */
    private function validatedToolClasses(array $toolClasses): array
    {
        $validatedToolClasses = [];

        foreach ($toolClasses as $toolClass) {
            if (!is_string($toolClass)) {
                throw new InvalidToolClassException('tool class references must be class-string values');
            }

            if (!class_exists($toolClass)) {
                throw new InvalidToolClassException(sprintf('tool class `%s` does not exist', $toolClass));
            }

            if (!is_subclass_of($toolClass, Tool::class)) {
                throw new InvalidToolClassException(sprintf('tool class `%s` must implement %s', $toolClass, Tool::class));
            }

            $validatedToolClasses[] = $toolClass;
        }

        return $validatedToolClasses;
    }

    private function moduleName(string $toolName): string
    {
        return str_replace('-', '_', $toolName);
    }

    /**
     * @param class-string<Tool> $toolClass
     */
    private function toolModuleSource(string $toolClass): string
    {
        $toolName = $toolClass::name();
        $toolDescription = $toolClass::description();
        $toolEndpoint = $this->toolEndpoint($toolClass::endpointName());

        return sprintf(
            <<<'RUST'
use serde_json::Value;

superwire_wasm_tool_sdk::php_proxy_tool!(
    tool = %sTool,
    name = "%s",
    description = "%s",
    endpoint = "%s",
    input = Value,
    bound_input = Value,
    output = Value,
);
RUST,
            $this->typeName($toolName),
            addslashes($toolName),
            addslashes($toolDescription),
            addslashes($toolEndpoint),
        );
    }

    private function toolEndpoint(string $endpointName): string
    {
        $baseUrl = rtrim((string) $this->config->get('superwire.tools.http_endpoint_base_url', 'http://127.0.0.1:8000'), '/');
        $prefix = trim((string) $this->config->get('superwire.tools.http_prefix', 'superwire/tools'), '/');

        return sprintf('%s/%s/%s/execute', $baseUrl, $prefix, $endpointName);
    }

    private function typeName(string $toolName): string
    {
        $segments = preg_split('/[_\-]+/', $toolName) ?: [ $toolName ];
        $typeName = '';

        foreach ($segments as $segment) {
            if ($segment === '') {
                continue;
            }

            $typeName .= ucfirst($segment);
        }

        return $typeName === '' ? 'ProxyTool' : $typeName;
    }

    private function toolSourcesCargoManifest(string $wasmToolSdkPath): string
    {
        return sprintf(
            <<<'TOML'
[package]
name = "superwire_php_tools"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"

[dependencies]
serde_json = "1.0"
superwire-wasm-tool-sdk = { path = "%s" }
TOML,
            addslashes($wasmToolSdkPath),
        );
    }

    /**
     * @return array<string, mixed>
     * @throws JsonException
     */
    private function schemaPayload(Schema $schema): array
    {
        $serializedSchema = json_encode($schema, JSON_THROW_ON_ERROR);
        $decodedSchema = json_decode($serializedSchema, true, 512, JSON_THROW_ON_ERROR);

        if (!is_array($decodedSchema)) {
            throw new ToolBuildException('tool schema must serialize to a json object');
        }

        return $decodedSchema;
    }
}
