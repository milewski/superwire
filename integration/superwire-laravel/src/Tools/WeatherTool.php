<?php

namespace Superwire\Laravel\Tools;

use Superwire\Laravel\Schema\JsonSchemaBuilder;

final class WeatherTool extends AbstractTool
{
    public static function name(): string
    {
        return 'weather';
    }

    public static function description(): string
    {
        return 'Fetches weather summary via wttr.in in Laravel runtime';
    }

    public static function inputSchema(): array
    {
        return JsonSchemaBuilder::object()
            ->property('city', JsonSchemaBuilder::nullableString())
            ->toArray();
    }

    public static function outputSchema(): array
    {
        return JsonSchemaBuilder::object()
            ->property('city', JsonSchemaBuilder::string())
            ->property('summary', JsonSchemaBuilder::string())
            ->property('source', JsonSchemaBuilder::string())
            ->required(['city', 'summary', 'source'])
            ->toArray();
    }

    public function execute(array $agentInput, array $boundInput): array
    {
        $cityName = $boundInput['city'] ?? ($agentInput['city'] ?? 'Madrid');
        $weatherUrl = 'https://wttr.in/' . rawurlencode((string) $cityName) . '?format=%C+%t';
        $weatherSummary = @file_get_contents($weatherUrl);

        if ($weatherSummary === false) {
            $weatherSummary = 'Weather service temporarily unavailable';
        }

        return [
            'city' => (string) $cityName,
            'summary' => trim((string) $weatherSummary),
            'source' => 'wttr.in via laravel package',
        ];
    }
}
