/**
 * Structured output example.
 *
 * Shows how workflow outputs can produce a stable nested JSON shape with
 * constants and model-produced values combined in one response object.
 */
import { Engine, Workflow } from '../src'
import { loadOpenAIProviderSecrets } from './env'

type StructuredOutputResponse = {
    version: number;
    generated_by: string;
    report: {
        source: string;
        overview: {
            text: string;
        };
        metrics: {
            confidence: number;
            status: string;
        };
    };
}

async function runStructuredOutputExample(): Promise<void> {
    const providerSecrets = loadOpenAIProviderSecrets()

    const engine = new Engine()

    try {
        const workflow = Workflow.fromFile('./examples/workflows/structured_output.wire', {
            inputs: {},
            secrets: providerSecrets,
        })

        const response = await engine.run<StructuredOutputResponse>(workflow)

        if (await response.isError()) {
            console.error('Error:', await response.error())

            return
        }

        console.log(await response.success())
    } finally {
        engine.close()
    }
}

runStructuredOutputExample()
