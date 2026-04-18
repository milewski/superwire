<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Provider;

final class ProviderExecution
{
    /**
     * @param array<string, mixed> $config
     */
    public function __construct(
        public readonly string $name,
        public readonly string $provider,
        public readonly array $config = [],
    ) {
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
