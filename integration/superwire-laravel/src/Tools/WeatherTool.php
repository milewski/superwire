<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools;

use Illuminate\Support\Facades\Http;
use Superwire\Laravel\Tools\Data\WeatherAgentInput;
use Superwire\Laravel\Tools\Data\WeatherBoundInput;
use Superwire\Laravel\Tools\Data\WeatherOutput;
use Throwable;

final class WeatherTool extends AbstractWitTool
{
    public static function witPath(): string
    {
        return __DIR__ . '/weather.wit';
    }

    protected function handle(WeatherAgentInput $agentInput, WeatherBoundInput $boundInput): WeatherOutput
    {
        $cityName = $boundInput->city ?? ($agentInput->city ?? 'Madrid');
        $weatherUrl = 'https://wttr.in/' . rawurlencode((string) $cityName) . '?format=%C+%t';

        try {

            $weatherResponse = Http::timeout(10)->get($weatherUrl);

            if ($weatherResponse->successful()) {

                $weatherSummary = $weatherResponse->body();

            } else {

                $weatherSummary = 'Weather service temporarily unavailable';

            }

        } catch (Throwable $throwable) {

            report($throwable);
            $weatherSummary = 'Weather service temporarily unavailable';

        }

        return new WeatherOutput(
            city: (string) $cityName,
            summary: trim((string) $weatherSummary),
            source: 'wttr.in via laravel package',
        );
    }
}
