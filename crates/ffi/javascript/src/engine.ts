import { EngineFfiBridge } from './bridge'
import { createEngineRunError, createEngineRunSuccess } from './types'
import type {
    EngineFfiBridgeOptions,
    EngineRunOptions,
    JsonRecord,
    EngineRunResult,
    WorkflowExecutionEnvelope,
    WorkflowExecutionRequest,
} from './types'
import type { Tool } from './tool'
import { Workflow } from './workflow'

export interface EngineOptions {
    bridge?: EngineFfiBridge;
    bridgeOptions?: EngineFfiBridgeOptions;
    executionIdGenerator?: () => string;
}

export class Engine {
    private readonly engineFfiBridge: EngineFfiBridge

    private readonly executionIdGenerator: () => string

    private readonly registeredToolsByName: Map<string, Tool>

    constructor(options: EngineOptions = {}) {
        this.engineFfiBridge = options.bridge ?? new EngineFfiBridge(options.bridgeOptions)
        this.executionIdGenerator = options.executionIdGenerator ?? (() => this.generateExecutionId())
        this.registeredToolsByName = new Map()
    }

    registerTool(tool: Tool): this {
        this.registeredToolsByName.set(tool.name, tool)

        return this
    }

    unregisterTool(toolName: string): boolean {
        return this.registeredToolsByName.delete(toolName)
    }

    registeredTools(): Tool[] {
        return [ ...this.registeredToolsByName.values() ]
    }

    async invokeTool<Input extends JsonRecord = JsonRecord, Output = unknown>(toolName: string, input: Input): Promise<Output> {
        const tool = this.registeredToolsByName.get(toolName)

        if (!tool) {
            throw new Error(`Tool \`${ toolName }\` is not registered. Call engine.registerTool(...) first.`)
        }

        const toolOutput = await tool.execute(input)

        return toolOutput as Output
    }

    async run<Output = unknown, Input extends JsonRecord = JsonRecord>(
        workflow: Workflow,
        inputPayload: Input,
        options: EngineRunOptions = {},
    ): Promise<EngineRunResult<Output>> {
        const executionId = options.executionId ?? this.executionIdGenerator()
        const workflowExecutionRequest = workflow.toExecutionRequest(executionId, inputPayload)

        let workflowExecutionEnvelope: WorkflowExecutionEnvelope<Output>

        try {
            workflowExecutionEnvelope = this.engineFfiBridge.executeWorkflow<
                WorkflowExecutionRequest<Input>,
                WorkflowExecutionEnvelope<Output>
            >(workflowExecutionRequest, {
                requestId: options.requestId,
            })
        } catch (error) {
            const executionError = error instanceof Error ? error : new Error(String(error))

            return createEngineRunError<Output>(executionError)
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

    close(): void {
        this.engineFfiBridge.close()
    }

    private generateExecutionId(): string {
        const timestamp = Date.now()
        const randomSuffix = Math.random().toString(16).slice(2, 10)

        return `execution-${ timestamp }-${ randomSuffix }`
    }
}
