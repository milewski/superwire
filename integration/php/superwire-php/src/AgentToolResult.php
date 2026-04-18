<?php

declare(strict_types=1);

namespace Superwire\Contracts;

final class AgentToolResult
{
    /**
     * @param array<string, mixed> $arguments
     */
    public function __construct(
        public readonly string $toolCallId,
        public readonly string $toolName,
        public readonly array $arguments,
        public readonly mixed $result,
    ) {
    }
}
