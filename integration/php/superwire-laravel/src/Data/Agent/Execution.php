<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Agent;

use Superwire\Laravel\Data\Concerns\ValidatesPayload;

final class Execution
{
    use ValidatesPayload;

    /**
     * @param list<string> $order
     * @param list<list<string>> $batches
     * @param list<array<string, mixed>> $edges
     */
    public function __construct(
        public readonly array $order,
        public readonly array $batches,
        public readonly array $edges,
    )
    {
    }

    /**
     * @param array<string, mixed> $payload
     */
    public static function fromArray(array $payload): self
    {
        return new self(
            order: self::list($payload, 'order'),
            batches: self::list($payload, 'batches'),
            edges: self::list($payload, 'edges'),
        );
    }
}
