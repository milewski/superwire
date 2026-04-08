<?php

namespace App\Superwire\Tools;

use App\Superwire\Tool;

final class WeatherTool extends Tool
{
    public static function name(): string
    {
        return 'weather';
    }

    public static function description(): string
    {
        return 'Fetch weather in Laravel class style';
    }

    public static function inputSchema(): array
    {
        return [
            'type' => 'object',
            'properties' => [
                'city' => [
                    'type' => ['string', 'null'],
                ],
            ],
        ];
    }

    public static function outputSchema(): array
    {
        return [
            'type' => 'object',
            'properties' => [
                'city' => ['type' => 'string'],
                'summary' => ['type' => 'string'],
                'source' => ['type' => 'string'],
            ],
            'required' => ['city', 'summary', 'source'],
        ];
    }

    public function execute(array $agentInput, array $boundInput): array
    {
        $city = $boundInput['city'] ?? ($agentInput['city'] ?? 'Madrid');
        $weatherUrl = 'https://wttr.in/' . rawurlencode((string) $city) . '?format=%C+%t';
        $weatherSummary = @file_get_contents($weatherUrl);

        if ($weatherSummary === false) {
            $weatherSummary = 'Weather service temporarily unavailable';
        }

        return [
            'city' => (string) $city,
            'summary' => trim((string) $weatherSummary),
            'source' => 'wttr.in via laravel-like weather tool',
        ];
    }
}
