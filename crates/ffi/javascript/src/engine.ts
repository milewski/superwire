import { EngineFfiBridge } from './bridge'
import { EngineRunFailure, EngineRunSuccess } from './types'
import type {
    EngineFfiBridgeOptions,
    EngineRunOptions,
    EngineRunResult,
    JsonRecord,
    WorkflowExecutionEnvelope,
    WorkflowExecutionRequest,
} from './types'
import { Workflow } from './workflow'

export interface EngineOptions {
    bridge?: EngineFfiBridge;
    bridgeOptions?: EngineFfiBridgeOptions;
    executionIdGenerator?: () => string;
}

export class Engine {
    private readonly engineFfiBridge: EngineFfiBridge

    private readonly executionIdGenerator: () => string

    constructor(options: EngineOptions = {}) {
        this.engineFfiBridge = options.bridge ?? new EngineFfiBridge(options.bridgeOptions)
        this.executionIdGenerator = options.executionIdGenerator ?? (() => this.generateExecutionId())
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

            return new EngineRunFailure(executionError)
        }

        if (workflowExecutionEnvelope.status === 'failed') {
            return new EngineRunFailure(
                new Error(
                    `[${ workflowExecutionEnvelope.error.code }] ${ workflowExecutionEnvelope.error.message }`,
                ),
            )
        }

        return new EngineRunSuccess(workflowExecutionEnvelope.output.output)
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
