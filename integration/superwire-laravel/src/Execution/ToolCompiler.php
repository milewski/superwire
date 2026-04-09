<?php

declare(strict_types = 1);

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

        if (!is_dir($toolSourcesDirectory) && !mkdir($toolSourcesDirectory, 0o777, true) && !is_dir($toolSourcesDirectory)) {
            throw new ToolBuildException(sprintf('failed to create tool sources directory %s', $toolSourcesDirectory));
        }

        if (!is_dir($toolOutputDirectory) && !mkdir($toolOutputDirectory, 0o777, true) && !is_dir($toolOutputDirectory)) {
            throw new ToolBuildException(sprintf('failed to create tool output directory %s', $toolOutputDirectory));
        }

        file_put_contents($toolSourcesManifestPath, $this->toolSourcesCargoManifest());

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
                'failed to build tool wasm modules using `%s`: %s',
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
        $toolTypeName = $this->typeName($toolName);
        $agentInputTypeName = sprintf('%sAgentInput', $toolTypeName);
        $boundInputTypeName = sprintf('%sBoundInput', $toolTypeName);
        $outputTypeName = sprintf('%sOutput', $toolTypeName);

        $agentInputSchemaJson = addslashes(json_encode($toolClass::inputSchema(), JSON_THROW_ON_ERROR));
        $boundInputSchemaJson = addslashes(json_encode($toolClass::boundInputSchema(), JSON_THROW_ON_ERROR));
        $outputSchemaJson = addslashes(json_encode($toolClass::outputSchema(), JSON_THROW_ON_ERROR));

        return sprintf(
            <<<'RUST'
            use std::borrow::Cow;

            use schemars::{JsonSchema, Schema, SchemaGenerator};
            use serde::{Deserialize, Serialize};
            use serde_json::Value;

            #[derive(Debug, Clone, Serialize, Deserialize)]
            #[serde(transparent)]
            pub struct %s(Value);

            impl JsonSchema for %s {
                fn schema_name() -> Cow<'static, str> {
                    Cow::Borrowed("%s")
                }

                fn json_schema(schema_generator: &mut SchemaGenerator) -> Schema {
                    let _ = schema_generator;

                    serde_json::from_str::<Schema>("%s").expect("agent input schema json should be valid")
                }
            }

            #[derive(Debug, Clone, Serialize, Deserialize)]
            #[serde(transparent)]
            pub struct %s(Value);

            impl JsonSchema for %s {
                fn schema_name() -> Cow<'static, str> {
                    Cow::Borrowed("%s")
                }

                fn json_schema(schema_generator: &mut SchemaGenerator) -> Schema {
                    let _ = schema_generator;

                    serde_json::from_str::<Schema>("%s").expect("bound input schema json should be valid")
                }
            }

            #[derive(Debug, Clone, Serialize, Deserialize)]
            #[serde(transparent)]
            pub struct %s(Value);

            impl JsonSchema for %s {
                fn schema_name() -> Cow<'static, str> {
                    Cow::Borrowed("%s")
                }

                fn json_schema(schema_generator: &mut SchemaGenerator) -> Schema {
                    let _ = schema_generator;

                    serde_json::from_str::<Schema>("%s").expect("output schema json should be valid")
                }
            }

            crate::php_proxy_tool!(
                tool = %sTool,
                name = "%s",
                description = "%s",
                endpoint = "%s",
                input = %s,
                bound_input = %s,
                output = %s,
            );
            RUST,
            $agentInputTypeName,
            $agentInputTypeName,
            $agentInputTypeName,
            $agentInputSchemaJson,
            $boundInputTypeName,
            $boundInputTypeName,
            $boundInputTypeName,
            $boundInputSchemaJson,
            $outputTypeName,
            $outputTypeName,
            $outputTypeName,
            $outputSchemaJson,
            $toolTypeName,
            addslashes($toolName),
            addslashes($toolDescription),
            addslashes($toolEndpoint),
            $agentInputTypeName,
            $boundInputTypeName,
            $outputTypeName,
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

    /**
     * @throws JsonException
     * @return array<string, mixed>
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
