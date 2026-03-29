import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

import type { CustomToolDeclaration, EngineRunOptions, JsonRecord, WorkflowExecutionRequest, WorkflowOptions } from './types'
import type { Tool } from './tool'

export class Workflow<
    Input extends JsonRecord = JsonRecord,
    Secrets extends JsonRecord = JsonRecord,
    Output = unknown,
> {
    readonly source: string

    readonly inputPayload: Input

    readonly secretsPayload?: Secrets

    readonly customTools: CustomToolDeclaration[]

    readonly scopedToolsByName: Map<string, Tool>

    readonly runOptions: EngineRunOptions

    constructor(source: string, options: WorkflowOptions<Input, Secrets> = {}) {
        const legacyRunOptions = options.options ?? {}
        const scopedToolsByName = this.resolveScopedToolsByName(options.tools ?? [])
        const customTools = this.resolveCustomTools(options.tools ?? [])

        this.source = source
        this.inputPayload = (options.inputs ?? {}) as Input
        this.secretsPayload = options.secrets
        this.customTools = customTools
        this.scopedToolsByName = scopedToolsByName
        this.runOptions = {
            requestId: options.requestId ?? legacyRunOptions.requestId,
            executionId: options.executionId ?? legacyRunOptions.executionId,
        }
    }

    static fromFile<
        Output = unknown,
        Input extends JsonRecord = JsonRecord,
        Secrets extends JsonRecord = JsonRecord,
    >(
        filePath: string,
        options: WorkflowOptions<Input, Secrets> = {},
    ): Workflow<Input, Secrets, Output> {
        const source = readFileSync(resolve(filePath), 'utf8')

        return new Workflow<Input, Secrets, Output>(source, options)
    }

    executionId(fallbackExecutionId: string): string {
        return this.runOptions.executionId ?? fallbackExecutionId
    }

    requestId(): string | undefined {
        return this.runOptions.requestId
    }

    scopedTools(): Tool[] {
        return [ ...this.scopedToolsByName.values() ]
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

    private resolveCustomTools(tools: NonNullable<WorkflowOptions<Input, Secrets>['tools']>): CustomToolDeclaration[] {
        return tools.map((toolOrDeclaration) => {
            if (this.isRuntimeTool(toolOrDeclaration)) {
                return toolOrDeclaration.toDeclaration()
            }

            return toolOrDeclaration
        })
    }

    private resolveScopedToolsByName(tools: NonNullable<WorkflowOptions<Input, Secrets>['tools']>): Map<string, Tool> {
        const scopedToolsByName = new Map<string, Tool>()

        for (const toolOrDeclaration of tools) {
            if (!this.isRuntimeTool(toolOrDeclaration)) {
                continue
            }

            scopedToolsByName.set(toolOrDeclaration.name, toolOrDeclaration)
        }

        return scopedToolsByName
    }

    private isRuntimeTool(toolOrDeclaration: CustomToolDeclaration | Tool): toolOrDeclaration is Tool {
        return toolOrDeclaration instanceof Object
            && 'toDeclaration' in toolOrDeclaration
            && typeof toolOrDeclaration.toDeclaration === 'function'
            && 'execute' in toolOrDeclaration
            && typeof toolOrDeclaration.execute === 'function'
    }
}
