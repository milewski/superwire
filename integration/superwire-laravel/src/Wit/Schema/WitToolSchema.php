<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Wit\Schema;

use RuntimeException;

final readonly class WitToolSchema
{
    /**
     * @param array<string, WitSchemaRecord> $records
     */
    public function __construct(
        public string $toolName,
        public string $toolDescription,
        public array $records,
    )
    {
    }

    public function record(WitSchemaRecordKind $recordKind): WitSchemaRecord
    {
        if (!isset($this->records[ $recordKind->value ])) {
            throw new RuntimeException(sprintf('missing `%s` record in WIT schema', $recordKind->value));
        }

        return $this->records[ $recordKind->value ];
    }
}
