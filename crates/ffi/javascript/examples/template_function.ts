/**
 * Template function example.
 *
 * Shows how to keep prompts in external markdown files and render them with
 * runtime data using the DSL `template(...)` helper.
 */
import { Engine, Workflow } from '../src'
import { loadOpenAIProviderSecrets } from './env'

type TemplateFunctionInput = {
    study_name: string;
    audience: string;
    findings: string[];
}

type TemplateFunctionResponse = {
    summary: string;
    top_actions: string[];
}

async function runTemplateFunctionExample(): Promise<void> {
    const providerSecrets = loadOpenAIProviderSecrets()

    const engine = new Engine()

    try {
        const inputPayload: TemplateFunctionInput = {
            study_name: 'Trial activation deep dive',
            audience: 'Product leadership',
            findings: [
                'Activation improves when setup starts with prefilled defaults.',
                'Confusion spikes around permission scopes.',
                'Users trust rollout when example data is included.',
            ],
        }

        const workflow = Workflow.fromFile('./examples/workflows/template_function.wire', {
            inputs: inputPayload,
            secrets: providerSecrets,
        })

        const response = await engine.run<TemplateFunctionResponse>(workflow)

        if (await response.isError()) {
            console.error('Error:', await response.error())

            return
        }

        console.log(await response.success())
    } finally {
        engine.close()
    }
}

runTemplateFunctionExample()
