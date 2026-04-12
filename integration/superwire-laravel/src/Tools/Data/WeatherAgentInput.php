<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools\Data;

use Spatie\LaravelData\Data;
use Superwire\Laravel\Contracts\ToolInputData;

final class WeatherAgentInput extends Data implements ToolInputData
{
    public function __construct(public ?string $city = null)
    {
    }
}
