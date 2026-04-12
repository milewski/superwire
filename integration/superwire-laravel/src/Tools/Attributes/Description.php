<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools\Attributes;

use Attribute;

#[Attribute(Attribute::TARGET_CLASS | Attribute::TARGET_PROPERTY | Attribute::TARGET_PARAMETER)]
final readonly class Description
{
    public function __construct(
        public string $text,
    )
    {
    }
}
