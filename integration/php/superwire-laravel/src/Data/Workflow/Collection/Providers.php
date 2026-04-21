<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Workflow\Collection;

use Illuminate\Support\Collection;
use Superwire\Laravel\Data\Workflow\ProviderData;

/**
 * @extends Collection<int, ProviderData>
 */
final class Providers extends Collection
{
    public function __construct(?array $items = null)
    {
        parent::__construct($items ?? []);
    }

    public static function fromArray(array $payload): self
    {
        $items = [];

        foreach ($payload as $providerPayload) {
            $items[] = ProviderData::fromArray($providerPayload);
        }

        return new self($items);
    }
}