import { ExecutionEngine } from '../index.js'
import { createTool } from '../tool-helper.js'
import { z } from 'zod'

async function main(): Promise<void> {
    console.log('=== Example 2: Calculator Tool with Zod ===\n')

    const calculatorTool = createTool(
        'calculator',
        'Perform basic arithmetic operations',
        z.object({
            operation: z.enum([ 'add', 'subtract', 'multiply', 'divide' ]).describe('The operation to perform'),
            a: z.number().describe('First number'),
            b: z.number().describe('Second number'),
        }),
        (params) => {
            let result: number | string
            switch (params.operation) {
                case 'add':
                    result = params.a + params.b
                    break
                case 'subtract':
                    result = params.a - params.b
                    break
                case 'multiply':
                    result = params.a * params.b
                    break
                case 'divide':
                    result = params.b !== 0 ? params.a / params.b : 'Error: Division by zero'
                    break
            }
            return { result }
        },
    )

    const engine = ExecutionEngine.withTools([ calculatorTool ])

    const workflow = `
        provider ollama {
            driver <- "ollama"
            models <- ["qwen3:8b"]
            config <- { endpoint <- "http://100.76.5.36:11434" }
        }
        
        agent math_helper {
            model <- "ollama/qwen3:8b"
            tools <- [tool.calculator]
            output <- string
            prompt <- "Calculate 15 multiplied by 7, then add 10 to the result"
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
