<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Wit\Schema;

final readonly class WitSchemaField
{
    public function __construct(
        public string $name,
        public string $witType,
        public bool $nullable,
        public ?string $description,
    )
    {
    }
}
