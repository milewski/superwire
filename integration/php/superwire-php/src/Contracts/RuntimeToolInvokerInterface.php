<?php

declare(strict_types=1);

namespace Superwire\Contracts\Contracts;

use Superwire\Contracts\AgentExecutionRequest;
use Superwire\Contracts\AgentToolCall;
use Superwire\Contracts\AgentToolResult;

interface RuntimeToolInvokerInterface
{
    public function invoke(AgentExecutionRequest $request, AgentToolCall $toolCall): AgentToolResult;
}
