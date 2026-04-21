<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Collection;

use Illuminate\Support\Collection;
use Superwire\Laravel\Data\Agent\Provider;

/**
 * @extends Collection<int, Provider>
 */
final class Providers extends Collection
{
    public static function fromArray(array $payload): self
    {
        $items = [];

        foreach ($payload as $providerPayload) {
            $items[] = Provider::fromArray($providerPayload);
        }

        return new self($items);
    }

    public function findByName(string $name): Provider
    {
        return $this->where('name', $name)->firstOrFail();
    }
}
