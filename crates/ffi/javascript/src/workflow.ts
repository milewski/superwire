import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

import type { CustomToolDeclaration, EngineRunOptions, JsonRecord, WorkflowExecutionRequest, WorkflowOptions } from './types'

export class Workflow<Input extends JsonRecord = JsonRecord, Secrets extends JsonRecord = JsonRecord> {
    readonly source: string

    readonly inputPayload: Input

    readonly secretsPayload?: Secrets

    readonly customTools: CustomToolDeclaration[]

    readonly runOptions: EngineRunOptions

    constructor(source: string, options: WorkflowOptions<Input, Secrets> = {}) {
        this.source = source
        this.inputPayload = (options.inputPayload ?? {}) as Input
        this.secretsPayload = options.secretsPayload
        this.customTools = options.customTools ?? []
        this.runOptions = options.runOptions ?? {}
    }

    static fromFile<Input extends JsonRecord = JsonRecord, Secrets extends JsonRecord = JsonRecord>(
        filePath: string,
        options: WorkflowOptions<Input, Secrets> = {},
    ): Workflow<Input, Secrets> {
        const workflowSource = readFileSync(resolve(filePath), 'utf8')

        return new Workflow<Input, Secrets>(workflowSource, options)
    }

    executionId(fallbackExecutionId: string): string {
        return this.runOptions.executionId ?? fallbackExecutionId
    }

    requestId(): string | undefined {
        return this.runOptions.requestId
    }

    toExecutionRequest(
        executionId: string,
        inputPayload: Input = this.inputPayload,
        secretsPayload: Secrets | undefined = this.secretsPayload,
    ): WorkflowExecutionRequest<Input> {
        const workflowExecutionRequest: WorkflowExecutionRequest<Input> = {
            execution_id: executionId,
            workflow_source: this.source,
            input: {
                payload: inputPayload,
            },
            custom_tools: this.customTools,
        }

        if (secretsPayload) {
            workflowExecutionRequest.secrets = {
                payload: secretsPayload,
            }
        }

        return workflowExecutionRequest
    }
}
