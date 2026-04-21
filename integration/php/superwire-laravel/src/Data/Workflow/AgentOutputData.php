<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Workflow;

use Superwire\Laravel\Data\Workflow\Concerns\ValidatesPayload;

final class AgentOutputData
{
    use ValidatesPayload;

    public function __construct(
        public readonly OutputFieldData $iteration,
        public readonly OutputFieldData $finalOutput,
    )
    {
    }

    /**
     * @param array<string, mixed> $payload
     */
    public static function fromArray(array $payload): self
    {
        return new self(
            iteration: OutputFieldData::fromArray(self::array($payload, 'iteration')),
            finalOutput: OutputFieldData::fromArray(self::array($payload, 'final_output')),
        );
    }
}