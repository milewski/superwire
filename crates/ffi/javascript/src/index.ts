export { FFI_LIBRARY_KEY, FFI_OPERATION, FFI_PROTOCOL_VERSION } from './Protocol/constants'
export type { FfiOperation } from './Protocol/constants'

export { createEngineFfiBridge, EngineFfiBridge } from './Bridge'

export { Engine } from './Engine'
export type { EngineOptions, RegisterToolOptions } from './Engine'

export { schema } from './Schema/schema'
export type {
    JsonSchema,
    JsonSchemaArrayOptions,
    JsonSchemaObjectOptions,
    JsonSchemaObjectProperties,
    JsonSchemaPrimitive,
} from './Schema/schema'

export { Tool, Workflow, ToolData, ToolValueBag, ToolOutputNormalizer } from './Workflow'
export type { ToolArguments, ToolExecutionContext } from './Workflow'

export type {
    CustomToolDeclaration,
    EngineExecutionResult,
    EngineFfiBridgeOptions,
    EngineRunOptions,
    ExecutionValueName,
    FfiBoundaryEnvelope,
    FfiBoundaryError,
    FfiBoundaryStatus,
    FfiInvokeRequestEnvelope,
    FfiResponseEnvelope,
    JsonRecord,
    ReadExecutionValueEnvelope,
    ReadExecutionValueRequest,
    RequestOptions,
    WorkflowExecutionEnvelope,
    WorkflowExecutionFailedEnvelope,
    WorkflowExecutionRequest,
    WorkflowExecutionSucceededEnvelope,
    WorkflowOptions,
} from './types'
