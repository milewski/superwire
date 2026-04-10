<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools\Data;

use Spatie\LaravelData\Data;

final class WeatherOutput extends Data
{
    public function __construct(
        public string $city,
        public string $summary,
        public string $source,
    ) {
    }
}
