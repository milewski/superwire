/**
 * Agent for-loop example.
 *
 * Shows iterative agent execution with `for ... in` loops over arrays,
 * then aggregation of per-item outputs in the final workflow result.
 */
import { Engine, Workflow } from '../src'
import { loadOpenAIProviderSecrets } from './env'

type PersonaCampaignInput = {
    product_name: string;
    personas: string[];
}

type PersonaCampaignResponse = {
    persona_campaigns: Array<{
        persona: string;
        tagline: string;
        key_benefits: string[];
    }>;
    persona_scores: Array<{
        persona: string;
        score: number;
        rationale: string;
    }>;
    execution_context: unknown;
}

async function runAgentForLoopExample(): Promise<void> {
    const providerSecrets = loadOpenAIProviderSecrets()

    const engine = new Engine()

    try {
        const inputPayload: PersonaCampaignInput = {
            product_name: 'Compass AI',
            personas: [
                'Engineering manager at a startup',
                'Enterprise compliance lead',
                'Customer support team lead',
            ],
        }

        const workflow = Workflow.fromFile('./examples/workflows/agent_for_loop.ai', {
            inputs: inputPayload,
            secrets: providerSecrets,
        })

        const response = await engine.run<PersonaCampaignResponse>(workflow)

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

runAgentForLoopExample()
