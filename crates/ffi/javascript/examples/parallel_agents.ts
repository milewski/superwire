import { Engine, Workflow } from '../src'

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
    const workflow = new Workflow(`
        provider openai_local {
            driver: "openai"
            endpoint: "http://100.118.249.48:3000/v1"
            api_key: "sk-S2Wcfi5cJhGGhFpTHjHcClDmQoR6IwTx1PNl9cmIZF6Wtuxz"
            models: ["qwen3.5-27b"]
        }

        input {
            product_name: string
            release_highlights: [string]
        }

        agent customer_story {
            model: openai_local("qwen3.5-27b")
            prompt: "Write a customer-facing announcement for {{ input.product_name }} using these highlights: {{ input.release_highlights }}"
            output: {
                headline: string
                summary: string
            }
        }

        agent investor_story {
            model: openai_local("qwen3.5-27b")
            prompt: "Write an investor update for {{ input.product_name }} based on: {{ input.release_highlights }}"
            output: {
                thesis: string
                growth_drivers: [string]
            }
        }

        agent social_snippets {
            model: openai_local("qwen3.5-27b")
            prompt: "Generate 3 short social posts for {{ input.product_name }} from: {{ input.release_highlights }}"
            output: {
                posts: [string; 3]
            }
        }

        agent review {
            model: openai_local("qwen3.5-27b")
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

        const response = await engine.run<ParallelNarrativesResponse>(workflow, inputPayload)

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
