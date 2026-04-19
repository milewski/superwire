<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests;

use Prism\Prism\Enums\ToolChoice;
use Prism\Prism\Facades\Prism;
use Prism\Prism\Testing\TextResponseFake;
use Prism\Prism\Text\Request as TextRequest;
use Prism\Prism\Tool as PrismTool;
use Prism\Prism\ValueObjects\ToolCall;
use Superwire\Contracts\Agent\AgentConversationMessage;
use Superwire\Contracts\Agent\AgentExecutionRequest;
use Superwire\Contracts\Agent\AgentExpectedOutput;
use Superwire\Contracts\Agent\AgentToolDefinition;
use Superwire\Contracts\Agent\AgentToolResult;
use Superwire\Contracts\Agent\AgentTurnRequest;
use Superwire\Contracts\Agent\ConversationRole;
use Superwire\Contracts\HasCompletionToolStage;
use Superwire\Contracts\Provider\ProviderExecution;
use Superwire\Contracts\Support\LoopAgentDriver;
use Superwire\Laravel\Driver\PrismAgentDriver;

final class PrismAgentDriverTest extends TestCase
{
    use HasCompletionToolStage;

    public function testItForcesFinalizeToolAfterPlainTextReply(): void
    {
        $this->ensurePrismBinding();

        $prismFake = Prism::fake([
            TextResponseFake::make()->withText('I think the answer is ready'),
            TextResponseFake::make()->withToolCalls([
                new ToolCall('tool-call-1', $this->completionStage->finalizeSuccessToolName(), [ 'answer' => [ 'summary' => 'done' ] ]),
            ]),
        ]);

        $driver = new LoopAgentDriver(new PrismAgentDriver());
        $request = new AgentExecutionRequest(
            agentName: 'quality_check',
            provider: new ProviderExecution(
                name: 'openai',
                provider: 'openai',
                config: [],
            ),
            model: 'gpt-4.1-mini',
            prompt: 'Analyze this',
            expectedOutput: new AgentExpectedOutput(
                workflowType: [ 'kind' => 'object', 'fields' => [ 'summary' => [ 'kind' => 'string' ] ] ],
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

        $result = $driver->execute($request);

        $this->assertSame([ 'summary' => 'done' ], $result->output);

        $prismFake->assertRequest(function (array $requests): void {

            $this->assertCount(2, $requests);
            $this->assertInstanceOf(TextRequest::class, $requests[ 0 ]);
            $this->assertInstanceOf(TextRequest::class, $requests[ 1 ]);
            $this->assertSame(ToolChoice::Auto, $requests[ 0 ]->toolChoice());
            $this->assertSame(ToolChoice::Any, $requests[ 1 ]->toolChoice());

        });
    }

    public function testItReturnsStringOnlyThroughFinalizeSuccessTool(): void
    {
        $this->ensurePrismBinding();

        $prismFake = Prism::fake([
            TextResponseFake::make()->withToolCalls([
                new ToolCall('tool-call-2', $this->completionStage->finalizeSuccessToolName(), [ 'answer' => 'plain result' ]),
            ]),
        ]);

        $driver = new LoopAgentDriver(new PrismAgentDriver());
        $request = new AgentExecutionRequest(
            agentName: 'simple_agent',
            provider: new ProviderExecution(
                name: 'openai',
                provider: 'openai',
                config: [],
            ),
            model: 'gpt-4.1-mini',
            prompt: 'Say hi',
            expectedOutput: new AgentExpectedOutput(
                workflowType: [ 'kind' => 'string' ],
                jsonSchema: [ 'type' => 'string' ],
            ),
        );

        $result = $driver->execute($request);

        $this->assertSame('plain result', $result->output);

        $prismFake->assertRequest(function (array $requests): void {

            $this->assertCount(1, $requests);
            $this->assertInstanceOf(TextRequest::class, $requests[ 0 ]);

        });
    }

    public function testItPassesToolDescriptionAndStrictModeToPrism(): void
    {
        $this->ensurePrismBinding();

        $prismFake = Prism::fake([
            TextResponseFake::make()->withText('ok'),
        ]);

        $driver = new PrismAgentDriver();

        $driver->generateTurn(new AgentTurnRequest(
            provider: 'openai',
            model: 'gpt-4.1-mini',
            providerConfig: [],
            messages: [ new AgentConversationMessage(ConversationRole::User, [ 'content' => 'do something' ]) ],
            tools: [
                new AgentToolDefinition(
                    name: 'get_answered_participants_for_task',
                    description: 'Fetch participants that answered a task in this project',
                    parametersSchema: [
                        'type' => 'object',
                        'properties' => [
                            'task_id' => [ 'type' => 'integer' ],
                        ],
                        'required' => [ 'task_id' ],
                        'additionalProperties' => false,
                    ],
                    strict: true,
                ),
            ],
            requireToolCall: false,
        ));

        $prismFake->assertRequest(function (array $requests): void {

            $this->assertCount(1, $requests);
            $this->assertInstanceOf(TextRequest::class, $requests[ 0 ]);

            $tools = $requests[ 0 ]->tools();

            $this->assertCount(1, $tools);
            $this->assertInstanceOf(PrismTool::class, $tools[ 0 ]);
            $this->assertSame('Fetch participants that answered a task in this project', $tools[ 0 ]->description());
            $this->assertTrue((bool) $tools[ 0 ]->providerOptions('strict'));

        });
    }

    public function testItIncludesToolResultsInConversationPrompt(): void
    {
        $this->ensurePrismBinding();

        $prismFake = Prism::fake([
            TextResponseFake::make()->withText('ok'),
        ]);

        $driver = new PrismAgentDriver();

        $driver->generateTurn(new AgentTurnRequest(
            provider: 'openai',
            model: 'gpt-4.1-mini',
            providerConfig: [],
            messages: [
                new AgentConversationMessage(ConversationRole::User, [ 'content' => 'summarize participant' ]),
                new AgentConversationMessage(ConversationRole::ToolResult, [
                    'tool_results' => [
                        new AgentToolResult(
                            toolCallId: 'tool-call-1',
                            toolName: 'get_task_answer_by_participant',
                            arguments: [ 'participant_id' => 1 ],
                            result: [
                                'status' => 'success',
                                'payload' => [
                                    'participant_id' => 1,
                                    'answer' => [ 'text' => 'hello world' ],
                                ],
                            ],
                        ),
                    ],
                ]),
            ],
            tools: [],
            requireToolCall: false,
        ));

        $prismFake->assertRequest(function (array $requests): void {

            $this->assertCount(1, $requests);
            $this->assertInstanceOf(TextRequest::class, $requests[ 0 ]);

            $messages = $requests[ 0 ]->messages();

            $this->assertCount(1, $messages);
            $this->assertStringContainsString('[tool_result]', $messages[ 0 ]->content);
            $this->assertStringContainsString('get_task_answer_by_participant', $messages[ 0 ]->content);
            $this->assertStringContainsString('hello world', $messages[ 0 ]->content);

        });
    }

    private function ensurePrismBinding(): void
    {
        if ($this->app->bound('prism')) {
            return;
        }

        $this->app->singleton('prism', static fn (): \Prism\Prism\Prism => new \Prism\Prism\Prism());
    }
}
