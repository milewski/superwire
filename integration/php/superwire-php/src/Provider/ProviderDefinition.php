<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Provider;

final class ProviderDefinition
{
    /**
     * @param array<string, mixed> $config
     */
    public function __construct(
        public readonly string $name,
        public readonly string $driver,
        public readonly array $config,
        public readonly mixed $models = null,
    ) {
    }
}
