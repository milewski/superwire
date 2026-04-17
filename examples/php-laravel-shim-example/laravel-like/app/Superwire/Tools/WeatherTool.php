<?php

namespace App\Superwire\Tools;

use App\Superwire\Tool;
use App\Superwire\Tools\Generated\WeatherAgentInput;
use App\Superwire\Tools\Generated\WeatherBoundInput;
use App\Superwire\Tools\Generated\WeatherOutput;

final class WeatherTool extends Tool
{
    public static function witPath(): string
    {
        return __DIR__ . '/weather.wit';
    }

    public function execute(object $agentInput, object $boundInput): object
    {
        if (!$agentInput instanceof WeatherAgentInput || !$boundInput instanceof WeatherBoundInput) {
            throw new \RuntimeException('invalid_tool_input_types');
        }

        $cityName = $boundInput->city ?? ($agentInput->city ?? 'Madrid');
        $weatherUrl = 'https://wttr.in/' . rawurlencode((string) $cityName) . '?format=%C+%t';
        $weatherSummary = @file_get_contents($weatherUrl);

        if ($weatherSummary === false) {
            $weatherSummary = 'Weather service temporarily unavailable';
        }

        return new WeatherOutput(
            city: (string) $cityName,
            summary: trim((string) $weatherSummary),
            source: 'wttr.in via laravel-like weather tool',
        );
    }
}
