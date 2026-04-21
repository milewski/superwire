<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Prompt;

use InvalidArgumentException;
use Superwire\Laravel\Data\Concerns\ValidatesPayload;

final class PromptExpression
{
    use ValidatesPayload;

    public function __construct(
        public readonly string $reference,
    )
    {
    }

    /**
     * @param array<string, mixed> $payload
     */
    public static function fromArray(array $payload): self
    {
        return new self(self::string($payload, '$ref'));
    }

    public static function fromValue(mixed $value): self
    {
        if (!is_array($value)) {
            throw new InvalidArgumentException('prompt expression must be an array');
        }

        return self::fromArray($value);
    }
}
