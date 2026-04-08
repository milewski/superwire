<?php

namespace Superwire\Laravel\Contracts;

interface Tool
{
    public static function name(): string;

    public static function description(): string;

    public static function endpointName(): string;

    /**
     * @return array<string, mixed>
     */
    public static function inputSchema(): array;

    /**
     * @return array<string, mixed>
     */
    public static function boundInputSchema(): array;

    /**
     * @return array<string, mixed>
     */
    public static function outputSchema(): array;

    /**
     * @param array<string, mixed> $agentInput
     * @param array<string, mixed> $boundInput
     * @return array<string, mixed>
     */
    public function execute(array $agentInput, array $boundInput): array;
}
