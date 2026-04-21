<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Agent;

use Superwire\Laravel\Data\Concerns\ValidatesPayload;

final class Provider
{
    use ValidatesPayload;

    /**
     * @param list<string> $models
     * @param array<string, mixed> $config
     */
    public function __construct(
        public readonly string $name,
        public readonly string $driver,
        public readonly array $models,
        public readonly array $config,
    )
    {
    }

    /**
     * @param array<string, mixed> $payload
     */
    public static function fromArray(array $payload): self
    {
        return new self(
            name: self::string($payload, 'name'),
            driver: self::string($payload, 'driver'),
            models: self::list($payload, 'models'),
            config: self::array($payload, 'config'),
        );
    }
}
