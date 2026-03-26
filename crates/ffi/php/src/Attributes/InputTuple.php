<?php

declare(strict_types=1);

namespace EngineAi\Ffi\Attributes;

use Attribute;

#[Attribute(Attribute::TARGET_PROPERTY)]
final class InputTuple
{
    public function __construct(public array $itemTypes, public bool $allowAdditionalItems = false)
    {
    }
}
