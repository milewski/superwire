<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Collection;

use Illuminate\Support\Collection;
use Superwire\Laravel\Data\Agent\Agent;

/**
 * @extends Collection<int, Agent>
 */
final class Agents extends Collection
{
    public static function fromArray(array $payload): self
    {
        $items = [];

        foreach ($payload as $agentPayload) {
            $items[] = Agent::fromArray($agentPayload);
        }

        return new self($items);
    }

    public function findByName(string $name): Agent
    {
        return $this->where('name', $name)->firstOrFail();
    }
}
