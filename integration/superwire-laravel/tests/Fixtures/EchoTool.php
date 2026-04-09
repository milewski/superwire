<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests\Fixtures;

use Superwire\Laravel\Contracts\ToolBoundInputData;
use Superwire\Laravel\Contracts\ToolInputData;
use Superwire\Laravel\Contracts\ToolOutputData;
use Superwire\Laravel\Tools\AbstractTool;

final class EchoTool extends AbstractTool
{
    public static function description(): string
    {
        return 'Echoes agent and bound input payloads';
    }

    protected function handle(EchoToolAgentInput $agentInput, EchoToolBoundInput $boundInput): EchoToolOutput
    {
        return new EchoToolOutput(
            agent_input: [
                'city' => $agentInput->city,
            ],
            bound_input: [
                'units' => $boundInput->units,
            ],
        );
    }
}

final readonly class EchoToolAgentInput implements ToolInputData
{
    public function __construct(public ?string $city = null)
    {
    }
}

final readonly class EchoToolBoundInput implements ToolBoundInputData
{
    public function __construct(public ?string $units = null)
    {
    }
}

final readonly class EchoToolOutput implements ToolOutputData
{
    /**
     * @param array<string, mixed> $agent_input
     * @param array<string, mixed> $bound_input
     */
    public function __construct(
        public array $agent_input,
        public array $bound_input,
    ) {
    }
}
