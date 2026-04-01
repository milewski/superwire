<?php

declare(strict_types = 1);

namespace EngineAi\Ffi;

use BackedEnum;
use JsonSerializable;
use UnitEnum;

final class ToolOutputNormalizer
{
    public static function normalize(mixed $value): mixed
    {
        if ($value === null || is_scalar($value)) {
            return $value;
        }

        if ($value instanceof UnitEnum) {
            return $value instanceof BackedEnum ? $value->value : $value->name;
        }

        if (is_array($value)) {

            $normalized = [];

            foreach ($value as $key => $item) {
                $normalized[ $key ] = self::normalize($item);
            }

            return $normalized;

        }

        if (is_object($value)) {

            if (method_exists($value, 'toArray')) {

                $arrayValue = $value->toArray();

                if (is_array($arrayValue)) {
                    return self::normalize($arrayValue);
                }

            }

            if ($value instanceof JsonSerializable) {
                return self::normalize($value->jsonSerialize());
            }

            return self::normalize(get_object_vars($value));

        }

        return $value;
    }
}
