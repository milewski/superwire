export const FFI_PROTOCOL_VERSION = 1

export const FFI_OPERATION = Object.freeze({
    EXECUTE_WORKFLOW: 'execute_workflow',
    INVOKE_TOOL: 'invoke_tool',
    READ_EXECUTION_VALUE: 'read_execution_value',
} as const)

export type FfiOperation = (typeof FFI_OPERATION)[keyof typeof FFI_OPERATION];

export const FFI_LIBRARY_KEY = 'engine_ai_ffi'
