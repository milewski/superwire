<?php

declare(strict_types=1);

namespace EngineAi\Ffi;

final class Schema
{
    public static function string(): array
    {
        return ['type' => 'string'];
    }

    public static function number(): array
    {
        return ['type' => 'number'];
    }

    public static function boolean(): array
    {
        return ['type' => 'boolean'];
    }

    public static function integer(): array
    {
        return ['type' => 'integer'];
    }

    public static function null(): array
    {
        return ['type' => 'null'];
    }

    public static function literal(string|int|float|bool|null $value): array
    {
        return ['const' => $value];
    }

    public static function enumeration(array $values): array
    {
        return ['enum' => $values];
    }

    public static function array(array $items, array $options = []): array
    {
        $arraySchema = [
            'type' => 'array',
            'items' => $items,
        ];

        if (\array_key_exists('minItems', $options)) {
            $arraySchema['minItems'] = $options['minItems'];
        }

        if (\array_key_exists('maxItems', $options)) {
            $arraySchema['maxItems'] = $options['maxItems'];
        }

        return $arraySchema;
    }

    public static function fixedArray(array $items, int $size): array
    {
        return self::array($items, [
            'minItems' => $size,
            'maxItems' => $size,
        ]);
    }

    public static function tuple(array $items): array
    {
        return [
            'type' => 'array',
            'prefixItems' => $items,
            'minItems' => \count($items),
            'maxItems' => \count($items),
        ];
    }

    public static function union(array $variants): array
    {
        return ['anyOf' => $variants];
    }

    public static function nullable(array $inner): array
    {
        return self::union([$inner, self::null()]);
    }

    public static function object(array $properties, array|null $requiredOrOptions = null): array
    {
        if ($requiredOrOptions === null) {
            $objectOptions = [
                'required' => \array_keys($properties),
            ];
        } elseif (self::isListOfStrings($requiredOrOptions)) {
            $objectOptions = [
                'required' => $requiredOrOptions,
            ];
        } else {
            $objectOptions = $requiredOrOptions;
        }

        return [
            'type' => 'object',
            'properties' => $properties,
            'required' => $objectOptions['required'] ?? \array_keys($properties),
            'additionalProperties' => $objectOptions['additionalProperties'] ?? false,
        ];
    }

    private static function isListOfStrings(array $value): bool
    {
        if ($value === []) {
            return true;
        }

        if (!\array_is_list($value)) {
            return false;
        }

        foreach ($value as $itemValue) {
            if (!\is_string($itemValue)) {
                return false;
            }
        }

        return true;
    }
}
