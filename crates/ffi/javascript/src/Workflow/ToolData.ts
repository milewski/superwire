import { ToolValueBag } from './ToolValueBag'
import type { ToolInput } from '../Contracts/ToolInput'
import type { ToolBounded } from '../Contracts/ToolBounded'

export class ToolData {
    public readonly input: Record<string, unknown> | ToolInput;

    public readonly bounded: Record<string, unknown> | ToolBounded;

    public readonly context: ToolValueBag;

    private readonly inputBag: ToolValueBag;

    private readonly boundedBag: ToolValueBag;

    constructor(
        inputValues: Record<string, unknown> = {},
        boundedValues: Record<string, unknown> = {},
        contextValues: Record<string, unknown> = {},
        inputType?: new (...args: never[]) => ToolInput,
        boundedType?: new (...args: never[]) => ToolBounded,
    ) {
        this.inputBag = new ToolValueBag(inputValues);
        this.boundedBag = new ToolValueBag(boundedValues);

        this.input = inputType !== undefined
            ? this.hydratePayload(inputType, this.inputBag.all())
            : this.inputBag.all();

        this.bounded = boundedType !== undefined
            ? this.hydratePayload(boundedType, this.boundedBag.all())
            : this.boundedBag.all();

        this.context = new ToolValueBag(contextValues);
    }

    inputAll(): Record<string, unknown> {
        if (this.input instanceof ToolValueBag) {
            return this.inputBag.all();
        }

        return this.input as Record<string, unknown>;
    }

    inputValue<K extends string>(key: K, defaultValue?: unknown): unknown {
        return this.inputBag.get(key, defaultValue);
    }

    boundedAll(): Record<string, unknown> {
        if (this.bounded instanceof ToolValueBag) {
            return this.boundedBag.all();
        }

        return this.bounded as Record<string, unknown>;
    }

    boundedValue<K extends string>(key: K, defaultValue?: unknown): unknown {
        return this.boundedBag.get(key, defaultValue);
    }

    contextAll(): Record<string, unknown> {
        return this.context.all();
    }

    contextValue<K extends string>(key: K, defaultValue?: unknown): unknown {
        return this.context.get(key, defaultValue);
    }

    private hydratePayload<T extends ToolInput | ToolBounded>(
        payloadType: new (...args: never[]) => T,
        values: Record<string, unknown>,
    ): T {
        return Object.assign(Object.create(payloadType.prototype), values) as T;
    }
}
