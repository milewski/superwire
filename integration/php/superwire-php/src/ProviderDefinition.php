<?php

declare(strict_types=1);

namespace Superwire\Contracts;

final class ProviderDefinition
{
    /**
     * @param array<string, mixed> $config
     * @param list<string>|null $models
     */
    public function __construct(
        public readonly string $name,
        public readonly string $driver,
        public readonly array $config,
        public readonly ?array $models = null,
    ) {
    }
}
