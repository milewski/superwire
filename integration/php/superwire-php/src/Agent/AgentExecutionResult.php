<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Agent;

final class AgentExecutionResult
{
    /**
     * @param array<string, mixed> $metadata
     */
    public function __construct(
        public readonly mixed $output,
        public readonly mixed $context = null,
        public readonly array $metadata = [],
    )
    {
    }
}
