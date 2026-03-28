import type { FfiOperation } from './constants'

export type JsonRecord = Record<string, unknown>;

export type FfiBoundaryStatus = 'succeeded' | 'failed';

export interface FfiBoundaryError {
    code: string;
    message: string;
}

export interface FfiResponseEnvelope<Payload = unknown> {
    protocol_version: number;
    request_id?: string;
    operation: FfiOperation;
    payload: Payload;
}

export interface FfiBoundaryEnvelope<Payload = unknown> {
    status: FfiBoundaryStatus;
    response?: FfiResponseEnvelope<Payload>;
    error?: FfiBoundaryError;
}

export interface EngineFfiBridgeOptions {
    libraryPath?: string;
}

export interface RequestOptions {
    requestId?: string;
}

export interface FfiInvokeRequestEnvelope<Payload = unknown> {
    protocol_version: number;
    request_id?: string;
    operation: FfiOperation;
    payload: Payload;
}

export interface CustomToolDeclaration {
    name: string;
    description?: string;
    input_schema: unknown;
    output_schema?: unknown;
}

export interface WorkflowExecutionRequest<Input extends JsonRecord = JsonRecord> {
    execution_id: string;
    workflow_source: string;
    input: {
        payload: Input;
    };
    secrets?: {
        payload: JsonRecord;
    };
    custom_tools: CustomToolDeclaration[];
    tool_callback?: ToolCallbackConfig;
    defer_output?: boolean;
}

export interface ToolCallbackConfig {
    endpoint: string;
    auth_token?: string;
}

export interface ToolInvocationPayload {
    execution_id: string;
    invocation_id: string;
    tool_name: string;
    arguments: unknown;
    execution_context?: unknown;
}

export interface ToolInvocationResult {
    execution_id: string;
    invocation_id: string;
    output: unknown;
}

export interface ToolInvocationError {
    code: string;
    message: string;
    details?: unknown;
}

export type ToolInvocationEnvelope =
    | { status: 'succeeded'; result: ToolInvocationResult }
    | { status: 'failed'; error: ToolInvocationError };

export interface WorkflowExecutionSucceededEnvelope<Output = unknown> {
    status: 'succeeded';
    output: {
        execution_id: string;
        output?: Output;
    };
}

export interface WorkflowExecutionFailedEnvelope {
    status: 'failed';
    error: {
        code: string;
        message: string;
        context?: unknown;
        details?: unknown;
    };
}

export type WorkflowExecutionEnvelope<Output = unknown> =
    | WorkflowExecutionSucceededEnvelope<Output>
    | WorkflowExecutionFailedEnvelope;

export interface WorkflowOptions {
    customTools?: CustomToolDeclaration[];
}

export interface EngineRunOptions {
    requestId?: string;
    executionId?: string;
}

export type ExecutionValueName = 'success' | 'error' | 'context';

export interface ReadExecutionValueRequest {
    execution_id: string;
    value: ExecutionValueName;
}

export interface ReadExecutionValueSucceededEnvelope {
    status: 'succeeded';
    result: {
        execution_id: string;
        value: unknown;
    };
}

export interface ReadExecutionValueFailedEnvelope {
    status: 'failed';
    error: {
        code: string;
        message: string;
        details?: unknown;
    };
}

export type ReadExecutionValueEnvelope =
    | ReadExecutionValueSucceededEnvelope
    | ReadExecutionValueFailedEnvelope;

export interface EngineExecutionError {
    readonly code: string;
    readonly message: string;
    readonly context?: unknown;
    readonly details?: unknown;
}

export interface EngineExecutionResult<Output = unknown> {
    readonly executionId: string;

    isSuccess(): Promise<boolean>;

    isError(): Promise<boolean>;

    success(): Promise<Output | null>;

    error(): Promise<EngineExecutionError | null>;

    context(): Promise<unknown>;
}
