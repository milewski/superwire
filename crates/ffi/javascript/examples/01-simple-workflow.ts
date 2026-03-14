import { ExecutionEngine } from '../index.js'

async function main(): Promise<void> {
    console.log('=== Example 1: Simple Workflow ===')

    const engine = new ExecutionEngine()

    const workflow = `
        provider ollama {
            driver <- "ollama"
            models <- ["qwen3:8b"]
            config <- { endpoint <- "http://100.76.5.36:11434" }
        }
        
        <- agent poet {
            model <- "ollama/qwen3:8b"
            output <- string
            prompt <- "Write a short haiku about programming"
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
