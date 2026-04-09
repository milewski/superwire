<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Contracts;

use Swaggest\JsonSchema\Schema;

interface Tool
{
    public static function name(): string;

    public static function description(): string;

    public static function endpointName(): string;

    /**
     * @return class-string<ToolInputData>
     */
    public static function agentInputClass(): string;

    /**
     * @return class-string<ToolBoundInputData>
     */
    public static function boundInputClass(): string;

    /**
     * @return class-string<ToolOutputData>
     */
    public static function outputClass(): string;

    public static function inputSchema(): Schema;

    public static function boundInputSchema(): Schema;

    public static function outputSchema(): Schema;

    /**
     * @param array<string, mixed> $agentInput
     * @param array<string, mixed> $boundInput
     * @return array<string, mixed>
     */
    public function execute(array $agentInput, array $boundInput): array;
}
