/**
 * Context compaction example.
 *
 * Shows the difference between full context (`context(...)`) and compacted context
 * (`compact(...)`) and how each affects downstream agent summarization.
 */
import { Engine, Workflow } from '../src'
import { loadOpenAIProviderSecrets } from './env'

type ContextCompactionInput = {
    product_name: string;
    customer_feedback: string[];
}

type ContextCompactionResponse = {
    analysis: {
        themes: string[];
        risks: string[];
        opportunities: string[];
    };
    full_context: unknown;
    compacted_context: unknown;
    summary_from_full_context: string;
    summary_from_compact_context: {
        summary: string;
        top_actions: string[];
    };
}

async function runContextCompactionExample(): Promise<void> {
    const providerSecrets = loadOpenAIProviderSecrets()

    const engine = new Engine()

    try {
        const inputPayload: ContextCompactionInput = {
            product_name: 'Compass AI',
            customer_feedback: [
                'Teams like the speed but want clearer alert routing.',
                'Users request stronger multilingual response consistency.',
                'Managers want trend summaries by region and segment.',
            ],
        }

        const workflow = Workflow.fromFile('./examples/workflows/context_compaction.ai', {
            inputs: inputPayload,
            secrets: providerSecrets,
        })

        const response = await engine.run<ContextCompactionResponse>(workflow)

        if (await response.isError()) {
            console.error('Error:', await response.error())

            return
        }

        console.log(await response.success())
    } finally {
        engine.close()
    }
}

runContextCompactionExample()
