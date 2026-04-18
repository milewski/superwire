<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Contracts;

use Superwire\Contracts\Agent\AgentExecutionRequest;
use Superwire\Contracts\Agent\AgentToolCall;
use Superwire\Contracts\Agent\AgentToolResult;

interface RuntimeToolInvokerInterface
{
    public function invoke(AgentExecutionRequest $request, AgentToolCall $toolCall): AgentToolResult;
}
