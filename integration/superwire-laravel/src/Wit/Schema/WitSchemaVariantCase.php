<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Wit\Schema;

final readonly class WitSchemaVariantCase
{
    public function __construct(
        public string $name,
        public ?string $payloadType,
        public ?string $description,
    )
    {
    }
}
