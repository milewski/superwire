<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools;

use Spatie\LaravelData\Data;
use Superwire\Laravel\Tools\Attributes\Description;

#[Description('Weather response payload.')]
final class WeatherOutput extends Data
{
    public function __construct(
        #[Description('City resolved by the tool.')]
        public string $city,

        #[Description('Weather summary returned from wttr.in.')]
        public string $summary,

        #[Description('Source attribution.')]
        public string $source,
    )
    {
    }
}
