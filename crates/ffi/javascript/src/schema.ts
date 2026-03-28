export type JsonSchema = Record<string, unknown>;

export type JsonSchemaPrimitive = string | number | boolean | null;

export interface JsonSchemaObjectProperties {
    [propertyName: string]: JsonSchema;
}

export interface JsonSchemaObjectOptions {
    required?: string[];
    additionalProperties?: boolean;
}

export interface JsonSchemaArrayOptions {
    minItems?: number;
    maxItems?: number;
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

    integer(): JsonSchema {
        return { type: 'integer' };
    },

    null(): JsonSchema {
        return { type: 'null' };
    },

    literal(value: JsonSchemaPrimitive): JsonSchema {
        return { const: value };
    },

    enumeration(values: JsonSchemaPrimitive[]): JsonSchema {
        return { enum: values };
    },

    array(items: JsonSchema, options: JsonSchemaArrayOptions = {}): JsonSchema {
        const arraySchema: JsonSchema = {
            type: 'array',
            items,
        };

        if (options.minItems !== undefined) {
            arraySchema.minItems = options.minItems;
        }

        if (options.maxItems !== undefined) {
            arraySchema.maxItems = options.maxItems;
        }

        return arraySchema;
    },

    fixedArray(items: JsonSchema, size: number): JsonSchema {
        return this.array(items, {
            minItems: size,
            maxItems: size,
        });
    },

    tuple(items: JsonSchema[]): JsonSchema {
        return {
            type: 'array',
            prefixItems: items,
            minItems: items.length,
            maxItems: items.length,
        };
    },

    union(variants: JsonSchema[]): JsonSchema {
        return {
            anyOf: variants,
        };
    },

    nullable(inner: JsonSchema): JsonSchema {
        return this.union([ inner, this.null() ]);
    },

    object(
        properties: JsonSchemaObjectProperties,
        requiredOrOptions: string[] | JsonSchemaObjectOptions = Object.keys(properties),
    ): JsonSchema {
        const objectOptions: JsonSchemaObjectOptions = Array.isArray(requiredOrOptions)
            ? {
                required: requiredOrOptions,
            }
            : requiredOrOptions;

        return {
            type: 'object',
            properties,
            required: objectOptions.required ?? Object.keys(properties),
            additionalProperties: objectOptions.additionalProperties ?? false,
        };
    },
};
