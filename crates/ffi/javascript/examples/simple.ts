import { Engine, schema, Tool, Workflow } from '../src'

type WeatherInput = {
    country: string;
}

type WeatherOutput = {
    prediction: string;
}

class Weather extends Tool<WeatherInput, WeatherOutput> {
    readonly description = 'Get weather prediction for a country'

    readonly inputSchema = schema.object({
        country: schema.string(),
    })

    constructor() {
        super('weather')
    }

    execute(input: WeatherInput): WeatherOutput {
        return {
            prediction: `It is very sunny in ${ input.country }`,
        }
    }
}

async function runSimpleExample(): Promise<void> {
    const workflow = new Workflow(`
        provider openai_local {
            driver: "openai"
            endpoint: "http://169.254.83.107:1234/v1"
            api_key: "local-api-key"
            models: ["qwen3.5-9b"]
        }

        input {
            country: string
        }

        agent assistant {
            model: openai_local("qwen3.5-9b")
            tools: [tool.weather(country: input.country)]
            prompt: "Call the weather tool first, then summarize the weather for {{ input.country }} in one sentence."
            output: string
        }

        output {
            weather: agent.assistant
        }
    `)

    type Response = {
        weather: string;
    }

    const engine = new Engine()
    engine.registerTool(new Weather())

    const response = await engine.run<Response>(workflow, {
        country: 'China',
    })

    if (response.isError()) {
        console.error(response.error)
        engine.close()
        return
    }

    console.log(response.success)

    engine.close()
}

runSimpleExample()
