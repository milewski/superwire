import { Engine, Workflow } from '../src'

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
    const workflow = new Workflow(`
        provider openai_local {
            driver: "openai"
            endpoint: "http://169.254.83.107:1234/v1"
            api_key: "local-api-key"
            models: ["qwen/qwen3.5-35b-a3b"]
        }

        secrets {
            response_policy: string
        }

        input {
            customer_question: string
        }

        agent support_reply {
            model: openai_local("qwen/qwen3.5-35b-a3b")
            prompt: "Answer this customer question: {{ input.customer_question }}. Follow this response policy exactly: {{ secrets.response_policy }}"
            output: {
                answer: string
                escalation_needed: boolean
                escalation_reason: string | null
            }
        }

        output {
            answer: agent.support_reply.answer
            escalation_needed: agent.support_reply.escalation_needed
            escalation_reason: agent.support_reply.escalation_reason
        }
    `)

    const engine = new Engine()

    try {
        const inputPayload: SupportReplyInput = {
            customer_question: 'I was charged twice after upgrading. Can you fix this and tell me what happened?',
        }

        const secretsPayload: SupportReplySecrets = {
            response_policy: 'Be concise, acknowledge the issue, never promise refunds instantly, and escalate billing disputes that involve duplicate charges.',
        }

        const response = await engine.run<SupportReplyResponse, SupportReplyInput, SupportReplySecrets>(
            workflow,
            inputPayload,
            secretsPayload,
        )

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
