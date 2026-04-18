<?php

declare(strict_types=1);

namespace Superwire\Contracts;

final class AgentForEachDefinition
{
    /**
     * @param array<string, mixed> $pattern
     */
    public function __construct(
        public readonly array $pattern,
        public readonly mixed $iterable,
    ) {
    }
}
