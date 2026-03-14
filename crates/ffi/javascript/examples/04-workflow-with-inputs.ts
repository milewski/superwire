import { ExecutionEngine } from '../index.js'

async function main(): Promise<void> {
    console.log('=== Example 4: Workflow with Inputs ===')

    const engine = new ExecutionEngine()

    const workflow = `
        provider ollama {
            driver <- "ollama"
            models <- ["qwen3:8b"]
            config <- { endpoint <- "http://100.76.5.36:11434" }
        }
        
        input {
            user_name <- string
            favorite_color <- string
        }
        
        agent personalizer {
            model <- "ollama/qwen3:8b"
            output <- string
            prompt <- """
                Create a personalized greeting for {input.user_name}.
                Mention that their favorite color is {input.favorite_color}.
                Make it warm and friendly.
            """
        }
  `

    const inputs = JSON.stringify({
        user_name: 'Alice',
        favorite_color: 'blue',
    })

    try {
        const result = await engine.executeWorkflowContentWithInputs(workflow, inputs)
        console.log(result)
    } catch (error) {
        console.error('Error:', (error as Error).message)
    }
}

main().catch(console.error)
