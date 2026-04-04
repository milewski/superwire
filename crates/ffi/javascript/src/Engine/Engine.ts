import { randomUUID } from 'node:crypto'
import { createServer, type IncomingMessage, type ServerResponse } from 'node:http'

import { EngineFfiBridge } from './bridge'
import type { CustomToolDeclaration, EngineExecutionError, EngineExecutionResult, EngineFfiBridgeOptions, JsonRecord, ReadExecutionValueEnvelope, ToolInvocationEnvelope, ToolInvocationPayload, WorkflowExecutionEnvelope, WorkflowExecutionRequest } from './types'
import type { Tool, ToolArguments, ToolExecutionContext } from './tool'
import { Workflow } from './workflow'

interface ToolCallbackHandle {
    endpoint: string;
    authToken: string;
    close: () => Promise<void>;
}

export interface EngineOptions {
    bridge?: EngineFfiBridge;
    bridgeOptions?: EngineFfiBridgeOptions;
    executionIdGenerator?: () => string;
}

export interface RegisterToolOptions {
    bounded?: JsonRecord;
}

interface RegisteredTool {
    tool: Tool;
    bounded: JsonRecord;
}

class DeferredEngineExecutionResult<Output> implements EngineExecutionResult<Output> {
    readonly executionId: string

    private readonly engineFfiBridge: EngineFfiBridge

    private cachedSuccessValue: Output | null | undefined

    private cachedErrorValue: EngineExecutionError | null | undefined

    private cachedContextValue: unknown

    private readonly canReadDeferredValues: boolean

    constructor(engineFfiBridge: EngineFfiBridge, executionId: string, eagerError: EngineExecutionError | null = null, canReadDeferredValues = true) {
        this.engineFfiBridge = engineFfiBridge
        this.executionId = executionId
        this.cachedErrorValue = eagerError
        this.cachedSuccessValue = eagerError ? null : undefined
        this.canReadDeferredValues = canReadDeferredValues
    }

    async isSuccess(): Promise<boolean> {
        const successValue = await this.success()

        return successValue !== null
    }

    async isError(): Promise<boolean> {
        const errorValue = await this.error()

        return errorValue !== null
    }

    async success(): Promise<Output | null> {
        if (this.cachedSuccessValue !== undefined) {
            return this.cachedSuccessValue
        }

        if (!this.canReadDeferredValues) {
            this.cachedSuccessValue = null

            return this.cachedSuccessValue
        }

        let responseValue: unknown

        try {
            responseValue = await this.readExecutionValue('success')
        } catch (error) {
            if (this.cachedErrorValue === undefined) {
                this.cachedErrorValue = {
                    code: 'execution_failed',
                    message: error instanceof Error ? error.message : String(error),
                }
            }

            this.cachedSuccessValue = null

            return this.cachedSuccessValue
        }

        this.cachedSuccessValue = responseValue as Output | null

        return this.cachedSuccessValue
    }

    async error(): Promise<EngineExecutionError | null> {
        if (this.cachedErrorValue !== undefined) {
            return this.cachedErrorValue
        }

        if (!this.canReadDeferredValues) {
            this.cachedErrorValue = null

            return this.cachedErrorValue
        }

        let responseValue: unknown

        try {
            responseValue = await this.readExecutionValue('error')
        } catch (error) {
            this.cachedErrorValue = {
                code: 'execution_failed',
                message: error instanceof Error ? error.message : String(error),
            }

            return this.cachedErrorValue
        }

        if (!responseValue || typeof responseValue !== 'object') {
            this.cachedErrorValue = null

            return this.cachedErrorValue
        }

        const errorObject = responseValue as {
            code?: string;
            message?: string;
            context?: unknown;
            details?: unknown;
        }

        this.cachedErrorValue = {
            code: errorObject.code ?? 'execution_failed',
            message: errorObject.message ?? 'Unknown workflow execution error',
            context: errorObject.context,
            details: errorObject.details,
        }

        return this.cachedErrorValue
    }

