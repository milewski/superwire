import { close, DataType, load, open, restorePointer, wrapPointer } from 'ffi-rs'

import { FFI_LIBRARY_KEY, FFI_OPERATION, FFI_PROTOCOL_VERSION } from './constants'
import { resolveDefaultLibraryPath } from './library-path'
import type {
    EngineFfiBridgeOptions,
    FfiBoundaryEnvelope,
    FfiInvokeRequestEnvelope,
    FfiResponseEnvelope,
    ReadExecutionValueEnvelope,
    ReadExecutionValueRequest,
    RequestOptions,
} from './types'

export class EngineFfiBridge {
    private readonly libraryPath: string

    private isLibraryOpen: boolean

    constructor(options: EngineFfiBridgeOptions = {}) {
        this.libraryPath = options.libraryPath ?? resolveDefaultLibraryPath()
        this.isLibraryOpen = false
    }

    async executeWorkflow<WorkflowPayload, WorkflowResult = unknown>(
        workflowExecutionRequest: WorkflowPayload,
        options: RequestOptions = {},
    ): Promise<WorkflowResult> {
        const responseEnvelope = await this.invoke<WorkflowResult>({
            protocol_version: FFI_PROTOCOL_VERSION,
            request_id: options.requestId,
            operation: FFI_OPERATION.EXECUTE_WORKFLOW,
            payload: workflowExecutionRequest,
        })

        if (responseEnvelope.operation !== FFI_OPERATION.EXECUTE_WORKFLOW) {
            throw new Error(`Unexpected FFI response operation: ${ responseEnvelope.operation }`)
        }

        return responseEnvelope.payload
    }

    async invokeTool<ToolPayload, ToolResult = unknown>(
        toolInvocationPayload: ToolPayload,
        options: RequestOptions = {},
    ): Promise<ToolResult> {
        const responseEnvelope = await this.invoke<ToolResult>({
            protocol_version: FFI_PROTOCOL_VERSION,
            request_id: options.requestId,
            operation: FFI_OPERATION.INVOKE_TOOL,
            payload: toolInvocationPayload,
        })

        if (responseEnvelope.operation !== FFI_OPERATION.INVOKE_TOOL) {
            throw new Error(`Unexpected FFI response operation: ${ responseEnvelope.operation }`)
        }

        return responseEnvelope.payload
    }

    async readExecutionValue(
        readExecutionValueRequest: ReadExecutionValueRequest,
        options: RequestOptions = {},
    ): Promise<ReadExecutionValueEnvelope> {
        const responseEnvelope = await this.invoke<ReadExecutionValueEnvelope>({
            protocol_version: FFI_PROTOCOL_VERSION,
            request_id: options.requestId,
            operation: FFI_OPERATION.READ_EXECUTION_VALUE,
            payload: readExecutionValueRequest,
        })

        if (responseEnvelope.operation !== FFI_OPERATION.READ_EXECUTION_VALUE) {
            throw new Error(`Unexpected FFI response operation: ${ responseEnvelope.operation }`)
        }

        return responseEnvelope.payload
    }

    async invoke<Payload>(requestEnvelope: FfiInvokeRequestEnvelope): Promise<FfiResponseEnvelope<Payload>> {
        this.ensureLibraryOpen()

        const boundaryEnvelope = await this.invokeBoundary<Payload>(requestEnvelope)

        if (boundaryEnvelope.status === 'failed') {
            const boundaryErrorCode = boundaryEnvelope.error?.code ?? 'unknown'
            const boundaryErrorMessage = boundaryEnvelope.error?.message ?? 'Unknown FFI boundary error'

            throw new Error(`FFI boundary error (${ boundaryErrorCode }): ${ boundaryErrorMessage }`)
        }

        if (boundaryEnvelope.status !== 'succeeded') {
            throw new Error(`Unknown FFI boundary status: ${ String(boundaryEnvelope.status) }`)
        }

        const responseEnvelope = boundaryEnvelope.response

        if (!responseEnvelope || typeof responseEnvelope !== 'object') {
            throw new Error('FFI response envelope is missing')
        }

        if (responseEnvelope.protocol_version !== FFI_PROTOCOL_VERSION) {
            throw new Error(
                `Unsupported FFI protocol version: ${ String(responseEnvelope.protocol_version) } (expected ${ FFI_PROTOCOL_VERSION })`,
            )
        }

        return responseEnvelope
    }

    close(): void {
        if (!this.isLibraryOpen) {
            return
        }

        close(FFI_LIBRARY_KEY)
        this.isLibraryOpen = false
    }

    private ensureLibraryOpen(): void {
        if (this.isLibraryOpen) {
            return
        }

        open({
            library: FFI_LIBRARY_KEY,
            path: this.libraryPath,
        })

        this.isLibraryOpen = true
    }

    private async invokeBoundary<Payload>(requestEnvelope: FfiInvokeRequestEnvelope): Promise<FfiBoundaryEnvelope<Payload>> {
        const requestPayload = JSON.stringify(requestEnvelope)

        const responsePointer = await load({
            library: FFI_LIBRARY_KEY,
            funcName: 'engine_ffi_invoke_json',
            retType: DataType.External,
            paramsType: [ DataType.String ],
            paramsValue: [ requestPayload ],
            runInNewThread: true,
        })

        if (!responsePointer) {
            throw new Error('FFI returned a null response pointer')
        }

        let responsePayload: string

        try {
            const restoredValues = restorePointer<DataType.String>({
                retType: [ DataType.String ],
                paramsValue: wrapPointer([ responsePointer ]),
            })

            responsePayload = restoredValues[ 0 ] ?? ''
        } finally {
            load({
                library: FFI_LIBRARY_KEY,
                funcName: 'engine_ffi_free_json',
                retType: DataType.Void,
                paramsType: [ DataType.External ],
                paramsValue: [ responsePointer ],
            })
        }

        try {
            return JSON.parse(responsePayload) as FfiBoundaryEnvelope<Payload>
        } catch (parseError) {
            const typedError = parseError as Error

            throw new Error(`Failed to parse FFI response payload: ${ String(typedError.message) }`)
        }
    }
}

export function createEngineFfiBridge(options?: EngineFfiBridgeOptions): EngineFfiBridge {
    return new EngineFfiBridge(options)
}
