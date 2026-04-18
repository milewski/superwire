<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Tests;

use PHPUnit\Framework\TestCase;
use Superwire\Contracts\Agent\AgentExecutionRequest;
use Superwire\Contracts\Agent\AgentExpectedOutput;
use Superwire\Contracts\Agent\AgentToolCall;
use Superwire\Contracts\Agent\AgentTurnRequest;
use Superwire\Contracts\Agent\AgentTurnResponse;
use Superwire\Contracts\Contracts\AgentTurnDriverInterface;
use Superwire\Contracts\Provider\ProviderExecution;
use Superwire\Contracts\Support\LoopAgentDriver;

final class LoopAgentDriverTest extends TestCase
{
    public function testItForcesCompletionToolWhenModelReturnsOnlyText(): void
    {
        $turnDriver = new FakeTurnDriver([
            new AgentTurnResponse([], 'plain text with no tool calls'),
            new AgentTurnResponse([ new AgentToolCall('1', 'finalize_success', [ 'answer' => [ 'summary' => 'ok' ] ]) ]),
        ]);

        $driver = new LoopAgentDriver($turnDriver);

        $result = $driver->execute(new AgentExecutionRequest(
            agentName: 'summary',
            provider: new ProviderExecution('openai', 'openai', []),
            model: 'gpt-4.1-mini',
            prompt: 'do work',
            expectedOutput: new AgentExpectedOutput(
                workflowType: [ 'kind' => 'object', 'fields' => [ 'summary' => [ 'kind' => 'string' ] ] ],
                jsonSchema: [ 'type' => 'object' ],
            ),
        ));

        self::assertSame([ 'summary' => 'ok' ], $result->output);
        self::assertCount(2, $turnDriver->requests);
        self::assertFalse($turnDriver->requests[ 0 ]->requireToolCall);
        self::assertTrue($turnDriver->requests[ 1 ]->requireToolCall);
    }

    public function testItSynthesizesFinalizeSuccessFromTextInForcedCompletionMode(): void
    {
        $turnDriver = new FakeTurnDriver([
            new AgentTurnResponse([], 'plain text with no tool calls'),
            new AgentTurnResponse([], 'final plain text'),
        ]);

        $driver = new LoopAgentDriver($turnDriver);

        $result = $driver->execute(new AgentExecutionRequest(
            agentName: 'summary',
            provider: new ProviderExecution('openai', 'openai', []),
            model: 'gpt-4.1-mini',
            prompt: 'do work',
            expectedOutput: new AgentExpectedOutput(
                workflowType: [ 'kind' => 'string' ],
                jsonSchema: [ 'type' => 'string' ],
            ),
        ));

        self::assertSame('final plain text', $result->output);
        self::assertTrue((bool) ($result->metadata[ 'synthetic_completion' ] ?? false));
    }
}

final class FakeTurnDriver implements AgentTurnDriverInterface
{
    /**
     * @var array<int, AgentTurnRequest>
     */
    public array $requests = [];

    /**
     * @param array<int, AgentTurnResponse> $responses
     */
    public function __construct(private array $responses)
    {
    }

    public function generateTurn(AgentTurnRequest $request): AgentTurnResponse
    {
        $this->requests[] = $request;

        return array_shift($this->responses) ?? new AgentTurnResponse([]);
    }
}
