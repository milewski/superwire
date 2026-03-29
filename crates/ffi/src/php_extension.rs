use ext_php_rs::prelude::*;

use crate::types::FfiOperation;
use crate::{invoke_ffi_json_payload, FFI_PROTOCOL_VERSION};

#[php_function]
pub fn engine_ai_ffi_invoke_json(request_payload: String) -> String {
    invoke_ffi_json_payload(&request_payload)
}

#[php_module]
#[php(name = "engine_ai_ffi")]
pub fn get_module(module: ModuleBuilder) -> ModuleBuilder {
    module
        .name("engine_ai_ffi")
        .function(wrap_function!(engine_ai_ffi_invoke_json))
        .constant(("ENGINE_AI_FFI_PROTOCOL_VERSION", i64::from(FFI_PROTOCOL_VERSION), &[]))
        .constant((
            "ENGINE_AI_FFI_OPERATION_EXECUTE_WORKFLOW",
            FfiOperation::ExecuteWorkflow.as_str(),
            &[],
        ))
        .constant(("ENGINE_AI_FFI_OPERATION_INVOKE_TOOL", FfiOperation::InvokeTool.as_str(), &[]))
        .constant((
            "ENGINE_AI_FFI_OPERATION_READ_EXECUTION_VALUE",
            FfiOperation::ReadExecutionValue.as_str(),
            &[],
        ))
}
