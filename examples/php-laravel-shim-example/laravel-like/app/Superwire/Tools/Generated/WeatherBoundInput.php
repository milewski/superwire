<?php

namespace App\Superwire\Tools\Generated;

final class WeatherBoundInput
{
    public function __construct(
        public ?string $city = null,
        public ?string $internal_token = null,
    ) {
    }

    /**
     * @param array<string, mixed> $payload
     */
    public static function fromPayload(array $payload): self
    {
        $cityValue = $payload['city'] ?? null;
        $internalTokenValue = $payload['internal_token'] ?? ($payload['internal-token'] ?? null);

        return new self(
            city: is_string($cityValue) ? $cityValue : null,
            internal_token: is_string($internalTokenValue) ? $internalTokenValue : null,
        );
    }

    /**
     * @return array<string, mixed>
     */
    public function toPayload(): array
    {
        return [
            'city' => $this->city,
            'internal_token' => $this->internal_token,
        ];
    }
}
