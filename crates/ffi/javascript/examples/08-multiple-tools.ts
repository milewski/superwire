import { ExecutionEngine } from '../index.js'
import { ZodTool } from '../tool-helper.js'
import { z } from 'zod'

class WeatherTool extends ZodTool {
    constructor() {
        super(
            'get_weather',
            'Get current weather for a location',
            z.object({
                location: z.string().describe('City name'),
            }),
            (params) => {
                const weather: Record<string, { temp: number; condition: string }> = {
                    'San Francisco': { temp: 65, condition: 'foggy' },
                    'New York': { temp: 72, condition: 'sunny' },
                    'London': { temp: 55, condition: 'rainy' },
                }

                const data = weather[ params.location ] || { temp: 70, condition: 'unknown' }
                return {
                    location: params.location,
                    temperature: data.temp,
                    condition: data.condition,
                }
            },
        )
    }
}

class TimeTool extends ZodTool {
    constructor() {
        super(
            'get_time',
            'Get current time',
            z.object({}),
            () => {
                return {
                    time: new Date().toISOString(),
                }
            },
        )
    }
}

async function main(): Promise<void> {
    console.log('=== Example 8: Multiple Custom Tools ===\n')

    const weatherTool = new WeatherTool()
    const timeTool = new TimeTool()
    const engine = ExecutionEngine.withTools([ weatherTool, timeTool ])

    const workflow = `
        provider ollama {
            driver <- "ollama"
            models <- ["qwen3:8b"]
            config <- { endpoint <- "http://100.76.5.36:11434" }
        }
        
        agent assistant {
            model <- "ollama/qwen3:8b"
            tools <- [tool.get_weather, tool.get_time]
            output <- string
            prompt <- """
                What's the weather like in San Francisco right now?
                Also tell me the current time.
            """
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
