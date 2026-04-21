<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Prompt;

use InvalidArgumentException;

final class PromptTemplatePart
{
    public function __construct(
        public readonly ?string $text,
        public readonly ?PromptExpression $expression,
    )
    {
    }

    public static function fromValue(mixed $value): self
    {
        if (is_string($value)) {
            return new self($value, null);
        }

        if (!is_array($value) || !isset($value[ '$expr' ]) || !is_array($value[ '$expr' ])) {
            throw new InvalidArgumentException('prompt template part must be a string or $expr object');
        }

        return new self(
            text: null,
            expression: PromptExpression::fromArray($value[ '$expr' ]),
        );
    }

    public function isText(): bool
    {
        return $this->text !== null;
    }

    public function isExpression(): bool
    {
        return $this->expression !== null;
    }
}
