<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Wit\Schema;

use RuntimeException;

final readonly class WitToolSchema
{
    /**
     * @param array<string, WitSchemaRecord> $records
     * @param array<string, WitSchemaRecord> $namedRecords
     * @param array<string, WitSchemaEnum> $namedEnums
     * @param array<string, WitSchemaVariant> $namedVariants
     */
    public function __construct(
        public string $toolName,
        public string $toolDescription,
        public array $records,
        public array $namedRecords,
        public array $namedEnums,
        public array $namedVariants,
    )
    {
    }

    public function hasRecord(WitSchemaRecordKind $recordKind): bool
    {
        return isset($this->records[ $recordKind->value ]);
    }

    public function record(WitSchemaRecordKind $recordKind): WitSchemaRecord
    {
        if (!isset($this->records[ $recordKind->value ])) {
            throw new RuntimeException(sprintf('missing `%s` record in WIT schema', $recordKind->value));
        }

        return $this->records[ $recordKind->value ];
    }

    public function hasNamedRecord(string $recordName): bool
    {
        return isset($this->namedRecords[ $recordName ]);
    }

    public function namedRecord(string $recordName): WitSchemaRecord
    {
        if (!isset($this->namedRecords[ $recordName ])) {
            throw new RuntimeException(sprintf('missing named `%s` record in WIT schema', $recordName));
        }

        return $this->namedRecords[ $recordName ];
    }

    public function hasNamedEnum(string $enumName): bool
    {
        return isset($this->namedEnums[ $enumName ]);
    }

    public function hasNamedVariant(string $variantName): bool
    {
        return isset($this->namedVariants[ $variantName ]);
    }
}
