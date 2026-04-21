<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Workflow;

use Superwire\Laravel\Data\Agent\OutputFieldReference;
use Superwire\Laravel\Data\Concerns\ValidatesPayload;

final class Output
{
    use ValidatesPayload;

    /**
     * @param array<string, OutputFieldReference> $fields
     * @param array<string, mixed> $contract
     */
    public function __construct(
        public readonly array $fields,
        public readonly array $contract,
    )
    {
    }

    /**
     * @param array<string, mixed> $payload
     */
    public static function fromArray(array $payload): self
    {
        $fieldsPayload = self::array($payload, 'fields');
        $fields = [];

        foreach ($fieldsPayload as $key => $value) {
            $fields[ $key ] = OutputFieldReference::fromArray($value);
        }

        return new self(
            fields: $fields,
            contract: self::array($payload, 'contract'),
        );
    }
}
