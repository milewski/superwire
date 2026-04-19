<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Provider;

final readonly class ProviderDefinition
{
    /**
     * @param array<string, mixed> $config
     */
    public function __construct(
        public string $name,
        public string $driver,
        public array $config,
        public mixed $models = null,
    )
    {
    }
}