    async context(): Promise<unknown> {
        if (this.cachedContextValue !== undefined) {
            return this.cachedContextValue
        }

        if (!this.canReadDeferredValues) {
            this.cachedContextValue = null

            return this.cachedContextValue
        }

        try {
            this.cachedContextValue = await this.readExecutionValue('context')
        } catch {
            this.cachedContextValue = null
        }

        return this.cachedContextValue
    }

    private async readExecutionValue(valueName: 'success' | 'error' | 'context'): Promise<unknown> {
        const readExecutionValueEnvelope: ReadExecutionValueEnvelope = await this.engineFfiBridge.readExecutionValue({
            execution_id: this.executionId,
            value: valueName,
        })

        if (readExecutionValueEnvelope.status === 'failed') {
            throw new Error(`[${ readExecutionValueEnvelope.error.code }] ${ readExecutionValueEnvelope.error.message }`)
        }

        return readExecutionValueEnvelope.result.value
    }
}

export class Engine {
    private readonly engineFfiBridge: EngineFfiBridge

    private readonly executionIdGenerator: () => string

    private readonly registeredToolsByName: Map<string, RegisteredTool>

    constructor(options: EngineOptions = {}) {
        this.engineFfiBridge = options.bridge ?? new EngineFfiBridge(options.bridgeOptions)
        this.executionIdGenerator = options.executionIdGenerator ?? (() => this.generateExecutionId())
        this.registeredToolsByName = new Map()
    }

    registerGlobalTool(tool: Tool, options: RegisterToolOptions = {}): this {
        this.registeredToolsByName.set(tool.name, {
            tool,
            bounded: options.bounded ?? {},
        })

        return this
    }

    registerTool(tool: Tool, options: RegisterToolOptions = {}): this {
        return this.registerGlobalTool(tool, options)
    }

    unregisterTool(toolName: string): boolean {
        return this.registeredToolsByName.delete(toolName)
    }

    unregisterGlobalTool(toolName: string): boolean {
        return this.unregisterTool(toolName)
    }

    registeredTools(): Tool[] {
        return [ ...this.registeredToolsByName.values() ].map((registeredTool) => registeredTool.tool)
    }

    registeredGlobalTools(): Tool[] {
        return this.registeredTools()
    }

    async invokeTool<Input extends JsonRecord = JsonRecord, Output = unknown>(toolName: string, input: Input): Promise<Output> {
        const registeredTool = this.registeredToolsByName.get(toolName)

        if (!registeredTool) {
            throw new Error(`Tool \`${ toolName }\` is not registered. Call engine.registerGlobalTool(...) first.`)
        }

        const toolOutput = await registeredTool.tool.execute({
            input,
            bounded: registeredTool.bounded,
            context: {},
        })

        return toolOutput as Output
    }

    async run<
        Output = unknown,
        Input extends JsonRecord = JsonRecord,
        Secrets extends JsonRecord = JsonRecord,
    >(workflow: Workflow<Input, Secrets, Output>): Promise<EngineExecutionResult<Output>> {
        let executionId = ''
        let toolCallbackHandle: ToolCallbackHandle | null = null

        try {
            const defaultExecutionId = this.executionIdGenerator()

            executionId = workflow.executionId(defaultExecutionId)

            const workflowExecutionRequest = workflow.toExecutionRequest(executionId)

            workflowExecutionRequest.custom_tools = this.resolveCustomToolDeclarations(workflowExecutionRequest.custom_tools)
            workflowExecutionRequest.defer_output = true

            toolCallbackHandle = await this.startToolCallbackServer(executionId, workflow.scopedToolsByName)

            if (toolCallbackHandle) {
                workflowExecutionRequest.tool_callback = {
                    endpoint: toolCallbackHandle.endpoint,
                    auth_token: toolCallbackHandle.authToken,
                }
            }

            const workflowExecutionEnvelope = await this.engineFfiBridge.executeWorkflow<
                WorkflowExecutionRequest,
                WorkflowExecutionEnvelope<Output>
            >(workflowExecutionRequest, {
                requestId: workflow.requestId(),
            })

            if (workflowExecutionEnvelope.status === 'failed') {
                return new DeferredEngineExecutionResult<Output>(
                    this.engineFfiBridge,
                    executionId,
                    {
                        code: workflowExecutionEnvelope.error.code,
                        message: workflowExecutionEnvelope.error.message,
                        context: workflowExecutionEnvelope.error.context,
                        details: workflowExecutionEnvelope.error.details,
                    },
                    true,
                )
            }

            return new DeferredEngineExecutionResult<Output>(
                this.engineFfiBridge,
                workflowExecutionEnvelope.output.execution_id,
            )
        } catch (error) {
            const executionError: EngineExecutionError = {
                code: 'execution_failed',
                message: error instanceof Error ? error.message : String(error),
            }

            const fallbackExecutionId = executionId || this.executionIdGenerator()

            return new DeferredEngineExecutionResult<Output>(
                this.engineFfiBridge,
                fallbackExecutionId,
                executionError,
                false,
            )
        } finally {
            if (toolCallbackHandle) {
                try {
                    await toolCallbackHandle.close()
                } catch {
                    // no-op: run() must remain non-throwing
                }
            }
        }
    }

