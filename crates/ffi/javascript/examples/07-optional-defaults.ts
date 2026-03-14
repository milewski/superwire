import { ExecutionEngine } from '../index.js'
import { createTool } from '../tool-helper.js'
import { z } from 'zod'

async function main(): Promise<void> {
    console.log('=== Example 7: Optional and Default Values ===\n')

    const greetingTool = createTool(
        'greeter',
        'Generate personalized greetings',
        z.object({
            name: z.string().describe('Person to greet'),
            style: z.enum([ 'formal', 'casual', 'funny' ]).default('casual').describe('Greeting style'),
            includeTime: z.boolean().optional().describe('Include time of day in greeting'),
        }),
        (params) => {
            const timeOfDay = params.includeTime ? new Date().getHours() < 12 ? 'morning' : 'evening' : ''
            const greetings: Record<string, string> = {
                formal: `Good ${ timeOfDay }, ${ params.name }. It is a pleasure to make your acquaintance.`,
                casual: `Hey ${ params.name }! ${ timeOfDay ? `Good ${ timeOfDay }!` : 'What\'s up?' }`,
                funny: `Yo ${ params.name }! ${ timeOfDay ? `Rise and shine!` : '*high five* 🙌' }`,
            }
            return { greeting: greetings[ params.style ] }
        },
    )

    const engine = ExecutionEngine.withTools([ greetingTool ])

    const workflow = `
        provider ollama {
            driver <- "ollama"
            models <- ["qwen3:8b"]
            config <- { endpoint <- "http://100.76.5.36:11434" }
        }
        
        agent greeter_agent {
            model <- "ollama/qwen3:8b"
            tools <- [tool.greeter]
            output <- string
            prompt <- "Greet Alice in a funny style and include the time of day"
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
