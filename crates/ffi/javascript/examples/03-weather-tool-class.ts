import { ExecutionEngine } from '../index.js'
import { ZodTool } from '../tool-helper.js'
import { z } from 'zod'

class WeatherTool extends ZodTool {
    constructor() {
        super(
            'get_weather',
            'Get weather information for a location',
            z.object({
                location: z.string().describe('The location to get weather for'),
            }),
            (params) => {
                return {
                    location: params.location,
                    temperature: 72,
                    condition: 'sunny',
                }
            },
        )
    }
}

async function main(): Promise<void> {
    console.log('=== Example 3: Weather Tool using ZodTool Class ===')

    const weatherTool = new WeatherTool()
    const engine = ExecutionEngine.withTools([ weatherTool ])

    const workflow = `
        provider ollama {
            driver <- "ollama"
            models <- ["qwen3:8b"]
            config <- { endpoint <- "http://100.76.5.36:11434" }
        }
        
        agent weather_agent {
            model <- "ollama/qwen3:8b"
            tools <- [tool.get_weather]
            output <- string
            prompt <- "What's the weather in San Francisco?"
        }
  `

    try {
        const result = await engine.executeWorkflowContent(workflow)
        console.log(result)
    } catch (error) {
        console.error('Error:', (error as Error).message)
    }
}

main().catch(console.error)
