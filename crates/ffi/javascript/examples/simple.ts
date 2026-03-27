import { Engine, Workflow } from '../src'

async function runSimpleExample(): Promise<void> {
    const workflow = new Workflow(`
        provider openai_local {
            driver: "openai"
            endpoint: "http://169.254.83.107:1234/v1"
            api_key: "local-api-key"
            models: ["qwen3.5-9b"]
        }
        
        input {
            topic: string
        }
        
        agent joker {
            model: openai_local("qwen3.5-9b")
            prompt: "Tell me a joke about {{ input.topic }}"
            output: string
        }
        
        output {
            joke: agent.joker
        }
    `)

    type Response = {
        joke: string;
    };

    const engine = new Engine()
    const response = await engine.run<Response>(workflow, { topic: 'Animals' })

    if (response.isError()) {
        console.error(response.failure)
    } else {
        console.log(response.success)
    }

    engine.close()
}

runSimpleExample()
