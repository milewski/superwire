<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Contracts;

use Superwire\Contracts\Agent\AgentExecutionRequest;
use Superwire\Contracts\Agent\AgentToolCall;
use Superwire\Contracts\Agent\AgentToolResult;

interface RuntimeToolBatchInvokerInterface
{
    /**
     * @param array<int, AgentToolCall> $toolCalls
     * @return array<int, AgentToolResult>
     */
    public function invokeBatch(AgentExecutionRequest $request, array $toolCalls): array;
}
