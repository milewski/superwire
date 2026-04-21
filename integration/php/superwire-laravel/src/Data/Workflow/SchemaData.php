<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Workflow;

use Superwire\Laravel\Data\Workflow\Concerns\ValidatesPayload;

final class SchemaData
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