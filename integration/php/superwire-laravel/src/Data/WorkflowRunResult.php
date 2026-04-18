<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data;

final class WorkflowRunResult
{
    /**
     * @param array<string, mixed> $context
     */
    public function __construct(
        public readonly mixed $output,
        public readonly array $context,
    ) {
    }
}
