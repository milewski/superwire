<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Contracts;

use Superwire\Contracts\Agent\AgentExecutionRequest;
use Superwire\Contracts\Agent\AgentExecutionResult;

interface AgentDriverInterface
{
    public function execute(AgentExecutionRequest $request): AgentExecutionResult;
}
