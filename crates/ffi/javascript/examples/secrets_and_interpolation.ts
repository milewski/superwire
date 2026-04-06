/**
 * Secrets and interpolation example.
 *
 * Shows how workflow secrets are injected at runtime and interpolated in prompts,
 * so policy text can be controlled by the caller instead of hardcoded.
 */
import { Engine, Workflow } from '../src'
import { loadOpenAIProviderSecrets } from './env'

type SupportReplyInput = {
    customer_question: string;
}

type SupportReplySecrets = {
    response_policy: string;
}

type SupportReplyResponse = {
    answer: string;
    escalation_needed: boolean;
    escalation_reason: string | null;
}

async function runSecretsAndInterpolationExample(): Promise<void> {
    const providerSecrets = loadOpenAIProviderSecrets()

    const engine = new Engine()

    try {
        const inputPayload: SupportReplyInput = {
            customer_question: 'I was charged twice after upgrading. Can you fix this and tell me what happened?',
        }

        const secretsPayload: SupportReplySecrets & typeof providerSecrets = {
            ...providerSecrets,
            response_policy: 'Be concise, acknowledge the issue, never promise refunds instantly, and escalate billing disputes that involve duplicate charges.',
        }

        const workflow = Workflow.fromFile('./examples/workflows/secrets_and_interpolation.wire', {
            inputs: inputPayload,
            secrets: secretsPayload,
        })

        const response = await engine.run<SupportReplyResponse>(workflow)

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

runSecretsAndInterpolationExample()
