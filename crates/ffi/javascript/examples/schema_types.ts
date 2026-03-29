/**
 * DSL schema type example.
 *
 * Shows how reusable DSL `schema` declarations can define output types for
 * agents and keep large structured outputs consistent.
 */
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

    const engine = new Engine()

    try {
        const inputPayload: StructuredRiskInput = {
            initiative_name: 'Customer Segmentation Rewrite',
            change_notes: 'We are replacing legacy segmentation rules with model-driven scoring and deploying to all regions in one week.',
        }

        const workflow = Workflow.fromFile('./examples/workflows/schema_types.ai', {
            inputs: inputPayload,
            secrets: providerSecrets,
        })

        const response = await engine.run<StructuredRiskResponse>(workflow)

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
