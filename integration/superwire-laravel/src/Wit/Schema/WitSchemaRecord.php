<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Wit\Schema;

final readonly class WitSchemaRecord
{
    /**
     * @param list<WitSchemaField> $fields
     */
    public function __construct(
        public WitSchemaRecordKind $kind,
        public ?string $description,
        public array $fields,
    )
    {
    }
}
