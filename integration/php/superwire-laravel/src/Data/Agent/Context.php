<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Agent;

use InvalidArgumentException;

final class Context
{
    /**
     * @param array<string, mixed>|null $definition
     */
    public function __construct(
        public readonly ?array $definition,
    )
    {
    }

    public static function fromValue(mixed $value): self
    {
        if ($value !== null && !is_array($value)) {
            throw new InvalidArgumentException('agent context must be null or an array');
        }

        return new self($value);
    }

    public function isDefined(): bool
    {
        return $this->definition !== null;
    }
}
