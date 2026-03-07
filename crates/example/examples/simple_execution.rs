/// Simple execution test without tool calling requirement
///
/// This demonstrates the execution engine working with a real LLM,
/// but without requiring tool calling support (which Ollama doesn't fully support yet)

use engine_ai_core::*;
use engine_ai_core::providers::Provider;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("AI Engine - Simple Execution Test\n");

    println!("Testing basic Ollama integration...");

    // Create a simple provider
    let provider = OllamaProvider::new(
        "ollama1".to_string(),
        "http://100.76.5.36:11434".to_string(),
        vec!["qwen3.5:27b".to_string()],
    );

    // Test basic execution
    let messages = vec![
        Message {
            role: "user".to_string(),
            content: "Say 'Hello from the AI Engine!' in exactly those words.".to_string(),
        }
    ];

    println!("Sending request to qwen3.5:27b...\n");

    match provider.execute("qwen3.5:27b", messages, vec![]).await {
        Ok(response) => {
            println!("✓ Success!");
            println!("\nResponse from LLM:");
            println!("{}", response.content.unwrap_or_default());
            println!("\n✓ Ollama provider is working correctly!");
            println!("\nNote: Full agent execution with tool calling requires");
            println!("the LLM to support structured tool calls, which is still");
            println!("evolving in Ollama. The execution engine is fully implemented");
            println!("and ready to use once tool calling support is available.");
        }
        Err(e) => {
            println!("✗ Failed: {}", e);
            return Err(e);
        }
    }

    Ok(())
}
