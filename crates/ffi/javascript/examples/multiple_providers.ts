/**
 * Multiple providers example.
 *
 * Shows one workflow that uses different providers for different stages:
 * local redaction with Ollama and synthesis with OpenAI.
 */
import { Engine, Workflow } from '../src'
import { loadOllamaProviderSecrets, loadOpenAIProviderSecrets } from './env'

type MultiProviderInput = {
    study_name: string;
    interview_notes: string[];
}

type MultiProviderResponse = {
    study_name: string;
    redacted_notes: string[];
    redaction_summary: string;
    insights: string[];
    recommendations: string[];
}

async function runMultipleProvidersExample(): Promise<void> {
    const providerSecrets = {
        ...loadOpenAIProviderSecrets(),
        ...loadOllamaProviderSecrets(),
    }

    const engine = new Engine()

    try {
        const inputPayload: MultiProviderInput = {
            study_name: 'Onboarding friction interviews',
            interview_notes: [
                'Alice from Acme said she was blocked by SSO setup.',
                'bob@example.com reported confusing error text on account linking.',
            ],
        }

        const workflow = Workflow.fromFile('./examples/workflows/multiple_providers.wire', {
            inputs: inputPayload,
            secrets: providerSecrets,
        })

        const response = await engine.run<MultiProviderResponse>(workflow)

        if (await response.isError()) {
            console.error('Error:', await response.error())

            return
        }

        console.log(await response.success())
    } finally {
        engine.close()
    }
}

runMultipleProvidersExample()
