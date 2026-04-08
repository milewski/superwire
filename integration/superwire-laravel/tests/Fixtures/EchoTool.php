<?php

namespace Superwire\Laravel\Tests\Fixtures;

use Superwire\Laravel\Tools\AbstractTool;

final class EchoTool extends AbstractTool
{
    public static function description(): string
    {
        return 'Echoes agent and bound input payloads';
    }

    public function execute(array $agentInput, array $boundInput): array
    {
        return [
            'agent_input' => $agentInput,
            'bound_input' => $boundInput,
        ];
    }
}
