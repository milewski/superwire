import type { JsonSchema } from './schema'
import type { CustomToolDeclaration, JsonRecord } from './types'

export abstract class Tool<Input extends JsonRecord = JsonRecord, Output = unknown> {
    readonly name: string

    abstract readonly description: string

    abstract readonly inputSchema: JsonSchema

    readonly outputSchema?: JsonSchema

    constructor(name?: string) {
        this.name = name ?? this.constructor.name
    }

    abstract execute(input: Input): Output | Promise<Output>

    toDeclaration(): CustomToolDeclaration {
        return {
            name: this.name,
            description: this.description,
            input_schema: this.inputSchema,
            output_schema: this.outputSchema,
        }
    }
}
