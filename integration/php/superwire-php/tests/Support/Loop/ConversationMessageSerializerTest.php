<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Tests;

use PHPUnit\Framework\TestCase;
use Superwire\Contracts\Agent\AgentConversationMessage;
use Superwire\Contracts\Agent\AgentToolCall;
use Superwire\Contracts\Agent\AgentToolResult;
use Superwire\Contracts\Agent\ConversationRole;
use Superwire\Contracts\Support\Loop\ConversationMessageSerializer;

final class ConversationMessageSerializerTest extends TestCase
{
    public function testItSerializesAssistantAndToolResultMessages(): void
    {
        $conversationMessageSerializer = new ConversationMessageSerializer();
        $messages = [
            new AgentConversationMessage(ConversationRole::Assistant, [
                'content' => 'working on it',
                'tool_calls' => [
                    new AgentToolCall('call-1', 'lookup_record', [ 'record_id' => 22 ]),
                ],
            ]),
            new AgentConversationMessage(ConversationRole::ToolResult, [
                'tool_results' => [
                    new AgentToolResult('call-1', 'lookup_record', [ 'record_id' => 22 ], [ 'status' => 'ok' ]),
                ],
            ]),
        ];

        $serializedMessages = $conversationMessageSerializer->serialize($messages);

        $this->assertCount(2, $serializedMessages);
        $this->assertSame('assistant', $serializedMessages[ 0 ][ 'role' ] ?? null);
        $this->assertSame('lookup_record', $serializedMessages[ 0 ][ 'tool_calls' ][ 0 ][ 'name' ] ?? null);
        $this->assertSame('tool_result', $serializedMessages[ 1 ][ 'role' ] ?? null);
        $this->assertStringContainsString('status', $serializedMessages[ 1 ][ 'tool_results' ][ 0 ][ 'result' ] ?? '');
    }
}
