export class ToolValueBag {
    private readonly values: Record<string, unknown>

    constructor(values: Record<string, unknown> = {}) {
        this.values = values
    }

    all(): Record<string, unknown> {
        return this.values
    }

    has(key: string): boolean {
        return key in this.values
    }

    get(key: string, defaultValue?: unknown): unknown {
        return this.has(key) ? this.values[ key ] : defaultValue
    }

    string(key: string, defaultValue?: string): string | undefined {
        return this.expectType(key, 'string', defaultValue, (value): value is string => typeof value === 'string')
    }

    integer(key: string, defaultValue?: number): number | undefined {
        return this.expectType(key, 'integer', defaultValue, (value): value is number => Number.isInteger(value))
    }

    number(key: string, defaultValue?: number): number | undefined {
        return this.expectType(key, 'number', defaultValue, (value): value is number => typeof value === 'number')
    }

    boolean(key: string, defaultValue?: boolean): boolean | undefined {
        return this.expectType(key, 'boolean', defaultValue, (value): value is boolean => typeof value === 'boolean')
    }

    array<T = unknown>(key: string, defaultValue?: T[]): T[] | undefined {
        if (!this.has(key)) {
            return defaultValue
        }

        const value = this.values[ key ]

        if (Array.isArray(value)) {
            return value as T[]
        }

        const receivedType = typeof value

        throw new Error(`Expected key \`${ key }\` to be array, got ${ receivedType }.`)
    }

    private expectType<T>(
        key: string,
        _expectedType: string,
        defaultValue: T | undefined,
        guard: (value: unknown) => value is T,
    ): T | undefined {
        if (!this.has(key)) {
            return defaultValue
        }

        const value = this.values[ key ]

        if (guard(value)) {
            return value
        }

        const receivedType = typeof value

        throw new Error(`Expected key \`${ key }\` to be ${ _expectedType }, got ${ receivedType }.`)
    }
}

export class ToolOutputNormalizer {
    static normalize(value: unknown): unknown {
        if (value === null || value === undefined || typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean') {
            return value
        }

        if (value instanceof Date) {
            return value.toISOString()
        }

        if (Array.isArray(value)) {
            return value.map((item) => ToolOutputNormalizer.normalize(item))
        }

        if (typeof value === 'object') {
            if ('toJSON' in value && typeof value.toJSON === 'function') {
                return ToolOutputNormalizer.normalize(value.toJSON())
            }

            if ('toArray' in value && typeof value.toArray === 'function') {
                const arrayValue = value.toArray()

                if (Array.isArray(arrayValue)) {
                    return ToolOutputNormalizer.normalize(arrayValue)
                }
            }

            if (Symbol.iterator in Object(value)) {
                return ToolOutputNormalizer.normalize(Array.from(value as Iterable<unknown>))
            }

            const obj: Record<string, unknown> = {}

            for (const key of Object.keys(value as object)) {
                obj[ key ] = ToolOutputNormalizer.normalize((value as Record<string, unknown>)[ key ])
            }

            return obj
        }

        return value
    }
}
