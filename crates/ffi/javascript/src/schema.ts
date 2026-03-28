export type JsonSchema = Record<string, unknown>;

export interface JsonSchemaObjectProperties {
    [propertyName: string]: JsonSchema;
}

export const schema = {
    string(): JsonSchema {
        return { type: 'string' };
    },

    number(): JsonSchema {
        return { type: 'number' };
    },

    boolean(): JsonSchema {
        return { type: 'boolean' };
    },

    object(properties: JsonSchemaObjectProperties, required: string[] = Object.keys(properties)): JsonSchema {
        return {
            type: 'object',
            properties,
            required,
            additionalProperties: false,
        };
    },
};
