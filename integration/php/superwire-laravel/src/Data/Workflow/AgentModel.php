<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Workflow;

use InvalidArgumentException;

final class AgentModel
{
    public function __construct(
        public readonly ?string $name,
        public readonly ?string $reference,
    )
    {
    }

    public static function fromValue(mixed $value): self
    {
        if (is_string($value)) {
            return new self($value, null);
        }

        if (is_array($value) && isset($value[ '$ref' ]) && is_string($value[ '$ref' ])) {
            return new self(null, $value[ '$ref' ]);
        }

        throw new InvalidArgumentException('agent model must be a string or $ref object');
    }

    public function isReference(): bool
    {
        return $this->reference !== null;
    }
}