    close(): void {
        this.engineFfiBridge.close()
    }

    private generateExecutionId(): string {
        const timestamp = Date.now()
        const randomSuffix = Math.random().toString(16).slice(2, 10)

        return `execution-${ timestamp }-${ randomSuffix }`
    }

    private resolveCustomToolDeclarations(workflowDeclaredTools: CustomToolDeclaration[]): CustomToolDeclaration[] {
        const customToolDeclarationsByName = new Map<string, CustomToolDeclaration>()

        for (const registeredTool of this.registeredTools()) {
            customToolDeclarationsByName.set(registeredTool.name, registeredTool.toDeclaration())
        }

        for (const customToolDeclaration of workflowDeclaredTools) {
            customToolDeclarationsByName.set(customToolDeclaration.name, customToolDeclaration)
        }

        return [ ...customToolDeclarationsByName.values() ]
    }

    private async startToolCallbackServer(executionId: string, scopedToolsByName: Map<string, Tool>): Promise<ToolCallbackHandle | null> {
        if (this.registeredToolsByName.size === 0 && scopedToolsByName.size === 0) {
            return null
        }

        const authToken = randomUUID()
        const callbackServer = createServer((request, response) => {
            void this.handleToolCallbackRequest(request, response, executionId, authToken, scopedToolsByName)
        })

        await new Promise<void>((resolve, reject) => {
            callbackServer.once('error', reject)
            callbackServer.listen(0, '127.0.0.1', () => {
                callbackServer.off('error', reject)
                resolve()
            })
        })

        const callbackAddress = callbackServer.address()

        if (!callbackAddress || typeof callbackAddress === 'string') {
            throw new Error('Tool callback server did not return a TCP address')
        }

        return {
            endpoint: `http://127.0.0.1:${ callbackAddress.port }/invoke-tool`,
            authToken,
            close: () => new Promise<void>((resolve, reject) => {
                callbackServer.close((closeError) => {
                    if (closeError) {
                        reject(closeError)

                        return
                    }

                    resolve()
                })
            }),
        }
    }

