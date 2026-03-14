import { Tool } from './index.js'
import { zodToJsonSchema } from 'zod-to-json-schema'
import { z, ZodType, ZodTypeAny } from 'zod'

// Helper to convert Zod schema to JSON schema without deep type instantiation
function toJsonSchema(schema: ZodType, name: string): string {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    return JSON.stringify(zodToJsonSchema(schema as any, name))
}

/**
 * Create a tool with Zod schema validation
 */
export function createTool<T extends ZodType>(
    name: string,
    description: string,
    zodSchema: T,
    executeFn: (params: z.infer<T>) => unknown,
): Tool {
    return new Tool(
        name,
        description,
        toJsonSchema(zodSchema, name),
        (_err: Error | null, paramsJson: string): string => {
            const params = JSON.parse(paramsJson)
            const validated = zodSchema.parse(params)
            const result = executeFn(validated)
            return JSON.stringify(result)
        },
    )
}

/**
 * Base class for creating tools with Zod schemas
 */
export class ZodTool<T extends ZodTypeAny = ZodTypeAny> extends Tool {
    constructor(
        name: string,
        description: string,
        zodSchema: T,
        executeFn: (params: z.infer<T>) => unknown,
    ) {
        super(
            name,
            description,
            toJsonSchema(zodSchema, name),
            (_: Error | null, paramsJson: string): string => {
                const params = JSON.parse(paramsJson)
                const validated = zodSchema.parse(params)
                const result = executeFn(validated)
                return JSON.stringify(result)
            },
        )
    }
}
