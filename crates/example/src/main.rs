mod calculator_tool;

use engine_ai_core::*;
use std::fs;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Parse the document
    println!("Parsing DSL...");
    let document = parse_document(include_str!("../example_context.dsl"))?;
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
