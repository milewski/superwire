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

export class EngineRunSuccess<Output = unknown> {
    readonly success: Output;

    constructor(success: Output) {
        this.success = success;
    }

    isSuccess(): this is EngineRunSuccess<Output> {
        return true;
    }

    isError(): this is EngineRunFailure {
        return false;
    }
}

export class EngineRunFailure {
    readonly failure: Error;

    constructor(failure: Error) {
        this.failure = failure;
    }

    isSuccess(): this is EngineRunSuccess<never> {
        return false;
    }

    isError(): this is EngineRunFailure {
        return true;
    }
}

export type EngineRunResult<Output = unknown> = EngineRunSuccess<Output> | EngineRunFailure;
