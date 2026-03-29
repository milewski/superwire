/**
 * Parallel agents example.
 *
 * Shows independent agent branches that can run concurrently, followed by
 * a review agent that checks consistency across all branch outputs.
 */
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

        const workflow = Workflow.fromFile('./examples/workflows/parallel_agents.ai', {
            inputs: inputPayload,
            secrets: providerSecrets,
        })

        const response = await engine.run<ParallelNarrativesResponse>(workflow)

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
