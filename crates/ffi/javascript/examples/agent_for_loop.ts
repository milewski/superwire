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
            personas: [string]
        }

        agent persona_campaign for persona in input.personas {
            model: openai(secrets.openai_model)
            prompt: "Create a short campaign for {{ input.product_name }} targeted at persona: {{ persona }}"
            output: {
                persona: string
                tagline: string
                key_benefits: [string; 3]
            }
        }

        agent persona_score for campaign in agent.persona_campaign {
            model: openai(secrets.openai_model)
            prompt: "Score this campaign from 1 to 10 and explain briefly: {{ campaign }}"
            output: {
                persona: string
                score: number
                rationale: string
            }
        }

        output {
            persona_campaigns: agent.persona_campaign
            persona_scores: agent.persona_score
            execution_context: context(agent.persona_score)
        }
    `)

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

        const response = await engine.run<PersonaCampaignResponse>(workflow, inputPayload, providerSecrets)

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
