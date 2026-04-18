<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Tests\Fakes;

use Closure;
use Superwire\Contracts\Agent\AgentTurnRequest;
use Superwire\Contracts\Agent\AgentTurnResponse;
use Superwire\Contracts\Contracts\AgentTurnDriverInterface;

final class ScriptedTurnDriver implements AgentTurnDriverInterface
{
    /**
     * @param list<AgentTurnResponse> $responses
     */
    public static function fake(array $responses): self
    {
        return new self(
            responseFactory: static function (int $turnIndex) use ($responses): AgentTurnResponse {
                return $responses[ $turnIndex ] ?? new AgentTurnResponse([]);
            },
        );
    }

    /**
     * @var list<AgentTurnRequest>
     */
    public array $requests = [];

    /**
     * @param Closure(int, AgentTurnRequest): AgentTurnResponse $responseFactory
     */
    public function __construct(
        private readonly Closure $responseFactory,
    )
    {
    }

    public function generateTurn(AgentTurnRequest $request): AgentTurnResponse
    {
        $this->requests[] = $request;
        $turnIndex = count($this->requests) - 1;

        return ($this->responseFactory)($turnIndex, $request);
    }
}
