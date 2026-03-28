import type { CustomToolDeclaration, JsonRecord, WorkflowExecutionRequest, WorkflowOptions } from './types'

export class Workflow {
    readonly source: string

    readonly customTools: CustomToolDeclaration[]

    constructor(source: string, options: WorkflowOptions = {}) {
        this.source = source
        this.customTools = options.customTools ?? []
    }

    toExecutionRequest<Input extends JsonRecord>(
        executionId: string,
        inputPayload: Input,
        secretsPayload?: JsonRecord,
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
