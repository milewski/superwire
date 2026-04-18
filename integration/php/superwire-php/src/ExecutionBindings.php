<?php

declare(strict_types=1);

namespace Superwire\Contracts;

final class ExecutionBindings
{
    /**
     * @param array<string, mixed> $values
     */
    public function __construct(
        public readonly array $values,
    ) {
    }

    public function value(string $key, mixed $default = null): mixed
    {
        if (array_key_exists($key, $this->values)) {
            return $this->values[$key];
        }

        return $default;
    }
}
