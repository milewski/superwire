<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Agent;

enum ConversationRole: string
{
    case User = 'user';
    case Assistant = 'assistant';
    case ToolResult = 'tool_result';
}
