/// Example showing how to actually execute agents with the execution engine
///
/// This connects to a real Ollama instance and executes the workflow.

use engine_ai_core::*;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logger
    colog::init();

    println!("AI Engine - Full Execution Example\n");

    // Define the DSL
    let dsl = r#"
provider ollama1 {
    driver <- "ollama"
    api_endpoint <- "http://100.76.5.36:11434"
    models <- ["qwen3.5:27b"]
}

schema person {
    name: string
    age: number
}

agent extract {
    model <- "ollama1/qwen3.5:27b"
    output <- schema.person
    prompt <- "Extract: John is 30 years old. Return JSON with fields 'name' and 'age'."
}

<- agent summary {
    model <- "ollama1/qwen3.5:27b"
    prompt <- "Create a one sentence summary about {{ extract.name }} who is {{ extract.age }} years old. remember to call the done tool once you are done"
}
    "#;

    // Parse the document
    println!("Parsing DSL...");
    let document = parse_document(dsl)?;
    println!("✓ Parsed successfully");
    println!("  - Agents: {}", document.agents.len());
    println!("  - Schemas: {}", document.schemas.len());
    println!("  - Providers: {}\n", document.providers.len());

    // Validate
    println!("Validating...");
    validate_document(&document)?;
    println!("✓ Validation passed\n");

    // Create execution engine
    println!("Setting up execution engine...");
    let mut engine = ExecutionEngine::new();

    // Add Ollama provider
    let ollama_provider = OllamaProvider::new(
        "ollama1".to_string(),
        "http://100.76.5.36:11434".to_string(),
        vec!["qwen3.5:27b".to_string()],
    );
    engine.add_provider(Arc::new(ollama_provider));
    println!("✓ Provider configured\n");

    // Create orchestrator
    let orchestrator = Orchestrator::new(engine);

    // Execute the workflow
    println!("Executing workflow...");
    println!("Connecting to Ollama at http://100.76.5.36:11434\n");

    match orchestrator.execute_document(&document).await {
        Ok(result) => {
            println!("✓ Execution completed successfully!\n");
            println!("=== Final Result ===");
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Err(e) => {
            println!("✗ Execution failed: {}\n", e);
            println!("Possible reasons:");
            println!("  - Ollama is not running at http://100.76.5.36:11434");
            println!("  - Model 'qwen2.5:3b' is not available");
            println!("  - Network connectivity issues");
            println!("  - Tool calling not supported (agents may timeout)");
            return Err(e);
        }
    }

    Ok(())
}
