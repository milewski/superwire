<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Agent;

use Superwire\Laravel\Data\Concerns\ValidatesPayload;

final class Schema
{
    use ValidatesPayload;

    /**
     * @param list<array<string, mixed>> $fields
     */
    public function __construct(
        public readonly string $name,
        public readonly array $fields,
    )
    {
    }

    /**
     * @param array<string, mixed> $payload
     */
    public static function fromArray(array $payload): self
    {
        return new self(
            name: self::string($payload, 'name'),
            fields: self::list($payload, 'fields'),
        );
    }
}
