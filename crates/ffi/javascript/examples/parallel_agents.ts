import { Engine, Workflow } from '../src'
import { loadOpenAIProviderSecrets } from './env'

type ParallelNarrativesInput = {
    product_name: string;
    release_highlights: string[];
}

type ParallelNarrativesResponse = {
    customer_story: {
        headline: string;
        summary: string;
    };
    investor_story: {
        thesis: string;
        growth_drivers: string[];
    };
    social_snippets: {
        posts: string[];
    };
    review: {
        approved: boolean;
        concerns: string[];
    };
}

async function runParallelAgentsExample(): Promise<void> {
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
            product_name: string
            release_highlights: [string]
        }

        agent customer_story {
            model: openai(secrets.openai_model)
            prompt: "Write a customer-facing announcement for {{ input.product_name }} using these highlights: {{ input.release_highlights }}"
            output: {
                headline: string
                summary: string
            }
        }

        agent investor_story {
            model: openai(secrets.openai_model)
            prompt: "Write an investor update for {{ input.product_name }} based on: {{ input.release_highlights }}"
            output: {
                thesis: string
                growth_drivers: [string]
            }
        }

        agent social_snippets {
            model: openai(secrets.openai_model)
            prompt: "Generate 3 short social posts for {{ input.product_name }} from: {{ input.release_highlights }}"
            output: {
                posts: [string; 3]
            }
        }

        agent review {
            model: openai(secrets.openai_model)
            prompt: "Check consistency across customer={{ agent.customer_story }} investor={{ agent.investor_story }} social={{ agent.social_snippets }}"
            output: {
                approved: boolean
                concerns: [string]
            }
        }

        output {
            customer_story: agent.customer_story
            investor_story: agent.investor_story
            social_snippets: agent.social_snippets
            review: agent.review
        }
    `)

    const engine = new Engine()

    try {
        const inputPayload: ParallelNarrativesInput = {
            product_name: 'Compass AI',
            release_highlights: [
                'faster onboarding with one-click setup',
                'new dashboard for team metrics',
                'improved response quality in multilingual prompts',
            ],
        }

        const response = await engine.run<ParallelNarrativesResponse>(workflow, inputPayload, providerSecrets)

        if (await response.isError()) {
            const executionError = await response.error()
            console.error('Error:', executionError)

            return
        }

        console.log(await response.success())
    } finally {
        engine.close()
    }
}

runParallelAgentsExample()
