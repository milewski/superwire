<?php

namespace App\Superwire\Tools\Generated;

final class WeatherOutput
{
    public function __construct(
        public string $city,
        public string $summary,
        public string $source,
    ) {
    }

    /**
     * @param array<string, mixed> $payload
     */
    public static function fromPayload(array $payload): self
    {
        $cityValue = $payload['city'] ?? '';
        $summaryValue = $payload['summary'] ?? '';
        $sourceValue = $payload['source'] ?? '';

        return new self(
            city: is_string($cityValue) ? $cityValue : '',
            summary: is_string($summaryValue) ? $summaryValue : '',
            source: is_string($sourceValue) ? $sourceValue : '',
        );
    }

    /**
     * @return array<string, mixed>
     */
    public function toPayload(): array
    {
        return [
            'city' => $this->city,
            'summary' => $this->summary,
            'source' => $this->source,
        ];
    }
}
