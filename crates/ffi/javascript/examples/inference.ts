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

    const workflow = new Workflow(`
        provider openai {
            driver: "openai"
            endpoint: secrets.openai_endpoint
            api_key: secrets.openai_api_key
            models: [secrets.openai_model]
        }

        secrets {
            openai_endpoint: string
            openai_api_key: string
            openai_model: string
        }

        input {
            release_scope: string
        }

        agent release_analyst {
            model: openai(secrets.openai_model)

            inference: {
                temperature: 0.2
                max_tokens: 1_500
            }

            prompt: "Write a short release-readiness note for: {{ input.release_scope }}"
            output: string
        }

        output {
            note: agent.release_analyst
        }
    `)

    const engine = new Engine()

    try {
        const inputPayload: ReleaseReadinessInput = {
            release_scope: 'Checkout reliability and retry handling improvements',
        }

        const response = await engine.run<ReleaseReadinessResponse>(workflow, inputPayload, providerSecrets)

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
