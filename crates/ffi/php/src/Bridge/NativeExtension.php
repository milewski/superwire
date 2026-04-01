<?php

declare(strict_types = 1);

namespace EngineAi\Ffi;

use RuntimeException;

final class NativeExtension
{
    private const NATIVE_FUNCTION_NAME = 'engine_ai_ffi_invoke_json';

    private function __construct()
    {
    }

    public static function invokeJson(string $requestPayload): string
    {
        if (!\function_exists(self::NATIVE_FUNCTION_NAME)) {

            throw new RuntimeException(
                'The `engine_ai_ffi` PHP extension is not loaded. Run `composer install-native` to build or install it.',
            );

        }

        $responsePayload = \call_user_func(self::NATIVE_FUNCTION_NAME, $requestPayload);

        if (!\is_string($responsePayload)) {
            throw new RuntimeException('Native extension returned a non-string JSON payload.');
        }

        return $responsePayload;
    }
}
