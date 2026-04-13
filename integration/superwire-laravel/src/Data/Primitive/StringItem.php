<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Primitive;

use Spatie\LaravelData\Data;

final class StringItem extends Data
{
    public function __construct(
        public string $value,
    )
    {
    }
}
