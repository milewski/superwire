import { Engine, Workflow } from '../src'
import { loadOpenAIProviderSecrets } from './env'

type StructuredOutputResponse = {
    version: number;
    generated_by: string;
    report: {
        source: string;
        overview: {
            text: string;
        };
        metrics: {
            confidence: number;
            status: string;
        };
    };
}

async function runStructuredOutputExample(): Promise<void> {
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

        agent summary {
            model: openai(secrets.openai_model)
            prompt: "Write a short project status summary and confidence score."
            output: {
                text: string
                confidence: number
            }
        }

        output {
            version: 2
            generated_by: "status_summary_workflow"
            report: {
                source: "workflow"
                overview: {
                    text: agent.summary.text
                }
                metrics: {
                    confidence: agent.summary.confidence
                    status: "ok"
                }
            }
        }
    `)

    const engine = new Engine()

    try {
        const response = await engine.run<StructuredOutputResponse>(workflow, {}, providerSecrets)

        if (await response.isError()) {
            console.error('Error:', await response.error())

            return
        }

        console.log(await response.success())
    } finally {
        engine.close()
    }
}

runStructuredOutputExample()
