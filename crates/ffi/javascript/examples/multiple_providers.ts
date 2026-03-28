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

    const workflow = new Workflow(`
        provider ollama {
            driver: "ollama"
            endpoint: secrets.ollama_endpoint
            models: [secrets.ollama_model]
        }

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
            ollama_endpoint: string
            ollama_model: string
        }

        input {
            study_name: string
            interview_notes: [string]
        }

        agent redact_notes {
            model: ollama(secrets.ollama_model)
            prompt: "Redact names, emails, and phone numbers from these interview notes: {{ input.interview_notes }}"
            output: {
                redacted_notes: [string]
                redaction_summary: string
            }
        }

        agent synthesize_insights {
            model: openai(secrets.openai_model)
            prompt: "Study={{ input.study_name }}. Use these redacted notes to produce concise insights and recommendations: {{ agent.redact_notes.redacted_notes }}"
            output: {
                insights: [string]
                recommendations: [string]
            }
        }

        output {
            study_name: input.study_name
            redacted_notes: agent.redact_notes.redacted_notes
            redaction_summary: agent.redact_notes.redaction_summary
            insights: agent.synthesize_insights.insights
            recommendations: agent.synthesize_insights.recommendations
        }
    `)

    const engine = new Engine()

    try {
        const inputPayload: MultiProviderInput = {
            study_name: 'Onboarding friction interviews',
            interview_notes: [
                'Alice from Acme said she was blocked by SSO setup.',
                'bob@example.com reported confusing error text on account linking.',
            ],
        }

        const response = await engine.run<MultiProviderResponse>(workflow, inputPayload, providerSecrets)

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
