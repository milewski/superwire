<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Contracts;

interface WitDefinedTool extends Tool
{
    public static function witPath(): string;
}
