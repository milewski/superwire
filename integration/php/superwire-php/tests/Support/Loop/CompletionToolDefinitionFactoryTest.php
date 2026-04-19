<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Tests;

use PHPUnit\Framework\TestCase;
use Superwire\Contracts\Agent\AgentExecutionRequest;
use Superwire\Contracts\Agent\AgentExpectedOutput;
use Superwire\Contracts\Provider\ProviderExecution;
use Superwire\Contracts\Support\Loop\CompletionToolDefinitionFactory;
use Superwire\Contracts\Support\Stages\CompletionToolLoopStage;

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
                jsonSchema: [
                    'type' => 'object',
                    'properties' => [
                        'summary' => [ 'type' => 'string' ],
                    ],
                    'required' => [ 'summary' ],
                    'additionalProperties' => false,
                ],
            ),
        );

        $successToolDefinition = $completionToolDefinitionFactory->finalizeSuccessTool($agentExecutionRequest);
        $errorToolDefinition = $completionToolDefinitionFactory->finalizeErrorTool();

        $this->assertSame($completionToolLoopStage->finalizeSuccessToolName(), $successToolDefinition->name);
        $this->assertSame('object', $successToolDefinition->parametersSchema[ 'type' ] ?? null);
        $this->assertSame([ 'answer' ], $successToolDefinition->parametersSchema[ 'required' ] ?? null);
        $this->assertSame($agentExecutionRequest->expectedOutput->jsonSchema, $successToolDefinition->parametersSchema[ 'properties' ][ 'answer' ] ?? null);

        $this->assertSame($completionToolLoopStage->finalizeErrorToolName(), $errorToolDefinition->name);
        $this->assertSame([ 'reason' ], $errorToolDefinition->parametersSchema[ 'required' ] ?? null);
        $this->assertSame('string', $errorToolDefinition->parametersSchema[ 'properties' ][ 'reason' ][ 'type' ] ?? null);
    }
}
