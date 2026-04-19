<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Tests;

use PHPUnit\Framework\TestCase;
use Superwire\Contracts\Agent\AgentExecutionRequest;
use Superwire\Contracts\Agent\AgentExpectedOutput;
use Superwire\Contracts\Provider\ProviderExecution;
use Superwire\Contracts\Support\Loop\CompletionToolDefinitionFactory;
use Superwire\Contracts\Support\Stages\CompletionToolLoopStage;
use Swaggest\JsonSchema\Schema;

final class CompletionToolDefinitionFactoryTest extends TestCase
{
    public function testItBuildsFinalizeSuccessAndErrorToolDefinitions(): void
    {
        $completionToolLoopStage = new CompletionToolLoopStage();
        $completionToolDefinitionFactory = new CompletionToolDefinitionFactory($completionToolLoopStage);
        $agentExecutionRequest = new AgentExecutionRequest(
            agentName: 'summary',
            provider: new ProviderExecution('openai', 'openai', []),
            model: 'gpt-4.1-mini',
            prompt: 'summarize',
            expectedOutput: new AgentExpectedOutput(
                workflowType: [ 'kind' => 'object' ],
                jsonSchema: Schema::object()
                    ->setProperty('summary', Schema::string())
                    ->setRequired([ 'summary' ])
                    ->setAdditionalProperties(false),
            ),
        );

        $successToolDefinition = $completionToolDefinitionFactory->finalizeSuccessTool($agentExecutionRequest);
        $errorToolDefinition = $completionToolDefinitionFactory->finalizeErrorTool();
        $successToolSchema = $this->schemaToArray($successToolDefinition->parametersSchema);
        $errorToolSchema = $this->schemaToArray($errorToolDefinition->parametersSchema);
        $expectedOutputSchema = $this->schemaToArray($agentExecutionRequest->expectedOutput->jsonSchema);

        $this->assertSame($completionToolLoopStage->finalizeSuccessToolName(), $successToolDefinition->name);
        $this->assertSame('object', $successToolSchema[ 'type' ] ?? null);
        $this->assertSame([ 'answer' ], $successToolSchema[ 'required' ] ?? null);
        $this->assertSame($expectedOutputSchema, $successToolSchema[ 'properties' ][ 'answer' ] ?? null);

        $this->assertSame($completionToolLoopStage->finalizeErrorToolName(), $errorToolDefinition->name);
        $this->assertSame([ 'reason' ], $errorToolSchema[ 'required' ] ?? null);
        $this->assertSame('string', $errorToolSchema[ 'properties' ][ 'reason' ][ 'type' ] ?? null);
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
