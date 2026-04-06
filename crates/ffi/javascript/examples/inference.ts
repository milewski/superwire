/**
 * Inference settings example.
 *
 * Shows how to configure model inference options (temperature, max tokens)
 * directly in workflow DSL while keeping runtime execution in TypeScript simple.
 */
import { Engine, Workflow } from '../src'
import { loadOpenAIProviderSecrets } from './env'

type ReleaseReadinessInput = {
    release_scope: string;
}

type ReleaseReadinessResponse = {
    note: string;
}

async function runInferenceExample(): Promise<void> {
    const providerSecrets = loadOpenAIProviderSecrets()

    const engine = new Engine()

    try {
        const inputPayload: ReleaseReadinessInput = {
            release_scope: 'Checkout reliability and retry handling improvements',
        }

        const workflow = Workflow.fromFile('./examples/workflows/inference.wire', {
            inputs: inputPayload,
            secrets: providerSecrets,
        })

        const response = await engine.run<ReleaseReadinessResponse>(workflow)

        if (await response.isError()) {
            console.error('Error:', await response.error())

            return
        }

        console.log(await response.success())
    } finally {
        engine.close()
    }
}

runInferenceExample()
