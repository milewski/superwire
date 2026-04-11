<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests\Unit;

use Illuminate\Config\Repository;
use Superwire\Laravel\Contracts\Tool;
use Superwire\Laravel\Contracts\ToolBoundInputData;
use Superwire\Laravel\Contracts\ToolInputData;
use Superwire\Laravel\Execution\Compiler\ToolEndpointResolver;
use Superwire\Laravel\Execution\Compiler\ToolModuleSourceGenerator;
use Superwire\Laravel\Execution\Compiler\ToolModuleTemplateRenderer;
use Superwire\Laravel\Execution\Compiler\ToolNameFormatter;
use Superwire\Laravel\Execution\Compiler\ToolSchemaPayloadSerializer;
use Superwire\Laravel\Tests\TestCase;
use Swaggest\JsonSchema\Schema;

final class ToolModuleSourceGeneratorTest extends TestCase
{
    public function testGeneratesRustModuleSourceFromExternalTemplate(): void
    {
        $config = new Repository([
            'superwire' => [
                'tools' => [
                    'http_endpoint_base_url' => 'http://127.0.0.1:8000',
                    'http_prefix' => 'superwire/tools',
                ],
            ],
        ]);

        $toolModuleSourceGenerator = new ToolModuleSourceGenerator(
            new ToolNameFormatter(),
            new ToolEndpointResolver($config),
            new ToolSchemaPayloadSerializer(),
            new ToolModuleTemplateRenderer(__DIR__ . '/../../resources/templates/tool_module.rs.tpl'),
        );

        $moduleSource = $toolModuleSourceGenerator->generate(ToolModuleSourceGeneratorTestTool::class);

        $this->assertStringContainsString('pub struct EchoToolAgentInput(Value);', $moduleSource);
        $this->assertStringContainsString('pub struct EchoToolBoundInput(Value);', $moduleSource);
        $this->assertStringContainsString('pub struct EchoToolOutput(Value);', $moduleSource);
        $this->assertStringContainsString('tool = EchoToolTool,', $moduleSource);
        $this->assertStringContainsString('name = "echo_tool"', $moduleSource);
        $this->assertStringContainsString('endpoint = "http://127.0.0.1:8000/superwire/tools/echo_tool/execute"', $moduleSource);
        $this->assertStringNotContainsString('{{', $moduleSource);
    }
}

final class ToolModuleSourceGeneratorTestTool implements Tool
{
    public static function name(): string
    {
        return 'echo_tool';
    }

    public static function description(): string
    {
        return 'Echo test tool';
    }

    public static function endpointName(): string
    {
        return 'echo_tool';
    }

    public static function agentInputClass(): string
    {
        return ToolModuleSourceGeneratorTestAgentInput::class;
    }

    public static function boundInputClass(): string
    {
        return ToolModuleSourceGeneratorTestBoundInput::class;
    }

    public static function outputClass(): string
    {
        return ToolModuleSourceGeneratorTestOutput::class;
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
        return new ToolModuleSourceGeneratorTestAgentInput();
    }

    public static function resolveBoundInput(array $boundInputPayload): ToolBoundInputData
    {
        return new ToolModuleSourceGeneratorTestBoundInput();
    }

    public function execute(ToolInputData $agentInput, ToolBoundInputData $boundInput): array
    {
        return [];
    }
}

final class ToolModuleSourceGeneratorTestAgentInput implements ToolInputData
{
}

final class ToolModuleSourceGeneratorTestBoundInput implements ToolBoundInputData
{
}

final class ToolModuleSourceGeneratorTestOutput
{
}
