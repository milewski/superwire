<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Workflow;

use InvalidArgumentException;

final class AgentInference
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
        if ($value !== null && ! is_array($value)) {
            throw new InvalidArgumentException('agent inference must be null or an array');
        }

        return new self($value);
    }

    public function isDefined(): bool
    {
        return $this->definition !== null;
    }
}
