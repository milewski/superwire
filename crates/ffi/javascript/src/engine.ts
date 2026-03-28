import { randomUUID } from 'node:crypto'
import { createServer, type IncomingMessage, type ServerResponse } from 'node:http'

import { EngineFfiBridge } from './bridge'
import { createEngineRunError, createEngineRunSuccess } from './types'
import type {
    CustomToolDeclaration,
    EngineFfiBridgeOptions,
    EngineRunOptions,
    EngineRunResult,
    JsonRecord,
    ToolInvocationEnvelope,
    ToolInvocationPayload,
    WorkflowExecutionEnvelope,
    WorkflowExecutionRequest,
} from './types'
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

export class Engine {
    private readonly engineFfiBridge: EngineFfiBridge

    private readonly executionIdGenerator: () => string

    private readonly registeredToolsByName: Map<string, RegisteredTool>

    constructor(options: EngineOptions = {}) {
        this.engineFfiBridge = options.bridge ?? new EngineFfiBridge(options.bridgeOptions)
        this.executionIdGenerator = options.executionIdGenerator ?? (() => this.generateExecutionId())
        this.registeredToolsByName = new Map()
    }

    registerTool(tool: Tool, options: RegisterToolOptions = {}): this {
        this.registeredToolsByName.set(tool.name, {
            tool,
            bounded: options.bounded ?? {},
        })

        return this
    }

    unregisterTool(toolName: string): boolean {
        return this.registeredToolsByName.delete(toolName)
    }

    registeredTools(): Tool[] {
        return [ ...this.registeredToolsByName.values() ].map((registeredTool) => registeredTool.tool)
    }

    async invokeTool<Input extends JsonRecord = JsonRecord, Output = unknown>(toolName: string, input: Input): Promise<Output> {
        const registeredTool = this.registeredToolsByName.get(toolName)

        if (!registeredTool) {
            throw new Error(`Tool \`${ toolName }\` is not registered. Call engine.registerTool(...) first.`)
        }

        const toolOutput = await registeredTool.tool.execute({
            input,
            bounded: registeredTool.bounded,
            context: {},
        })

        return toolOutput as Output
    }

    async run<Output = unknown, Input extends JsonRecord = JsonRecord, Secrets extends JsonRecord = JsonRecord>(
        workflow: Workflow,
        inputPayload: Input,
        secretsOrOptions: Secrets | EngineRunOptions = {},
        options: EngineRunOptions = {},
    ): Promise<EngineRunResult<Output>> {
        const { secretsPayload, runOptions } = this.resolveRunArguments(secretsOrOptions, options)
        const executionId = runOptions.executionId ?? this.executionIdGenerator()
        const workflowExecutionRequest = workflow.toExecutionRequest(executionId, inputPayload, secretsPayload)
        workflowExecutionRequest.custom_tools = this.resolveCustomToolDeclarations(workflowExecutionRequest.custom_tools)

        const toolCallbackHandle = await this.startToolCallbackServer(executionId)

        if (toolCallbackHandle) {
            workflowExecutionRequest.tool_callback = {
                endpoint: toolCallbackHandle.endpoint,
                auth_token: toolCallbackHandle.authToken,
            }
        }

        let workflowExecutionEnvelope: WorkflowExecutionEnvelope<Output>

        try {
            workflowExecutionEnvelope = await this.engineFfiBridge.executeWorkflow<
                WorkflowExecutionRequest<Input>,
                WorkflowExecutionEnvelope<Output>
            >(workflowExecutionRequest, {
                requestId: runOptions.requestId,
            })
        } catch (error) {
            if (toolCallbackHandle) {
                await toolCallbackHandle.close()
            }

            const executionError = error instanceof Error ? error : new Error(String(error))

            return createEngineRunError<Output>(executionError)
        }

        if (toolCallbackHandle) {
            await toolCallbackHandle.close()
        }

        if (workflowExecutionEnvelope.status === 'failed') {
            return createEngineRunError<Output>(
                new Error(
                    `[${ workflowExecutionEnvelope.error.code }] ${ workflowExecutionEnvelope.error.message }`,
                ),
            )
        }

        return createEngineRunSuccess(workflowExecutionEnvelope.output.output)
    }

    private resolveRunArguments<Secrets extends JsonRecord>(
        secretsOrOptions: Secrets | EngineRunOptions,
        options: EngineRunOptions,
    ): {
        secretsPayload?: Secrets;
        runOptions: EngineRunOptions;
    } {
        if (this.isEngineRunOptions(secretsOrOptions) && Object.keys(options).length === 0) {
            return {
                runOptions: secretsOrOptions,
            }
        }

        return {
            secretsPayload: secretsOrOptions as Secrets,
            runOptions: options,
        }
    }

    private isEngineRunOptions(value: JsonRecord | EngineRunOptions): value is EngineRunOptions {
        const valueRecord = value as Record<string, unknown>

        const hasOnlyRunOptionFields = Object.keys(valueRecord).every((key) => key === 'requestId' || key === 'executionId')

        if (!hasOnlyRunOptionFields) {
            return false
        }

        if ('requestId' in valueRecord && valueRecord.requestId !== undefined && typeof valueRecord.requestId !== 'string') {
            return false
        }

        if ('executionId' in valueRecord && valueRecord.executionId !== undefined && typeof valueRecord.executionId !== 'string') {
            return false
        }

        return true
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

        for (const customToolDeclaration of workflowDeclaredTools) {
            customToolDeclarationsByName.set(customToolDeclaration.name, customToolDeclaration)
        }

        for (const registeredTool of this.registeredTools()) {
            customToolDeclarationsByName.set(registeredTool.name, registeredTool.toDeclaration())
        }

        return [ ...customToolDeclarationsByName.values() ]
    }

    private async startToolCallbackServer(executionId: string): Promise<ToolCallbackHandle | null> {
        if (this.registeredToolsByName.size === 0) {
            return null
        }

        const authToken = randomUUID()
        const callbackServer = createServer((request, response) => {
            void this.handleToolCallbackRequest(request, response, executionId, authToken)
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

        const callbackToken = request.headers['x-engine-ai-tool-callback-token']

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

        const registeredTool = this.registeredToolsByName.get(toolInvocationPayload.tool_name)

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
