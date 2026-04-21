<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Prompt;

use InvalidArgumentException;

final readonly class Prompt
{
    /**
     * @param list<PromptTemplatePart> $templateParts
     */
    public function __construct(
        public ?string $text,
        public array $templateParts,
    )
    {
    }

    public static function fromValue(mixed $value): self
    {
        if (is_string($value)) {
            return new self($value, []);
        }

        if (!is_array($value)) {
            throw new InvalidArgumentException('agent prompt must be a string or template array');
        }

        $templatePayload = $value[ '$template' ] ?? null;

        if (!is_array($templatePayload)) {
            throw new InvalidArgumentException('agent prompt template must contain a $template array');
        }

        $templateParts = [];

        foreach ($templatePayload as $templatePart) {
            $templateParts[] = PromptTemplatePart::fromValue($templatePart);
        }

        return new self(
            text: null,
            templateParts: $templateParts,
        );
    }

    public function isText(): bool
    {
        return $this->text !== null;
    }
}
