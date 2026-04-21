<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Workflow\Collection;

use Illuminate\Support\Collection;
use Superwire\Laravel\Data\Workflow\Agent;

/**
 * @extends Collection<int, Agent>
 */
final class Agents extends Collection
{
    public function __construct(?array $items = null)
    {
        parent::__construct($items ?? []);
    }

    public static function fromArray(array $payload): self
    {
        $items = [];

        foreach ($payload as $agentPayload) {
            $items[] = Agent::fromArray($agentPayload);
        }

        return new self($items);
    }
}