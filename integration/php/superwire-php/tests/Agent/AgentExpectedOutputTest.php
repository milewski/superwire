<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Tests\Agent;

use PHPUnit\Framework\TestCase;
use Superwire\Contracts\Agent\AgentExpectedOutput;
use Superwire\Contracts\Exception\InvalidWorkflowDefinitionException;
use Swaggest\JsonSchema\Schema;

final class AgentExpectedOutputTest extends TestCase
{
    public function testItConvertsContractJsonSchemaIntoSchemaObject(): void
    {
        $agentExpectedOutput = AgentExpectedOutput::fromContract([
            'workflow_type' => [ 'kind' => 'object' ],
            'json_schema' => [
                'type' => 'object',
                'properties' => [
                    'summary' => [ 'type' => 'string' ],
                ],
                'required' => [ 'summary' ],
                'additionalProperties' => false,
            ],
        ]);

        $this->assertInstanceOf(Schema::class, $agentExpectedOutput->jsonSchema);
        $this->assertSame('object', $this->schemaToArray($agentExpectedOutput->jsonSchema)[ 'type' ] ?? null);
    }

    public function testItRejectsContractWithoutJsonSchemaObject(): void
    {
        $this->expectException(InvalidWorkflowDefinitionException::class);

        AgentExpectedOutput::fromContract([
            'workflow_type' => [ 'kind' => 'string' ],
            'json_schema' => 'not-an-object',
        ]);
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
