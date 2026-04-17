<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools;

use Spatie\LaravelData\Data;
use Superwire\Laravel\Contracts\ToolInputData;
use Superwire\Laravel\Tools\Attributes\Description;

#[Description('Agent-provided weather request payload.')]
final class WeatherAgentInput extends Data implements ToolInputData
{
    public function __construct(
        #[Description('Optional city requested by the model.')]
        public ?string $city = null,
    )
    {
    }
}
