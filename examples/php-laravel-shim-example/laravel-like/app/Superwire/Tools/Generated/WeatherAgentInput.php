<?php

namespace App\Superwire\Tools\Generated;

final class WeatherAgentInput
{
    public function __construct(
        public ?string $city = null,
    ) {
    }

    /**
     * @param array<string, mixed> $payload
     */
    public static function fromPayload(array $payload): self
    {
        $cityValue = $payload['city'] ?? null;

        return new self(
            city: is_string($cityValue) ? $cityValue : null,
        );
    }

    /**
     * @return array<string, mixed>
     */
    public function toPayload(): array
    {
        return [
            'city' => $this->city,
        ];
    }
}
