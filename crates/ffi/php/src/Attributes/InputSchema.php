<?php

declare(strict_types=1);

namespace EngineAi\Ffi\Attributes;

use Attribute;

#[Attribute(Attribute::TARGET_PROPERTY)]
final class InputSchema
{
    public function __construct(public array $schema)
    {
    }
}
