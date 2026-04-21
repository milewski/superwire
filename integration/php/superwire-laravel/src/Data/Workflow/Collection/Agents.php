<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Workflow\Collection;

use Illuminate\Support\Collection;
use Superwire\Laravel\Data\Workflow\AgentData;

/**
 * @extends Collection<int, AgentData>
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
            $items[] = AgentData::fromArray($agentPayload);
        }

        return new self($items);
    }
}