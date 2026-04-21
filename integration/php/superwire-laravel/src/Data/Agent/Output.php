<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Agent;

use Superwire\Laravel\Data\Concerns\ValidatesPayload;

final class Output
{
    use ValidatesPayload;

    public function __construct(
        public readonly OutputField $iteration,
        public readonly OutputField $finalOutput,
    )
    {
    }

    /**
     * @param array<string, mixed> $payload
     */
    public static function fromArray(array $payload): self
    {
        return new self(
            iteration: OutputField::fromArray(self::array($payload, 'iteration')),
            finalOutput: OutputField::fromArray(self::array($payload, 'final_output')),
        );
    }
}
