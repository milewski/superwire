<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools;

use Spatie\LaravelData\Data;
use Superwire\Laravel\Contracts\ToolBoundInputData;
use Superwire\Laravel\Tools\Attributes\Description;

#[Description('Workflow-bound weather request payload.')]
final class WeatherBoundInput extends Data implements ToolBoundInputData
{
    public function __construct(
        #[Description('Optional city provided through workflow bindings.')]
        public ?string $city = null,
    )
    {
    }
}
