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

    const workflow = new Workflow(`
        provider openai {
            driver: "openai"
            endpoint: secrets.openai_endpoint
            api_key: secrets.openai_api_key
            models: [input.models.large, input.models.small]
        }

        secrets {
            openai_endpoint: string
            openai_api_key: string
        }

        input {
            request: string
            models: {
                large: string
                small: string
            }
        }

        agent router {
            model: openai(input.models.small)
            prompt: """
                Choose the best model for this request: {{ input.request }}. 
                You must pick exactly one model from [{{ input.models.large }}, {{ input.models.small }}].
            """
            output: {
                model: input.models.large | input.models.small
                rationale: string
            }
        }

        agent specialist {
            model: openai(agent.router.model)
            prompt: "Answer this request in under 180 words: {{ input.request }}"
            output: string
        }

        output {
            routing: agent.router
            response: agent.specialist
        }
    `)

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

        const response = await engine.run<ModelRoutingResponse>(workflow, inputPayload, secretsPayload)

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
