<?php

declare(strict_types=1);

namespace EngineAi\Ffi;

use RuntimeException;

final class EngineAiFfi
{
    public static function executeWorkflow(string $requestJson): string
    {
        self::assertExtensionLoaded();

        return engine_ai_execute_workflow($requestJson);
    }

    public static function registerToolCallback(string $callbackName): bool
    {
        self::assertExtensionLoaded();

        return engine_ai_register_tool_callback($callbackName);
    }

    public static function clearToolCallback(): bool
    {
        self::assertExtensionLoaded();

        return engine_ai_clear_tool_callback();
    }

    public static function moduleInfo(): string
    {
        self::assertExtensionLoaded();

        return engine_ai_module_info();
    }

    private static function assertExtensionLoaded(): void
    {
        if (!extension_loaded('engine_ai_ffi')) {
            throw new RuntimeException('The engine_ai_ffi extension is not loaded. Run `composer ffi:install`.');
        }
    }
}
