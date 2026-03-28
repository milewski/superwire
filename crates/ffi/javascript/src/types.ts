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
    custom_tools: CustomToolDeclaration[];
}

export interface WorkflowExecutionSucceededEnvelope<Output = unknown> {
    status: 'succeeded';
    output: {
        execution_id: string;
        output: Output;
    };
}

export interface WorkflowExecutionFailedEnvelope {
    status: 'failed';
    error: {
        code: string;
        message: string;
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

export interface EngineRunSuccess<Output = unknown> {
    readonly kind: 'success';
    readonly success: Output;
    isSuccess(): this is EngineRunSuccess<Output>;
    isError(): this is EngineRunError<Output>;
}

export interface EngineRunError<Output = never> {
    readonly kind: 'error';
    readonly error: Error;
    isSuccess(): this is EngineRunSuccess<Output>;
    isError(): this is EngineRunError<Output>;
}

export type EngineRunResult<Output = unknown> = EngineRunSuccess<Output> | EngineRunError<Output>;

export function createEngineRunSuccess<Output>(success: Output): EngineRunSuccess<Output> {
    return {
        kind: 'success',
        success,
        isSuccess(): this is EngineRunSuccess<Output> {
            return true;
        },
        isError(): this is EngineRunError<Output> {
            return false;
        },
    }
}

export function createEngineRunError<Output = never>(error: Error): EngineRunError<Output> {
    return {
        kind: 'error',
        error,
        isSuccess(): this is EngineRunSuccess<Output> {
            return false;
        },
        isError(): this is EngineRunError<Output> {
            return true;
        },
    }
}
