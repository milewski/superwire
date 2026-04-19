<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Support\Loop;

use Superwire\Contracts\Agent\AgentConversationMessage;
use Superwire\Contracts\Agent\AgentToolCall;
use Superwire\Contracts\Agent\AgentToolResult;
use Superwire\Contracts\Agent\ConversationRole;

final class ConversationMessageSerializer
{
    /**
     * @param array<int, AgentConversationMessage> $messages
     * @return list<array{role: string, content?: string, tool_calls?: list<array{name: string, arguments: string}>, tool_results?: list<array{tool_call_id: string, result: string}>}>
     */
    public function serialize(array $messages): array
    {
        return array_map(
            function (AgentConversationMessage $message): array {

                $payload = $message->payload;
                $role = $message->role;

                if ($role === ConversationRole::ToolResult) {

                    return [
                        'role' => ConversationRole::ToolResult->value,
                        'tool_results' => array_map(
                            function (AgentToolResult $toolResult): array {

                                return [
                                    'tool_call_id' => $toolResult->toolCallId,
                                    'result' => is_string($toolResult->result) ? $toolResult->result : json_encode($toolResult->result),
                                ];

                            },
                            $payload[ 'tool_results' ] ?? [],
                        ),
                    ];

                }

                $content = $payload[ 'content' ] ?? '';

                if (($payload[ 'tool_calls' ] ?? []) !== []) {

                    return [
                        'role' => $role->value,
                        'content' => $content,
                        'tool_calls' => array_map(
                            static fn (AgentToolCall $toolCall): array => [
                                'name' => $toolCall->name,
                                'arguments' => is_array($toolCall->arguments) ? json_encode($toolCall->arguments) : $toolCall->arguments,
                            ],
                            $payload[ 'tool_calls' ],
                        ),
                    ];

                }

                return [ 'role' => $role->value, 'content' => $content ];

            },
            $messages,
        );
    }
}
