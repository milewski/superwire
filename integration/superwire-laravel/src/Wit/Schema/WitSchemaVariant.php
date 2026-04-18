<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Wit\Schema;

final readonly class WitSchemaVariant
{
    /**
     * @param list<WitSchemaVariantCase> $cases
     */
    public function __construct(
        public string $name,
        public ?string $description,
        public array $cases,
    )
    {
    }
}
