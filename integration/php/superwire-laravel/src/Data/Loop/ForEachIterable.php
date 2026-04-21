<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Loop;

use Superwire\Laravel\Data\Concerns\ValidatesPayload;

final class ForEachIterable
{
    use ValidatesPayload;

    public function __construct(
        public readonly string $reference,
    )
    {
    }

    /**
     * @param array<string, mixed> $payload
     */
    public static function fromArray(array $payload): self
    {
        return new self(self::string($payload, '$ref'));
    }
}
