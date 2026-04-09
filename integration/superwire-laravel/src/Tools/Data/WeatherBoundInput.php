<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools\Data;

use Superwire\Laravel\Contracts\ToolBoundInputData;

final readonly class WeatherBoundInput implements ToolBoundInputData
{
    public function __construct(public ?string $city = null)
    {
    }
}
