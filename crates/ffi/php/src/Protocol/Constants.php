<?php

declare(strict_types=1);

namespace EngineAi\Ffi;

final class Constants
{
    public const FFI_PROTOCOL_VERSION = \EngineAi\Ffi\FFI_PROTOCOL_VERSION;

    public const FFI_LIBRARY_KEY = \EngineAi\Ffi\FFI_LIBRARY_KEY;

    public const FFI_OPERATION = [
        'EXECUTE_WORKFLOW' => FfiOperation::EXECUTE_WORKFLOW,
        'INVOKE_TOOL' => FfiOperation::INVOKE_TOOL,
        'READ_EXECUTION_VALUE' => FfiOperation::READ_EXECUTION_VALUE,
    ];
}
