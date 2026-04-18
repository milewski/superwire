<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Wit\Schema;

final readonly class WitSchemaEnum
{
    /**
     * @param list<string> $cases
     */
    public function __construct(
        public string $name,
        public ?string $description,
        public array $cases,
    )
    {
    }
}
