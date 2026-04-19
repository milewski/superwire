<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Provider;

final readonly class ProviderExecution
{
    /**
     * @param array<string, mixed> $config
     */
    public function __construct(
        public string $name,
        public string $provider,
        public array $config = [],
    )
    {
    }

    public function hasConfig(string $key): bool
    {
        return array_key_exists($key, $this->config);
    }

    public function configValue(string $key, mixed $default = null): mixed
    {
        if ($this->hasConfig($key)) {
            return $this->config[ $key ];
        }

        return $default;
    }
}
