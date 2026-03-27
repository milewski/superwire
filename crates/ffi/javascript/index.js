"use strict";

const path = require("node:path");

const { DataType, close, load, open, restorePointer, wrapPointer } = require("ffi-rs");

const FFI_PROTOCOL_VERSION = 1;

const FFI_LIBRARY_KEY = "engine_ai_ffi";

const FFI_OPERATION = Object.freeze({
  EXECUTE_WORKFLOW: "execute_workflow",
  INVOKE_TOOL: "invoke_tool",
});

class EngineFfiBridge {
  constructor(options = {}) {
    this.libraryPath = options.libraryPath ?? resolveDefaultLibraryPath();
    this.isLibraryOpen = false;
  }

  executeWorkflow(workflowExecutionRequest, options = {}) {
    const responseEnvelope = this.invoke({
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

  invokeTool(toolInvocationPayload, options = {}) {
    const responseEnvelope = this.invoke({
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

  invoke(requestEnvelope) {
    this.ensureLibraryOpen();

    const boundaryEnvelope = this.invokeBoundary(requestEnvelope);

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

  close() {
    if (!this.isLibraryOpen) {
      return;
    }

    close(FFI_LIBRARY_KEY);
    this.isLibraryOpen = false;
  }

  ensureLibraryOpen() {
    if (this.isLibraryOpen) {
      return;
    }

    open({
      library: FFI_LIBRARY_KEY,
      path: this.libraryPath,
    });

    this.isLibraryOpen = true;
  }

  invokeBoundary(requestEnvelope) {
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

    let responsePayload;

    try {
      responsePayload = restorePointer({
        retType: [DataType.String],
        paramsValue: wrapPointer([responsePointer]),
      });
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
      return JSON.parse(responsePayload);
    } catch (parseError) {
      throw new Error(`Failed to parse FFI response payload: ${String(parseError.message)}`);
    }
  }
}

function resolveDefaultLibraryPath() {
  if (process.env.ENGINE_AI_FFI_LIBRARY_PATH) {
    return path.resolve(process.env.ENGINE_AI_FFI_LIBRARY_PATH);
  }

  return path.resolve(__dirname, "native", libraryFileNameForCurrentPlatform());
}

function libraryFileNameForCurrentPlatform() {
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

function createEngineFfiBridge(options) {
  return new EngineFfiBridge(options);
}

module.exports = {
  EngineFfiBridge,
  FFI_OPERATION,
  FFI_PROTOCOL_VERSION,
  createEngineFfiBridge,
};
