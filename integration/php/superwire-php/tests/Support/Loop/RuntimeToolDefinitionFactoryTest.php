<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Tests;

use PHPUnit\Framework\TestCase;
use Superwire\Contracts\Agent\AgentExecutionRequest;
use Superwire\Contracts\Agent\AgentToolCall;
use Superwire\Contracts\Agent\AgentToolResult;
use Superwire\Contracts\Contracts\RuntimeToolInvokerInterface;
use Superwire\Contracts\Contracts\RuntimeToolMetadataProviderInterface;
use Superwire\Contracts\Contracts\RuntimeToolSchemaProviderInterface;
use Superwire\Contracts\Support\Loop\RuntimeToolDefinitionFactory;
use Swaggest\JsonSchema\Schema;

final class RuntimeToolDefinitionFactoryTest extends TestCase
{
    public function testItUsesSchemaAndMetadataFromRuntimeInvoker(): void
    {
        $runtimeToolInvoker = new class () implements RuntimeToolInvokerInterface, RuntimeToolMetadataProviderInterface, RuntimeToolSchemaProviderInterface {
            public function invoke(AgentExecutionRequest $request, AgentToolCall $toolCall): AgentToolResult
            {
                return new AgentToolResult($toolCall->id, $toolCall->name, $toolCall->arguments, [ 'ok' => true ]);
            }

            public function schemaForTool(string $toolName): ?Schema
            {
                return Schema::object()
                    ->setProperty('entity_id', Schema::integer())
                    ->setRequired([ 'entity_id' ])
                    ->setAdditionalProperties(false);
            }

            public function descriptionForTool(string $toolName): ?string
            {
                return 'fetch entity details by id';
            }

            public function strictSchemaForTool(string $toolName): ?bool
            {
                return false;
            }
        };

        $runtimeToolDefinitionFactory = new RuntimeToolDefinitionFactory($runtimeToolInvoker);
        $toolDefinition = $runtimeToolDefinitionFactory->definitionForToolName('fetch_entity');
        $schema = $this->schemaToArray($toolDefinition->parametersSchema);

        $this->assertSame('fetch_entity', $toolDefinition->name);
        $this->assertSame('fetch entity details by id', $toolDefinition->description);
        $this->assertSame('integer', $schema[ 'properties' ][ 'entity_id' ][ 'type' ] ?? null);
        $this->assertFalse($toolDefinition->strict);
    }

    public function testItFallsBackToGenericStrictSchemaWithoutProjectSpecificRules(): void
    {
        $runtimeToolDefinitionFactory = new RuntimeToolDefinitionFactory(null);
        $toolDefinition = $runtimeToolDefinitionFactory->definitionForToolName('get_task_by_participant');
        $schema = $this->schemaToArray($toolDefinition->parametersSchema);

        $this->assertSame('get_task_by_participant', $toolDefinition->name);
        $this->assertSame('Execute runtime tool `get_task_by_participant` and use result to continue', $toolDefinition->description);
        $this->assertSame('object', $schema[ 'type' ] ?? null);
        $this->assertSame([], $schema[ 'properties' ] ?? null);
        $this->assertFalse(array_key_exists('required', $schema));
        $this->assertTrue($toolDefinition->strict);
    }

    /**
     * @return array<string, mixed>
     */
    private function schemaToArray(Schema $schema): array
    {
        $decodedSchema = json_decode(json_encode($schema, JSON_THROW_ON_ERROR), true, 512, JSON_THROW_ON_ERROR);

        $this->assertIsArray($decodedSchema);

        return $decodedSchema;
    }
}
