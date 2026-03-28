import type { JsonSchema } from './schema'
import type { CustomToolDeclaration, JsonRecord } from './types'

export interface ToolExecutionContext {
    workflowInput?: unknown;
    boundArguments?: unknown;

    [ key: string ]: unknown;
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
    static readonly toolName?: string

    readonly name: string

    abstract readonly description: string

    abstract readonly inputSchema: JsonSchema

    readonly outputSchema?: JsonSchema

    constructor(name?: string) {
        this.name = name ?? this.resolveToolName()
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

    private resolveToolName(): string {
        const toolConstructor = this.constructor as typeof Tool
        const staticToolName = toolConstructor.toolName?.trim()

        if (staticToolName) {
            return staticToolName
        }

        return this.deriveToolNameFromClassName()
    }

    private deriveToolNameFromClassName(): string {
        const className = this.constructor.name.trim()

        if (!className) {
            throw new Error(
                'Tool name could not be inferred from class name. Provide a static `toolName` or pass a name to `super(name)`.',
            )
        }

        const normalizedName = className
            .replace(/([a-z0-9])([A-Z])/gu, '$1_$2')
            .replace(/[-\s]+/gu, '_')
            .toLowerCase()

        if (!normalizedName) {
            throw new Error(
                'Tool name normalization produced an empty value. Provide a static `toolName` or pass a name to `super(name)`.',
            )
        }

        return normalizedName
    }
}
