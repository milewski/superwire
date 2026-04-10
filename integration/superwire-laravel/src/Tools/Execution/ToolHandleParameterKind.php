<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools\Execution;

enum ToolHandleParameterKind: string
{
    case AgentInput = 'agent_input';
    case BoundInput = 'bound_input';
    case Container = 'container';
}
