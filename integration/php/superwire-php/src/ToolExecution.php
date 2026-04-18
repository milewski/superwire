<?php

declare(strict_types=1);

namespace Superwire\Contracts;

final class ToolExecution
{
    /**
     * @param array<string, mixed> $bindings
     */
    public function __construct(
        public readonly string $name,
        public readonly array $bindings,
    ) {
    }
}
