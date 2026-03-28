import { Engine, schema, Tool, type ToolArguments, Workflow } from '../src'

type WeatherInput = {
    country?: string;
}

type WeatherBoundedInput = {
    key: string;
}

type WeatherOutput = {
    prediction: string;
}

type WeatherArguments = ToolArguments<WeatherInput, WeatherBoundedInput>

class Weather extends Tool<WeatherInput, WeatherOutput, WeatherBoundedInput> {
    readonly description = 'Get weather prediction for a country'

    readonly inputSchema = schema.object({
        country: schema.string(),
    })

    constructor() {
        super('weather')
    }

    execute(toolArguments: WeatherArguments): WeatherOutput {
        const country = toolArguments.input.country
        const apiKey = toolArguments.bounded.key

        console.log('Key', apiKey)

        return {
            prediction: `It is very sunny in ${ country }`,
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
        
        secrets {
            key: string
        }

        input {
            country: string
        }

        agent assistant {
            model: openai_local("qwen3.5-9b")
            tools: [tool.weather(key: secrets.key)]
            prompt: "Call the weather tool first, then summarize the weather for {{ input.country }}."
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

    const response = await engine.run<Response>(
        workflow, { country: 'Japan' }, { key: 'secret-key' },
    )

    if (response.isError()) {
        console.error(response.error)
        engine.close()
        return
    }

    console.log(response.success)

    engine.close()
}

runSimpleExample()
