<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Workflow;

use InvalidArgumentException;

final class AgentModel
{
    public function __construct(
        public readonly string $name,
    )
    {
    }

    public static function fromValue(mixed $value): self
    {
        if (! is_string($value)) {
            throw new InvalidArgumentException('agent model must be a string');
        }

        return new self($value);
    }
}
