import { Engine, Workflow } from '../src'
import { loadOpenAIProviderSecrets } from './env'

type StructuredRiskInput = {
    initiative_name: string;
    change_notes: string;
}

type StructuredRiskResponse = {
    report: {
        overview: string;
        risks: Array<{
            title: string;
            severity: 'low' | 'medium' | 'high';
            owner: string | null;
            mitigations: string[];
        }>;
    };
    overview: string;
}

async function runSchemaTypesExample(): Promise<void> {
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

        schema RiskItem {
            title: string
            severity: "low" | "medium" | "high"
            owner: string | null
            mitigations: [string]
        }

        schema RiskReport {
            overview: string
            risks: [schema.RiskItem]
        }

        input {
            initiative_name: string
            change_notes: string
        }

        agent risk_report {
            model: openai(secrets.openai_model)
            prompt: "Analyze the rollout risks for {{ input.initiative_name }} with details: {{ input.change_notes }}"
            output: schema.RiskReport
        }

        output {
            report: agent.risk_report
            overview: agent.risk_report.overview
        }
    `)

    const engine = new Engine()

    try {
        const inputPayload: StructuredRiskInput = {
            initiative_name: 'Customer Segmentation Rewrite',
            change_notes: 'We are replacing legacy segmentation rules with model-driven scoring and deploying to all regions in one week.',
        }

        const response = await engine.run<StructuredRiskResponse>(workflow, inputPayload, providerSecrets)

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

runSchemaTypesExample()
