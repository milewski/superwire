<?php

declare(strict_types=1);

namespace Superwire\Contracts\Contracts;

use Superwire\Contracts\AgentTurnRequest;
use Superwire\Contracts\AgentTurnResponse;

interface AgentTurnDriverInterface
{
    public function generateTurn(AgentTurnRequest $request): AgentTurnResponse;
}
