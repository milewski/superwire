<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools\Data;

use Superwire\Laravel\Contracts\ToolOutputData;

final readonly class WeatherOutput implements ToolOutputData
{
    public function __construct(
        public string $city,
        public string $summary,
        public string $source,
    ) {
    }
}
