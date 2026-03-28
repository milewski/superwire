import type { JsonSchema } from './schema'
import type { CustomToolDeclaration, JsonRecord } from './types'

export interface ToolExecutionContext {
    workflowInput?: unknown;
    boundArguments?: unknown;
    [key: string]: unknown;
}

export interface ToolArguments<
    ToolInput extends JsonRecord = JsonRecord,
    ToolBoundedInput extends JsonRecord = JsonRecord,
    ToolContext extends ToolExecutionContext = ToolExecutionContext,
> {
    input: ToolInput;
    bounded: ToolBoundedInput;
    context: ToolContext;
}

export abstract class Tool<
    ToolInput extends JsonRecord = JsonRecord,
    ToolOutput = unknown,
    ToolBoundedInput extends JsonRecord = JsonRecord,
    ToolContext extends ToolExecutionContext = ToolExecutionContext,
> {
    readonly name: string

    abstract readonly description: string

    abstract readonly inputSchema: JsonSchema

    readonly outputSchema?: JsonSchema

    constructor(name?: string) {
        this.name = name ?? this.constructor.name
    }

    abstract execute(toolArguments: ToolArguments<ToolInput, ToolBoundedInput, ToolContext>): ToolOutput | Promise<ToolOutput>

    toDeclaration(): CustomToolDeclaration {
        return {
            name: this.name,
            description: this.description,
            input_schema: this.inputSchema,
            output_schema: this.outputSchema,
        }
    }
}
