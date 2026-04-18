<?php

declare(strict_types=1);

namespace Superwire\Laravel\Data;

use Spatie\LaravelData\Data;

final class ResolvedProviderData extends Data
{
    /**
     * @param array<string, mixed> $config
     */
    public function __construct(
        public string $name,
        public string $provider,
        public array $config,
    ) {
    }
}