    private async handleToolCallbackRequest(
        request: IncomingMessage,
        response: ServerResponse,
        executionId: string,
        authToken: string,
        scopedToolsByName: Map<string, Tool>,
    ): Promise<void> {
        if (request.method !== 'POST') {
            this.writeToolCallbackResponse(response, 405, {
                status: 'failed',
                error: {
                    code: 'execution_failed',
                    message: 'Only POST is supported for tool callbacks',
                },
            })

            return
        }

        const callbackToken = request.headers[ 'x-engine-ai-tool-callback-token' ]

        if (callbackToken !== authToken) {
            this.writeToolCallbackResponse(response, 401, {
                status: 'failed',
                error: {
                    code: 'execution_failed',
                    message: 'Invalid tool callback token',
                },
            })

            return
        }

        let rawRequestBody = ''

        for await (const bodyChunk of request) {
            rawRequestBody += bodyChunk
        }

        let toolInvocationPayload: ToolInvocationPayload

        try {
            toolInvocationPayload = JSON.parse(rawRequestBody) as ToolInvocationPayload
        } catch (parseError) {
            this.writeToolCallbackResponse(response, 400, {
                status: 'failed',
                error: {
                    code: 'invalid_arguments',
                    message: `Invalid callback payload: ${ String(parseError) }`,
                },
            })

            return
        }

        if (toolInvocationPayload.execution_id !== executionId) {
            this.writeToolCallbackResponse(response, 404, {
                status: 'failed',
                error: {
                    code: 'tool_not_found',
                    message: `Unknown execution id: ${ toolInvocationPayload.execution_id }`,
                },
            })

            return
        }

        const registeredTool = this.resolveRegisteredTool(toolInvocationPayload.tool_name, scopedToolsByName)

        if (!registeredTool) {
            this.writeToolCallbackResponse(response, 404, {
                status: 'failed',
                error: {
                    code: 'tool_not_found',
                    message: `Tool \`${ toolInvocationPayload.tool_name }\` is not registered`,
                },
            })

            return
        }

        const toolArguments = toolInvocationPayload.arguments

        if (!toolArguments || typeof toolArguments !== 'object' || Array.isArray(toolArguments)) {
            this.writeToolCallbackResponse(response, 400, {
                status: 'failed',
                error: {
                    code: 'invalid_arguments',
                    message: 'Tool arguments must be a JSON object',
                },
            })

            return
        }

        try {
            const executionContext = this.normalizeExecutionContext(toolInvocationPayload.execution_context)
            const boundedFromWorkflow = this.normalizeBoundedArguments(executionContext?.boundArguments)
            const executionArguments: ToolArguments<JsonRecord, JsonRecord, ToolExecutionContext> = {
                input: toolArguments as JsonRecord,
                bounded: {
                    ...boundedFromWorkflow,
                    ...registeredTool.bounded,
                },
                context: executionContext ?? {},
            }

            const output = await registeredTool.tool.execute(executionArguments)

            this.writeToolCallbackResponse(response, 200, {
                status: 'succeeded',
                result: {
                    execution_id: toolInvocationPayload.execution_id,
                    invocation_id: toolInvocationPayload.invocation_id,
                    output,
                },
            })
        } catch (error) {
            const toolError = error instanceof Error ? error : new Error(String(error))

            this.writeToolCallbackResponse(response, 500, {
                status: 'failed',
                error: {
                    code: 'execution_failed',
                    message: toolError.message,
                },
            })
        }
    }

    private resolveRegisteredTool(toolName: string, scopedToolsByName: Map<string, Tool>): RegisteredTool | undefined {
        const workflowScopedTool = scopedToolsByName.get(toolName)

        if (workflowScopedTool) {
            return {
                tool: workflowScopedTool,
                bounded: {},
            }
        }

        return this.registeredToolsByName.get(toolName)
    }

    private writeToolCallbackResponse(response: ServerResponse, statusCode: number, envelope: ToolInvocationEnvelope): void {
        response.statusCode = statusCode
        response.setHeader('content-type', 'application/json')
        response.end(JSON.stringify(envelope))
    }

    private normalizeExecutionContext(rawExecutionContext: unknown): ToolExecutionContext | undefined {
        if (!rawExecutionContext || typeof rawExecutionContext !== 'object' || Array.isArray(rawExecutionContext)) {
            return undefined
        }

        const contextObject = rawExecutionContext as Record<string, unknown>

        if ('workflow_input' in contextObject || 'bound_arguments' in contextObject) {
            return {
                workflowInput: contextObject.workflow_input,
                boundArguments: contextObject.bound_arguments,
            }
        }

        return contextObject
    }

    private normalizeBoundedArguments(rawBoundedArguments: unknown): JsonRecord {
        if (!rawBoundedArguments || typeof rawBoundedArguments !== 'object' || Array.isArray(rawBoundedArguments)) {
            return {}
        }

        return rawBoundedArguments as JsonRecord
    }
}
