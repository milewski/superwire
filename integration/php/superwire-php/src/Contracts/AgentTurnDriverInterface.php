<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Contracts;

use Superwire\Contracts\Agent\AgentTurnRequest;
use Superwire\Contracts\Agent\AgentTurnResponse;

interface AgentTurnDriverInterface
{
    public function generateTurn(AgentTurnRequest $request): AgentTurnResponse;
}
