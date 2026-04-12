<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools\Data;

use Spatie\LaravelData\Data;
use Superwire\Laravel\Contracts\ToolBoundInputData;

final class WeatherBoundInput extends Data implements ToolBoundInputData
{
    public function __construct(public ?string $city = null)
    {
    }
}
