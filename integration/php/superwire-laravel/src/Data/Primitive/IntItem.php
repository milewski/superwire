<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Primitive;

use Spatie\LaravelData\Data;

final class IntItem extends Data
{
    public function __construct(
        public int $value,
    )
    {
    }
}
