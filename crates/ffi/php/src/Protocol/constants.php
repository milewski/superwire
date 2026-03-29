<?php

declare(strict_types=1);

namespace EngineAi\Ffi;

const FFI_PROTOCOL_VERSION = 1;

const FFI_LIBRARY_KEY = 'engine_ai_ffi';

const FFI_OPERATION = [
    'EXECUTE_WORKFLOW' => 'execute_workflow',
    'INVOKE_TOOL' => 'invoke_tool',
    'READ_EXECUTION_VALUE' => 'read_execution_value',
];
