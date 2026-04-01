<?php

declare(strict_types = 1);

namespace EngineAi\Ffi;

final class ToolData
{
    public readonly object $input;

    public readonly object $bounded;

    public readonly ToolValueBag $context;

    private readonly ToolValueBag $inputBag;

    private readonly ToolValueBag $boundedBag;

    /**
     * @param array<string, mixed> $input
     * @param array<string, mixed> $bounded
     * @param array<string, mixed> $context
     * @param class-string|null $inputType
     * @param class-string|null $boundedType
     */
    public function __construct(
        array $input = [],
        array $bounded = [],
        array $context = [],
        ?string $inputType = null,
        ?string $boundedType = null,
    )
    {
        $this->inputBag = new ToolValueBag($input);
        $this->boundedBag = new ToolValueBag($bounded);

        $this->input = $inputType === null
            ? $this->inputBag
            : ToolPayloadHydrator::hydrate($inputType, $this->inputBag->all());

        $this->bounded = $boundedType === null
            ? $this->boundedBag
            : ToolPayloadHydrator::hydrate($boundedType, $this->boundedBag->all());

        $this->context = new ToolValueBag($context);
    }

    /**
     * @return array<string, mixed>
     */
    public function inputAll(): array
    {
        return $this->inputBag->all();
    }

    public function input(string $key, mixed $default = null): mixed
    {
        return $this->inputBag->get($key, $default);
    }

    /**
     * @return array<string, mixed>
     */
    public function boundedAll(): array
    {
        return $this->boundedBag->all();
    }

    public function bounded(string $key, mixed $default = null): mixed
    {
        return $this->boundedBag->get($key, $default);
    }

    /**
     * @return array<string, mixed>
     */
    public function contextAll(): array
    {
        return $this->context->all();
    }

    public function context(string $key, mixed $default = null): mixed
    {
        return $this->context->get($key, $default);
    }
}
