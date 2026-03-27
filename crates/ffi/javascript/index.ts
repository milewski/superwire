import path from "node:path";

import { DataType, close, load, open, restorePointer, wrapPointer } from "ffi-rs";

export const FFI_PROTOCOL_VERSION = 1;

const FFI_LIBRARY_KEY = "engine_ai_ffi";

export const FFI_OPERATION = Object.freeze({
  EXECUTE_WORKFLOW: "execute_workflow",
  INVOKE_TOOL: "invoke_tool",
} as const);

export type FfiOperation = (typeof FFI_OPERATION)[keyof typeof FFI_OPERATION];

export type FfiBoundaryStatus = "succeeded" | "failed";

export interface FfiBoundaryError {
  code: string;
  message: string;
}

export interface FfiResponseEnvelope<TPayload = unknown> {
  protocol_version: number;
  request_id?: string;
  operation: FfiOperation;
  payload: TPayload;
}

export interface FfiBoundaryEnvelope<TPayload = unknown> {
  status: FfiBoundaryStatus;
  response?: FfiResponseEnvelope<TPayload>;
  error?: FfiBoundaryError;
}

export interface EngineFfiBridgeOptions {
  libraryPath?: string;
}

export interface RequestOptions {
  requestId?: string;
}

export interface FfiInvokeRequestEnvelope<TPayload = unknown> {
  protocol_version: number;
  request_id?: string;
  operation: FfiOperation;
  payload: TPayload;
}

type JsonRecord = Record<string, unknown>;

export class EngineFfiBridge {
  private readonly libraryPath: string;

  private isLibraryOpen: boolean;

  constructor(options: EngineFfiBridgeOptions = {}) {
    this.libraryPath = options.libraryPath ?? resolveDefaultLibraryPath();
    this.isLibraryOpen = false;
  }

  executeWorkflow<TWorkflowPayload extends JsonRecord, TWorkflowResult = unknown>(
    workflowExecutionRequest: TWorkflowPayload,
    options: RequestOptions = {},
  ): TWorkflowResult {
    const responseEnvelope = this.invoke<TWorkflowResult>({
      protocol_version: FFI_PROTOCOL_VERSION,
      request_id: options.requestId,
      operation: FFI_OPERATION.EXECUTE_WORKFLOW,
      payload: workflowExecutionRequest,
    });

    if (responseEnvelope.operation !== FFI_OPERATION.EXECUTE_WORKFLOW) {
      throw new Error(`Unexpected FFI response operation: ${responseEnvelope.operation}`);
    }

    return responseEnvelope.payload;
  }

  invokeTool<TToolPayload extends JsonRecord, TToolResult = unknown>(
    toolInvocationPayload: TToolPayload,
    options: RequestOptions = {},
  ): TToolResult {
    const responseEnvelope = this.invoke<TToolResult>({
      protocol_version: FFI_PROTOCOL_VERSION,
      request_id: options.requestId,
      operation: FFI_OPERATION.INVOKE_TOOL,
      payload: toolInvocationPayload,
    });

    if (responseEnvelope.operation !== FFI_OPERATION.INVOKE_TOOL) {
      throw new Error(`Unexpected FFI response operation: ${responseEnvelope.operation}`);
    }

    return responseEnvelope.payload;
  }

  invoke<TPayload>(requestEnvelope: FfiInvokeRequestEnvelope): FfiResponseEnvelope<TPayload> {
    this.ensureLibraryOpen();

    const boundaryEnvelope = this.invokeBoundary<TPayload>(requestEnvelope);

    if (boundaryEnvelope.status === "failed") {
      const boundaryErrorCode = boundaryEnvelope.error?.code ?? "unknown";
      const boundaryErrorMessage = boundaryEnvelope.error?.message ?? "Unknown FFI boundary error";

      throw new Error(`FFI boundary error (${boundaryErrorCode}): ${boundaryErrorMessage}`);
    }

    if (boundaryEnvelope.status !== "succeeded") {
      throw new Error(`Unknown FFI boundary status: ${String(boundaryEnvelope.status)}`);
    }

    const responseEnvelope = boundaryEnvelope.response;

    if (!responseEnvelope || typeof responseEnvelope !== "object") {
      throw new Error("FFI response envelope is missing");
    }

    if (responseEnvelope.protocol_version !== FFI_PROTOCOL_VERSION) {
      throw new Error(
        `Unsupported FFI protocol version: ${String(responseEnvelope.protocol_version)} (expected ${FFI_PROTOCOL_VERSION})`,
      );
    }

    return responseEnvelope;
  }

  close(): void {
    if (!this.isLibraryOpen) {
      return;
    }

    close(FFI_LIBRARY_KEY);
    this.isLibraryOpen = false;
  }

  private ensureLibraryOpen(): void {
    if (this.isLibraryOpen) {
      return;
    }

    open({
      library: FFI_LIBRARY_KEY,
      path: this.libraryPath,
    });

    this.isLibraryOpen = true;
  }

  private invokeBoundary<TPayload>(requestEnvelope: FfiInvokeRequestEnvelope): FfiBoundaryEnvelope<TPayload> {
    const requestPayload = JSON.stringify(requestEnvelope);

    const responsePointer = load({
      library: FFI_LIBRARY_KEY,
      funcName: "engine_ffi_invoke_json",
      retType: DataType.External,
      paramsType: [DataType.String],
      paramsValue: [requestPayload],
    });

    if (!responsePointer) {
      throw new Error("FFI returned a null response pointer");
    }

    let responsePayload: string;

    try {
      const restoredValues = restorePointer<DataType.String>({
        retType: [DataType.String],
        paramsValue: wrapPointer([responsePointer]),
      });

      responsePayload = restoredValues[0] ?? "";
    } finally {
      load({
        library: FFI_LIBRARY_KEY,
        funcName: "engine_ffi_free_json",
        retType: DataType.Void,
        paramsType: [DataType.External],
        paramsValue: [responsePointer],
      });
    }

    try {
      return JSON.parse(responsePayload) as FfiBoundaryEnvelope<TPayload>;
    } catch (parseError) {
      const typedError = parseError as Error;

      throw new Error(`Failed to parse FFI response payload: ${String(typedError.message)}`);
    }
  }
}

function resolveDefaultLibraryPath(): string {
  if (process.env.ENGINE_AI_FFI_LIBRARY_PATH) {
    return path.resolve(process.env.ENGINE_AI_FFI_LIBRARY_PATH);
  }

  return path.resolve(__dirname, "..", "native", libraryFileNameForCurrentPlatform());
}

function libraryFileNameForCurrentPlatform(): string {
  switch (process.platform) {
    case "darwin":
      return "libffi.dylib";

    case "linux":
      return "libffi.so";

    case "win32":
      return "ffi.dll";

    default:
      throw new Error(`Unsupported platform for engine ffi library: ${process.platform}`);
  }
}

export function createEngineFfiBridge(options?: EngineFfiBridgeOptions): EngineFfiBridge {
  return new EngineFfiBridge(options);
}
