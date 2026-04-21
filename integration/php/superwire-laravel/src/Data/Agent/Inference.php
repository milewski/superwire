<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Agent;

use InvalidArgumentException;

final class Inference
{
    /**
     * @param array<string, mixed>|null $definition
     */
    public function __construct(
        public readonly ?array $definition,
    )
    {
    }

    public static function fromValue(mixed $value): self
    {
        if ($value !== null && !is_array($value)) {
            throw new InvalidArgumentException('agent inference must be null or an array');
        }

        return new self($value);
    }

    public function isDefined(): bool
    {
        return $this->definition !== null;
    }

    public function temperature(): int|float|null
    {
        $temperature = $this->definition[ 'temperature' ] ?? null;

        if ($temperature === null || is_int($temperature) || is_float($temperature)) {
            return $temperature;
        }

        throw new InvalidArgumentException('agent inference temperature must be a number');
    }

    public function maxTokens(): ?int
    {
        $maxTokens = $this->definition[ 'max_tokens' ] ?? null;

        if ($maxTokens === null || is_int($maxTokens)) {
            return $maxTokens;
        }

        throw new InvalidArgumentException('agent inference max_tokens must be an int');
    }

    public function topP(): int|float|null
    {
        $topP = $this->definition[ 'top_p' ] ?? null;

        if ($topP === null || is_int($topP) || is_float($topP)) {
            return $topP;
        }

        throw new InvalidArgumentException('agent inference top_p must be a number');
    }
}
