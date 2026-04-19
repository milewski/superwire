<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Tests\Fakes;

use Superwire\Contracts\Agent\AgentTurnRequest;
use Superwire\Contracts\Agent\AgentTurnResponse;
use Superwire\Contracts\Contracts\AgentTurnDriverInterface;

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
