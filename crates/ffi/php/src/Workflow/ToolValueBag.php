<?php

declare(strict_types = 1);

namespace EngineAi\Ffi;

use UnexpectedValueException;

final class ToolValueBag
{
    /**
     * @param array<string, mixed> $values
     */
    public function __construct(private readonly array $values)
    {
    }

    /**
     * @return array<string, mixed>
     */
    public function all(): array
    {
        return $this->values;
    }

    public function has(string $key): bool
    {
        return \array_key_exists($key, $this->values);
    }

    public function get(string $key, mixed $default = null): mixed
    {
        return $this->has($key) ? $this->values[ $key ] : $default;
    }

    public function string(string $key, ?string $default = null): ?string
    {
        return $this->expectType($key, 'string', $default, \is_string(...));
    }

    public function integer(string $key, ?int $default = null): ?int
    {
        return $this->expectType($key, 'integer', $default, \is_int(...));
    }

    public function number(string $key, int|float|null $default = null): int|float|null
    {
        return $this->expectType($key, 'number', $default, static fn (mixed $value): bool => \is_int($value) || \is_float($value));
    }

    public function boolean(string $key, ?bool $default = null): ?bool
    {
        return $this->expectType($key, 'boolean', $default, \is_bool(...));
    }

    public function array(string $key, ?array $default = null): ?array
    {
        return $this->expectType($key, 'array', $default, \is_array(...));
    }

    private function expectType(string $key, string $expectedType, mixed $default, callable $guard): mixed
    {
        if (!$this->has($key)) {
            return $default;
        }

        $value = $this->values[ $key ];

        if ($guard($value)) {
            return $value;
        }

        $receivedType = \get_debug_type($value);

        throw new UnexpectedValueException("Expected key `{$key}` to be {$expectedType}, got {$receivedType}.");
    }
}
