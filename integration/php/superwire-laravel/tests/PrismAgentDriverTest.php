<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests;

use Prism\Prism\Enums\FinishReason;
use Prism\Prism\Enums\ToolChoice;
use Prism\Prism\Facades\Prism;
use Prism\Prism\Testing\TextResponseFake;
use Prism\Prism\Text\Request as TextRequest;
use Prism\Prism\Text\Step as TextStep;
use Prism\Prism\Tool as PrismTool;
use Prism\Prism\ValueObjects\Messages\AssistantMessage;
use Prism\Prism\ValueObjects\Messages\ToolResultMessage;
use Prism\Prism\ValueObjects\Messages\UserMessage;
use Prism\Prism\ValueObjects\Meta;
use Prism\Prism\ValueObjects\ToolCall;
use Prism\Prism\ValueObjects\Usage;
use Superwire\Contracts\Agent\AgentConversationMessage;
use Superwire\Contracts\Agent\AgentExecutionRequest;
use Superwire\Contracts\Agent\AgentExpectedOutput;
use Superwire\Contracts\Agent\AgentToolCall;
use Superwire\Contracts\Agent\AgentToolDefinition;
use Superwire\Contracts\Agent\AgentToolResult;
use Superwire\Contracts\Agent\AgentTurnRequest;
use Superwire\Contracts\Agent\ConversationRole;
use Superwire\Contracts\HasCompletionToolStage;
use Superwire\Contracts\Provider\ProviderExecution;
use Superwire\Contracts\Support\LoopAgentDriver;
use Superwire\Laravel\Driver\PrismAgentDriver;
use Superwire\Laravel\Tools\WorkflowToolResult;
use Swaggest\JsonSchema\Schema;

final class PrismAgentDriverTest extends TestCase
{
    use HasCompletionToolStage;

    public function testItForcesFinalizeToolAfterPlainTextReply(): void
    {
        $this->ensurePrismBinding();

        $prismFake = Prism::fake([
            TextResponseFake::make()->withText('I think the answer is ready'),
            TextResponseFake::make()->withSteps(collect([
                $this->buildTextStep(toolCalls: [
                    new ToolCall('tool-call-1', $this->completionStage->finalizeSuccessToolName(), [ 'answer' => [ 'summary' => 'done' ] ]),
                ]),
            ])),
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
                jsonSchema: Schema::object()
                    ->setProperty('summary', Schema::string())
                    ->setRequired([ 'summary' ])
                    ->setAdditionalProperties(false),
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
            TextResponseFake::make()->withSteps(collect([
                $this->buildTextStep(toolCalls: [
                    new ToolCall('tool-call-2', $this->completionStage->finalizeSuccessToolName(), [ 'answer' => 'plain result' ]),
                ]),
            ])),
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
                jsonSchema: Schema::string(),
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
                    parametersSchema: Schema::object()
                        ->setProperty('task_id', Schema::integer())
                        ->setRequired([ 'task_id' ])
                        ->setAdditionalProperties(false),
                    strict: true,
                ),
            ],
            requireToolCall: false,
        ));

        $prismFake->assertRequest(function (array $requests): void {

            $this->assertCount(1, $requests);
            $this->assertInstanceOf(TextRequest::class, $requests[ 0 ]);

            $tools = $requests[ 0 ]->tools();
            $providerTools = $requests[ 0 ]->providerTools();

            $this->assertCount(1, $tools);
            $this->assertCount(0, $providerTools);
            $this->assertInstanceOf(PrismTool::class, $tools[ 0 ]);
            $this->assertSame('Fetch participants that answered a task in this project', $tools[ 0 ]->description());
            $this->assertTrue((bool) $tools[ 0 ]->providerOptions('strict'));

        });
    }

    public function testItBuildsPrismConversationMessagesByRole(): void
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
                new AgentConversationMessage(ConversationRole::Assistant, [
                    'content' => 'calling tool',
                    'tool_calls' => [
                        new AgentToolCall(
                            id: 'tool-call-1',
                            name: 'get_task_answer_by_participant',
                            arguments: [ 'participant_id' => 1 ],
                        ),
                    ],
                ]),
                new AgentConversationMessage(ConversationRole::ToolResult, [
                    'tool_results' => [
                        new AgentToolResult(
                            toolCallId: 'tool-call-1',
                            toolName: 'get_task_answer_by_participant',
                            arguments: [ 'participant_id' => 1 ],
                            result: WorkflowToolResult::success([
                                'status' => 'success',
                                'payload' => [
                                    'participant_id' => 1,
                                    'answer' => [ 'text' => 'hello world' ],
                                ],
                            ]),
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

            $this->assertCount(3, $messages);
            $this->assertInstanceOf(UserMessage::class, $messages[ 0 ]);
            $this->assertInstanceOf(AssistantMessage::class, $messages[ 1 ]);
            $this->assertInstanceOf(ToolResultMessage::class, $messages[ 2 ]);
            $this->assertSame('summarize participant', $messages[ 0 ]->content);
            $this->assertSame('calling tool', $messages[ 1 ]->content);
            $this->assertCount(1, $messages[ 1 ]->toolCalls);
            $this->assertSame('get_task_answer_by_participant', $messages[ 1 ]->toolCalls[ 0 ]->name);
            $this->assertSame('tool-call-1', $messages[ 1 ]->toolCalls[ 0 ]->resultId);
            $this->assertCount(1, $messages[ 2 ]->toolResults);
            $this->assertSame('get_task_answer_by_participant', $messages[ 2 ]->toolResults[ 0 ]->toolName);
            $this->assertSame('tool-call-1', $messages[ 2 ]->toolResults[ 0 ]->toolCallResultId);
            $this->assertSame('hello world', $messages[ 2 ]->toolResults[ 0 ]->result[ 'payload' ][ 'payload' ][ 'answer' ][ 'text' ]);

        });
    }

    private function ensurePrismBinding(): void
    {
        if ($this->app->bound('prism')) {
            return;
        }

        $this->app->singleton('prism', static fn (): \Prism\Prism\Prism => new \Prism\Prism\Prism());
    }

    /**
     * @param array<int, ToolCall> $toolCalls
     */
    private function buildTextStep(string $text = '', array $toolCalls = []): TextStep
    {
        return new TextStep(
            text: $text,
            finishReason: FinishReason::Stop,
            toolCalls: $toolCalls,
            toolResults: [],
            providerToolCalls: [],
            usage: new Usage(0, 0),
            meta: new Meta('fake', 'fake'),
            messages: [],
            systemPrompts: [],
            additionalContent: [],
        );
    }
}
