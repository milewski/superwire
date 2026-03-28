import { Engine, Workflow } from '../src'

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
    const workflow = new Workflow(`
        provider openai_local {
            driver: "openai"
            endpoint: "http://169.254.83.107:1234/v1"
            api_key: "local-api-key"
            models: ["qwen/qwen3.5-35b-a3b"]
        }

        input {
            product_name: string
            personas: [string]
        }

        agent persona_campaign for persona in input.personas {
            model: openai_local("qwen/qwen3.5-35b-a3b")
            prompt: "Create a short campaign for {{ input.product_name }} targeted at persona: {{ persona }}"
            output: {
                persona: string
                tagline: string
                key_benefits: [string; 3]
            }
        }

        agent persona_score for campaign in agent.persona_campaign {
            model: openai_local("qwen/qwen3.5-35b-a3b")
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

        const response = await engine.run<PersonaCampaignResponse>(workflow, inputPayload)

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
