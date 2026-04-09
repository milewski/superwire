<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools\Data;

use Superwire\Laravel\Contracts\ToolInputData;

final readonly class WeatherAgentInput implements ToolInputData
{
    public function __construct(public ?string $city = null)
    {
    }
}
