/**
 * Model routing example.
 *
 * Shows a small router agent choosing which model to use, followed by a
 * specialist agent that runs with the selected model.
 */
import { Engine, Workflow } from '../src'
import { loadOpenAIProviderSecrets } from './env'

type ModelRoutingInput = {
    request: string;
    models: {
        large: string,
        small: string,
    }
}

type ModelRoutingResponse = {
    routing: {
        model: string;
        rationale: string;
    };
    response: string
}

async function runModelRoutingBetweenAgentsExample(): Promise<void> {
    const providerSecrets = loadOpenAIProviderSecrets()

    const engine = new Engine()

    try {
        const inputPayload: ModelRoutingInput = {
            request: 'Draft a migration plan from a monolith to services with rollout risks and mitigations.',
            models: {
                large: 'qwen3.5-27b',
                small: 'qwen3.5-9b',
            },
        }

        const secretsPayload = {
            openai_endpoint: providerSecrets.openai_endpoint,
            openai_api_key: providerSecrets.openai_api_key,
        }

        const workflow = Workflow.fromFile('./examples/workflows/model_routing_between_agents.ai', {
            inputs: inputPayload,
            secrets: secretsPayload,
        })

        const response = await engine.run<ModelRoutingResponse>(workflow)

        if (await response.isError()) {
            console.error('Error:', await response.error())

            return
        }

        console.log(await response.success())
    } finally {
        engine.close()
    }
}

runModelRoutingBetweenAgentsExample()
