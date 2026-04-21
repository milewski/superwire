<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Collection;

use Illuminate\Support\Collection;
use Superwire\Laravel\Data\Agent\Schema;

/**
 * @extends Collection<int, Schema>
 */
final class Schemas extends Collection
{
    public static function fromArray(array $payload): self
    {
        $items = [];

        foreach ($payload as $schemaPayload) {
            $items[] = Schema::fromArray($schemaPayload);
        }

        return new self($items);
    }
}
