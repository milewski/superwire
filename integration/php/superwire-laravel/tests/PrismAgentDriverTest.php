<?php

declare(strict_types=1);

namespace Superwire\Laravel\Tests;

use Prism\Prism\Enums\ToolChoice;
use Prism\Prism\Facades\Prism;
use Prism\Prism\Testing\TextResponseFake;
use Prism\Prism\Text\Request as TextRequest;
use Prism\Prism\ValueObjects\ToolCall;
use Superwire\Contracts\AgentExpectedOutput;
use Superwire\Contracts\AgentExecutionRequest;
use Superwire\Contracts\ProviderExecution;
use Superwire\Contracts\Support\LoopAgentDriver;
use Superwire\Laravel\Driver\PrismAgentDriver;

final class PrismAgentDriverTest extends TestCase
{
    public function testItForcesFinalizeToolAfterPlainTextReply(): void
    {
        $prismFake = Prism::fake([
            TextResponseFake::make()->withText('I think the answer is ready'),
            TextResponseFake::make()->withToolCalls([
                new ToolCall('tool-call-1', 'finalize_success', ['answer' => ['summary' => 'done']]),
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
                workflowType: ['kind' => 'object', 'fields' => ['summary' => ['kind' => 'string']]],
                jsonSchema: [
                    'type' => 'object',
                    'properties' => [
                        'summary' => ['type' => 'string'],
                    ],
                    'required' => ['summary'],
                    'additionalProperties' => false,
                ],
            ),
        );

        $result = $driver->execute($request);

        self::assertSame(['summary' => 'done'], $result->output);

        $prismFake->assertRequest(static function (array $requests): void {
            self::assertCount(2, $requests);
            self::assertInstanceOf(TextRequest::class, $requests[0]);
            self::assertInstanceOf(TextRequest::class, $requests[1]);
            self::assertSame(ToolChoice::Auto, $requests[0]->toolChoice());
            self::assertSame(ToolChoice::Any, $requests[1]->toolChoice());
        });
    }

    public function testItReturnsStringOnlyThroughFinalizeSuccessTool(): void
    {
        $prismFake = Prism::fake([
            TextResponseFake::make()->withToolCalls([
                new ToolCall('tool-call-2', 'finalize_success', ['answer' => 'plain result']),
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
                workflowType: ['kind' => 'string'],
                jsonSchema: ['type' => 'string'],
            ),
        );

        $result = $driver->execute($request);

        self::assertSame('plain result', $result->output);

        $prismFake->assertRequest(static function (array $requests): void {
            self::assertCount(1, $requests);
            self::assertInstanceOf(TextRequest::class, $requests[0]);
        });
    }
}
