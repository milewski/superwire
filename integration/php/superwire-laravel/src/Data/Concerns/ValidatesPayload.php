<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Concerns;

use InvalidArgumentException;

trait ValidatesPayload
{
    private static function string(array $payload, string $key): string
    {
        if (!isset($payload[ $key ]) || !is_string($payload[ $key ])) {
            throw new InvalidArgumentException("{$key} must be a string");
        }

        return $payload[ $key ];
    }

    private static function array(array $payload, string $key): array
    {
        if (!isset($payload[ $key ]) || !is_array($payload[ $key ])) {
            throw new InvalidArgumentException("{$key} must be an array");
        }

        return $payload[ $key ];
    }

    private static function list(array $payload, string $key): array
    {
        return $payload[ $key ] ?? [];
    }

    private static function int(array $payload, string $key): int
    {
        if (!isset($payload[ $key ]) || !is_int($payload[ $key ])) {
            throw new InvalidArgumentException("{$key} must be an int");
        }

        return $payload[ $key ];
    }
}
