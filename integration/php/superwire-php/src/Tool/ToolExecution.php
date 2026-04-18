<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Tool;

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
