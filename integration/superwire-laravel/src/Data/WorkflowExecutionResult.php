<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data;

final readonly class WorkflowExecutionResult
{
    /**
     * @param array<string, mixed> $output
     */
    public function __construct(public array $output)
    {
    }
}
