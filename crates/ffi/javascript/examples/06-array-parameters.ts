import { ExecutionEngine } from '../index.js'
import { createTool } from '../tool-helper.js'
import { z } from 'zod'

async function main(): Promise<void> {
    console.log('=== Example 6: Array Parameters ===')

    const batchTool = createTool(
        'batch_process',
        'Process multiple items at once',
        z.object({
            items: z.array(z.string()).min(1).describe('List of items to process'),
            operation: z.enum([ 'uppercase', 'lowercase', 'reverse' ]).describe('Operation to apply'),
        }),
        (params) => {
            const processed = params.items.map(item => {
                switch (params.operation) {
                    case 'uppercase':
                        return item.toUpperCase()
                    case 'lowercase':
                        return item.toLowerCase()
                    case 'reverse':
                        return item.split('').reverse().join('')
                    default:
                        return item
                }
            })
            return { processed }
        },
    )

    const engine = ExecutionEngine.withTools([ batchTool ])

    const workflow = `
        provider ollama {
            driver <- "ollama"
            models <- ["qwen3:8b"]
            config <- { endpoint <- "http://100.76.5.36:11434" }
        }
        
        agent batch_agent {
            model <- "ollama/qwen3:8b"
            tools <- [tool.batch_process]
            output <- string
            prompt <- "Convert the words 'hello', 'world', 'engine' to uppercase using the batch_process tool"
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
