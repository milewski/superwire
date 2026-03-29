<?php

declare(strict_types = 1);

namespace EngineAi\Ffi\Attributes;

use Attribute;

#[Attribute(Attribute::TARGET_CLASS | Attribute::TARGET_PROPERTY)]
final class Description
{
    public function __construct(
        public readonly string $value,
    )
    {
    }
}
