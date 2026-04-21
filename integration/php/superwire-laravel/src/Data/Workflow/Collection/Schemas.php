<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Workflow\Collection;

use Illuminate\Support\Collection;
use Superwire\Laravel\Data\Workflow\Schema;

/**
 * @extends Collection<int, Schema>
 */
final class Schemas extends Collection
{
    public function __construct(?array $items = null)
    {
        parent::__construct($items ?? []);
    }

    public static function fromArray(array $payload): self
    {
        $items = [];

        foreach ($payload as $schemaPayload) {
            $items[] = Schema::fromArray($schemaPayload);
        }

        return new self($items);
    }
}