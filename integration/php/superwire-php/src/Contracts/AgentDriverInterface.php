<?php

declare(strict_types=1);

namespace Superwire\Contracts\Contracts;

use Superwire\Contracts\AgentExecutionRequest;
use Superwire\Contracts\AgentExecutionResult;

interface AgentDriverInterface
{
    public function execute(AgentExecutionRequest $request): AgentExecutionResult;
}
