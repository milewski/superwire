<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Support;

use Superwire\Contracts\Agent\AgentExecutionRequest;
use Superwire\Contracts\Agent\AgentToolCall;
use Superwire\Contracts\Agent\AgentToolResult;
use Superwire\Contracts\Contracts\RuntimeToolInvokerInterface;

final class DefaultRuntimeToolInvoker implements RuntimeToolInvokerInterface
{
    public function invoke(AgentExecutionRequest $request, AgentToolCall $toolCall): AgentToolResult
    {
        foreach ($request->tools as $toolExecution) {

            if ($toolExecution->name !== $toolCall->name) {
                continue;
            }

            return new AgentToolResult(
                toolCallId: $toolCall->id,
                toolName: $toolCall->name,
                arguments: $toolCall->arguments,
                result: [
                    'status' => 'ok',
                    'tool' => $toolExecution->name,
                    'bindings' => $toolExecution->bindings,
                    'arguments' => $toolCall->arguments,
                ],
            );

        }

        return new AgentToolResult(
            toolCallId: $toolCall->id,
            toolName: $toolCall->name,
            arguments: $toolCall->arguments,
            result: "Tool `{$toolCall->name}` is not registered for this agent run",
        );
    }
}
