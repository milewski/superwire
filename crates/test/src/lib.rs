mod macros;

pub use macros::{current_test_name, get};

use engine_ai_core::ast::Workflow;
use engine_ai_core::execution::engine::ExecutionEngine;
use engine_ai_core::parser::AstBuilder;
use engine_ai_core::providers::{CachedProvider, ProviderFactory, ProviderRegistry};
use serde_json::Value;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use serde::Deserialize;

pub async fn execute_cached_workflow_from_content(
    test_name: &str,
    workflow_path: &str,
    workflow_content: &str,
    inputs: HashMap<String, Value>,
) -> Result<Value, engine_ai_core::execution::error::ExecutionError> {
    let workflow_hash = hash_workflow_content(workflow_content);

    let builder = AstBuilder::new(workflow_path.to_string());
    let workflow = builder.parse(workflow_content).map_err(|error| {
        engine_ai_core::execution::error::ExecutionError::RuntimeError {
            agent: "workflow".to_string(),
            message: format!("Failed to parse workflow: {error}"),
            suggestion: Some("Check workflow syntax".to_string()),
        }
    })?;

    let engine = ExecutionEngine::new();
    execute_with_cached_providers(&engine, &workflow, inputs, test_name, &workflow_hash).await
}

fn hash_workflow_content(workflow_content: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    workflow_content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

async fn execute_with_cached_providers(
    engine: &ExecutionEngine,
    workflow: &Workflow,
    inputs: HashMap<String, Value>,
    test_name: &str,
    workflow_hash: &str,
) -> Result<Value, engine_ai_core::execution::error::ExecutionError> {
    let mut provider_registry = ProviderRegistry::new();

    for provider in &workflow.providers {
        let inner_provider = ProviderFactory::create_provider(provider).map_err(|error| {
            engine_ai_core::execution::error::ExecutionError::ProviderError {
                agent: "workflow".to_string(),
                message: format!("Failed to create provider '{}': {}", provider.name, error),
                suggestion: Some("Check provider configuration".to_string()),
            }
        })?;

        let cached_provider = CachedProvider::new(test_name.to_string(), workflow_hash.to_string(), inner_provider);

        provider_registry.register(provider.name.clone(), Arc::new(cached_provider));
    }

    engine
        .execute_parsed_workflow_with_inputs_and_registry(workflow, inputs, provider_registry)
        .await
}

// Integration tests for Engine AI workflows
// Tests are marked with #[ignore] by default to avoid requiring a running Ollama instance
// Run with: cargo test --package engine-ai-test -- --ignored
// Or run specific test: cargo test --package engine-ai-test test_basic_workflow -- --ignored
//
// CACHING: LLM responses are automatically cached per test to .test_cache/<test_name>.json
// The cache is organized by agent, so each agent's conversation history is tracked separately.
// This works correctly even when multiple agents use different models.
// The cache also stores a hash of the workflow file contents.
// If the .ai file changes, the hash mismatch invalidates the cache and regenerates it.
// On first run, the LLM is called and all messages are saved.
// On subsequent runs, the cached messages are replayed instead of calling the LLM.
// The workflow execution logic still runs, so code changes are tested.
// Delete the cache file to force fresh LLM calls for that specific test.
//
// Cache structure:
// {
//   "workflow_hash": "a1b2c3d4e5f67890",
//   "agents": {
//     "joke": {
//       "model": "ollama/qwen3:8b",
//       "messages": [
//         { "type": "user", "content": "Tell me a short programming joke." },
//         { "type": "assistant", "content": "Why do programmers..." }
//       ]
//     },
//     "fact": {
//       "model": "ollama/qwen3:8b",
//       "messages": [
//         { "type": "user", "content": "Tell me an interesting fact..." },
//         { "type": "assistant", "content": "The first computer bug..." }
//       ]
//     },
//     "quote": {
//       "model": "openai/gpt-4",
//       "messages": [...]
//     }
//   }
// }
//
// Usage examples:
//   let result = try_workflow!("../workflows/basic.ai").await;
//   let output = workflow!("../workflows/basic.ai").await;
//
//   // With typed output
//   #[derive(Deserialize)]
//   struct Output { greeting: String }
//   let output: Output = workflow!("../workflows/basic.ai" => Output).await;
//
//   let inputs = input!(
//       topic: "Rust",
//       audience: "developers"
//   );
//   let result = try_workflow!(inputs => "../workflows/input_output.ai").await;
//   let output = workflow!(inputs => "../workflows/input_output.ai").await;
//   let typed_output: Output = workflow!(inputs => "../workflows/input_output.ai" => Output).await;
//
//   // Optional explicit test name override
//   let output = workflow!("custom_cache_name", "../workflows/basic.ai").await;
//
//   // Simple assertions
//   assert!(get(&output, "/greeting").is_string());
//   assert_eq!(get(&output, "/topic"), "Rust");

#[tokio::test]
async fn test_basic_workflow() {
    #[derive(Deserialize)]
    struct Output {
        greeting: String,
    }

    let output = workflow!("../workflows/basic.ai" => Output).await;

    assert!(output.greeting.contains("AI assistant"));
}

#[tokio::test]
async fn test_input_output_workflow() {
    let inputs = input!(topic: "Rust", audience: "developers");

    #[derive(Deserialize)]
    struct Output {
        topic: String,
        audience: String,
        summary: String,
    };

    let output = workflow!(inputs => "../workflows/input_output.ai" => Output).await;

    assert_eq!(output.topic, "Rust");
    assert_eq!(output.audience, "developers");
    assert!(output.summary.contains("Rust"));
}

#[tokio::test]
#[ignore]
async fn test_with_try_workflow() {
    let inputs = input!(topic: "WebAssembly", audience: "systems programmers");

    let result = try_workflow!(inputs => "../workflows/input_output.ai").await;

    assert!(result.is_ok(), "Workflow execution failed: {:?}", result.err());

    let output = result.unwrap();
    assert_eq!(get(&output, "/topic"), "WebAssembly");
}
