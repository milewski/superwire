import { Engine, Workflow } from '../src'
import { loadOpenAIProviderSecrets } from './env'

type TemplateFunctionInput = {
    study_name: string;
    audience: string;
    findings: string[];
}

type TemplateFunctionResponse = {
    summary: string;
    top_actions: string[];
}

async function runTemplateFunctionExample(): Promise<void> {
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
            study_name: string
            audience: string
            findings: [string]
        }

        agent research_brief {
            model: openai(secrets.openai_model)
            prompt: template("examples/research_brief_prompt.md", {
                study_name: input.study_name
                audience: input.audience
                findings: input.findings
            })
            output: {
                summary: string
                top_actions: [string; 3]
            }
        }

        output {
            summary: agent.research_brief.summary
            top_actions: agent.research_brief.top_actions
        }
    `)

    const engine = new Engine()

    try {
        const inputPayload: TemplateFunctionInput = {
            study_name: 'Trial activation deep dive',
            audience: 'Product leadership',
            findings: [
                'Activation improves when setup starts with prefilled defaults.',
                'Confusion spikes around permission scopes.',
                'Users trust rollout when example data is included.',
            ],
        }

        const response = await engine.run<TemplateFunctionResponse>(workflow, inputPayload, providerSecrets)

        if (await response.isError()) {
            console.error('Error:', await response.error())

            return
        }

        console.log(await response.success())
    } finally {
        engine.close()
    }
}

runTemplateFunctionExample()
