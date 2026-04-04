import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

import type { CustomToolDeclaration, JsonRecord, WorkflowExecutionRequest } from '../types'
import { Tool } from './Tool'

type WorkflowOptions<Input extends JsonRecord, Secrets extends JsonRecord> = {
    inputs?: Input;
    secrets?: Secrets;
    tools?: Tool[];
    requestId?: string;
    executionId?: string;
}

export class Workflow<
    Input extends JsonRecord = JsonRecord,
    Secrets extends JsonRecord = JsonRecord,
    Output = unknown,
> {
    readonly source: string

    readonly inputPayload: Input

    readonly secretsPayload?: Secrets

    readonly customTools: CustomToolDeclaration[]

    readonly scopedToolsMap: Map<string, Tool>

    readonly runOptions: {
        requestId: string | undefined;
        executionId: string | undefined;
    }

    constructor(
        source: string,
        inputsOrOptions: Input | WorkflowOptions<Input, Secrets> = {} as Input,
        secrets: Secrets | undefined = undefined,
        toolInstances: Tool[] = [],
        requestId: string | undefined = undefined,
        executionId: string | undefined = undefined,
    ) {
        let normalizedInputs: Input
        let normalizedSecrets: Secrets | undefined
        let normalizedTools: Tool[]
        let normalizedRequestId: string | undefined
        let normalizedExecutionId: string | undefined

        if (this.isWorkflowOptions(inputsOrOptions)) {
            normalizedInputs = inputsOrOptions.inputs ?? {} as Input
            normalizedSecrets = inputsOrOptions.secrets
            normalizedTools = this.normalizeTools(inputsOrOptions.tools ?? [])
            normalizedRequestId = inputsOrOptions.requestId
            normalizedExecutionId = inputsOrOptions.executionId
        } else {
            normalizedInputs = inputsOrOptions
            normalizedSecrets = secrets
            normalizedTools = this.normalizeTools(toolInstances)
            normalizedRequestId = requestId
            normalizedExecutionId = executionId
        }

        this.source = source
        this.inputPayload = normalizedInputs
        this.secretsPayload = normalizedSecrets
        this.customTools = this.resolveCustomTools(normalizedTools)
        this.scopedToolsMap = this.resolveScopedToolsByName(normalizedTools)
        this.runOptions = {
            requestId: normalizedRequestId,
            executionId: normalizedExecutionId,
        }
    }

    static fromFile<
        Output = unknown,
        Input extends JsonRecord = JsonRecord,
        Secrets extends JsonRecord = JsonRecord,
    >(
        filePath: string,
        inputsOrOptions: Input | WorkflowOptions<Input, Secrets> = {} as Input,
        secrets: Secrets | undefined = undefined,
        toolInstances: Tool[] = [],
        requestId: string | undefined = undefined,
        executionId: string | undefined = undefined,
    ): Workflow<Input, Secrets, Output> {
        const resolvedPath = resolve(filePath)
        const source = readFileSync(resolvedPath, 'utf8')

        return new Workflow<Input, Secrets, Output>(source, inputsOrOptions, secrets, toolInstances, requestId, executionId)
    }

    private isWorkflowOptions(value: unknown): value is WorkflowOptions<Input, Secrets> {
        if (typeof value !== 'object' || value === null) {
            return false
        }

        const keys = Object.keys(value as object)

        return keys.includes('inputs')
            || keys.includes('secrets')
            || keys.includes('tools')
            || keys.includes('requestId')
            || keys.includes('executionId')
    }

    executionId(fallbackExecutionId: string): string {
        return this.runOptions.executionId ?? fallbackExecutionId
    }

    requestId(): string | undefined {
        return this.runOptions.requestId
    }

    scopedTools(): Tool[] {
        return [ ...this.scopedToolsMap.values() ]
    }

    scopedToolsByName(): Map<string, Tool> {
        return this.scopedToolsMap
    }

    toExecutionRequest(
        executionId: string,
        inputPayload: Input | null = null,
        secretsPayload: Secrets | undefined = undefined,
    ): WorkflowExecutionRequest<Input> {
        const workflowExecutionRequest: WorkflowExecutionRequest<Input> = {
            execution_id: executionId,
            workflow_source: this.source,
            input: {
                payload: inputPayload ?? this.inputPayload,
            },
            custom_tools: this.customTools,
        }

        const resolvedSecretsPayload = secretsPayload ?? this.secretsPayload

        if (resolvedSecretsPayload !== undefined) {
            workflowExecutionRequest.secrets = {
                payload: resolvedSecretsPayload,
            }
        }

        return workflowExecutionRequest
    }

    private normalizeTools(toolList: Tool[]): Tool[] {
        return toolList
    }

    private resolveCustomTools(tools: Tool[]): CustomToolDeclaration[] {
        return tools.map((tool) => tool.toDeclaration())
    }

    private resolveScopedToolsByName(tools: Tool[]): Map<string, Tool> {
        const scopedToolsMap = new Map<string, Tool>()

        for (const tool of tools) {
            scopedToolsMap.set(tool.name, tool)
        }

        return scopedToolsMap
    }
}
