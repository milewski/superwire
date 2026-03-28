import { Engine, Workflow } from '../src'

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
    const workflow = new Workflow(`
        provider openai_local {
            driver: "openai"
            endpoint: "http://169.254.83.107:1234/v1"
            api_key: "local-api-key"
            models: ["qwen/qwen3.5-35b-a3b"]
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
            model: openai_local("qwen/qwen3.5-35b-a3b")
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

        const response = await engine.run<StructuredRiskResponse>(workflow, inputPayload)

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
