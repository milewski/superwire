<?php

namespace App\Superwire;

abstract class Tool
{
    abstract public static function name(): string;

    abstract public static function description(): string;

    abstract public static function inputSchema(): array;

    abstract public static function outputSchema(): array;

    public static function boundInputSchema(): array
    {
        return [
            'type' => 'object',
        ];
    }

    abstract public function execute(array $agentInput, array $boundInput): array;
}
