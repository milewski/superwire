<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Workflow;

use Superwire\Laravel\Data\Workflow\Concerns\ValidatesPayload;

final class OutputFieldReference
{
    use ValidatesPayload;

    public function __construct(
        public readonly string $ref,
    )
    {
    }

    /**
     * @param array<string, mixed> $payload
     */
    public static function fromArray(array $payload): self
    {
        return new self(
            ref: self::string($payload, '$ref'),
        );
    }
}