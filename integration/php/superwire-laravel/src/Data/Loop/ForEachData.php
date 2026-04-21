<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Loop;

use InvalidArgumentException;
use Superwire\Laravel\Data\Concerns\ValidatesPayload;

final class ForEachData
{
    use ValidatesPayload;

    public function __construct(
        public readonly ForEachPattern $pattern,
        public readonly ForEachIterable $iterable,
    )
    {
    }

    /**
     * @param array<string, mixed> $payload
     */
    public static function fromArray(array $payload): self
    {
        return new self(
            pattern: ForEachPattern::fromArray(self::array($payload, 'pattern')),
            iterable: ForEachIterable::fromArray(self::array($payload, 'iterable')),
        );
    }

    public static function fromValue(mixed $value): ?self
    {
        if ($value === null) {
            return null;
        }

        if (!is_array($value)) {
            throw new InvalidArgumentException('agent for_each must be null or an array');
        }

        return self::fromArray($value);
    }
}
