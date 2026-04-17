<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Wit\Schema;

enum WitSchemaRecordKind: string
{
    case AgentInput = 'agent-input';
    case BoundInput = 'bound-input';
    case Output = 'output';

    public function suffix(): string
    {
        return match ($this) {
            self::AgentInput => 'AgentInput',
            self::BoundInput => 'BoundInput',
            self::Output => 'Output',
        };
    }
}
