<?php

namespace Superwire\Laravel\Data;

final readonly class ToolInvocationResponse
{
    /**
     * @param array<string, mixed> $output
     */
    public function __construct(public array $output)
    {
    }
}
