import type { JsonSchema } from '../Schema/schema'
import type { CustomToolDeclaration, JsonRecord } from '../types'
import { ToolData } from './ToolData'
import { ToolOutputNormalizer } from './ToolValueBag'
import type { ToolInput } from '../Contracts/ToolInput'
import type { ToolBounded } from '../Contracts/ToolBounded'

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
    static readonly TOOL_NAME?: string;

    abstract readonly description: string;

    abstract readonly inputSchema: JsonSchema;

    readonly outputSchema?: JsonSchema;

    readonly name: string;

    constructor(name?: string) {
        this.name = name ?? this.resolveToolName();
    }

    abstract execute(toolArguments: ToolArguments<ToolInput, ToolBoundedInput, ToolContext>): ToolOutput | Promise<ToolOutput>;

    invoke(toolData: ToolData): unknown {
        const toolArguments: ToolArguments<ToolInput, ToolBoundedInput, ToolContext> = {
            input: toolData.inputAll() as ToolInput,
            bounded: toolData.boundedAll() as ToolBoundedInput,
            context: toolData.contextAll() as ToolContext,
        };

        const output = this.execute(toolArguments);

        return ToolOutputNormalizer.normalize(output);
    }

    executeForTesting(
        input: Record<string, unknown> = {},
        bounded: Record<string, unknown> = {},
        context: Record<string, unknown> = {},
    ): ToolOutput {
        const toolData = new ToolData(
            input,
            bounded,
            context,
        );

        const executeMethod = this.execute;
        const executeArguments: ToolArguments<ToolInput, ToolBoundedInput, ToolContext> = {
            input: toolData.inputAll() as ToolInput,
            bounded: toolData.boundedAll() as ToolBoundedInput,
            context: toolData.contextAll() as ToolContext,
        };

        const output = executeMethod.call(this, executeArguments);

        return ToolOutputNormalizer.normalize(output) as ToolOutput;
    }

    invokeForTesting(
        input: Record<string, unknown> = {},
        bounded: Record<string, unknown> = {},
        context: Record<string, unknown> = {},
    ): unknown {
        return this.executeForTesting(input, bounded, context);
    }

    toDeclaration(): CustomToolDeclaration {
        const declaration: CustomToolDeclaration = {
            name: this.name,
            description: this.description,
            input_schema: this.inputSchema,
        };

        if (this.outputSchema !== undefined) {
            declaration.output_schema = this.outputSchema;
        }

        return declaration;
    }

    private resolveToolName(): string {
        const toolConstructor = this.constructor as typeof Tool;
        const staticToolName = toolConstructor.TOOL_NAME?.trim();

        if (staticToolName) {
            return staticToolName;
        }

        return this.deriveToolNameFromClassName();
    }

    private deriveToolNameFromClassName(): string {
        const className = this.constructor.name.trim();

        if (!className) {
            throw new Error(
                'Tool name could not be inferred from class name. Provide a static `TOOL_NAME` or pass a name to `super(name)`.',
            );
        }

        const normalizedName = className
            .replace(/([a-z0-9])([A-Z])/gu, '$1_$2')
            .replace(/[-\s]+/gu, '_')
            .toLowerCase();

        if (!normalizedName) {
            throw new Error(
                'Tool name normalization produced an empty value. Provide a static `TOOL_NAME` or pass a name to `super(name)`.',
            );
        }

        return normalizedName;
    }
}
