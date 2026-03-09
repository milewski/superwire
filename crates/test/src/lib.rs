mod macros;

pub use macros::current_test_name;

use engine_ai_core::ast::Workflow;
use engine_ai_core::execution::engine::ExecutionEngine;
use engine_ai_core::parser::AstBuilder;
use engine_ai_core::providers::{CachedProvider, ProviderFactory, ProviderRegistry};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

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
    }

    let output = workflow!(inputs => "../workflows/input_output.ai" => Output).await;

    assert_eq!(output.topic, "Rust");
    assert_eq!(output.audience, "developers");
    assert!(output.summary.contains("Rust"));
}

#[tokio::test]
async fn test_schema_workflow() {
    #[derive(Deserialize)]
    struct Output {
        person: Person,
    }

    #[derive(Deserialize)]
    struct Person {
        name: String,
        age: u32,
        hobbies: Vec<String>,
    }

    let output = workflow!("../workflows/schema.ai" => Output).await;

    assert!(!output.person.name.is_empty());
    assert!(output.person.age > 0);
    assert!(!output.person.hobbies.is_empty());
}

#[tokio::test]
async fn test_inline_schema_workflow() {
    #[derive(Deserialize)]
    struct Output {
        person: Person,
    }

    #[derive(Deserialize)]
    struct Person {
        name: String,
        age: u32,
        city: String,
    }

    let output = workflow!("../workflows/inline_schema.ai" => Output).await;

    assert!(!output.person.name.is_empty());
    assert!(output.person.age > 0);
    assert!(!output.person.city.is_empty());
}

#[tokio::test]
async fn test_parallel_execution_workflow() {
    #[derive(Deserialize)]
    struct Output {
        joke: String,
        fact: String,
        quote: String,
    }

    let output = workflow!("../workflows/parallel_execution.ai" => Output).await;

    assert!(!output.joke.is_empty());
    assert!(!output.fact.is_empty());
    assert!(!output.quote.is_empty());
}

#[tokio::test]
async fn test_enum_schema_workflow() {
    #[derive(Deserialize)]
    struct Output {
        weather: Weather,
    }

    #[derive(Deserialize)]
    struct Weather {
        condition: String,
        temperature: f64,
    }

    let output = workflow!("../workflows/enum_schema.ai" => Output).await;

    assert!(["sunny", "rainy", "cloudy", "snowy"].contains(&output.weather.condition.as_str()));
    assert!(output.weather.temperature >= -50.0 && output.weather.temperature <= 50.0);
}

#[tokio::test]
async fn test_dependencies_workflow() {
    #[derive(Deserialize)]
    struct Output {
        article: String,
    }

    let output = workflow!("../workflows/dependencies.ai" => Output).await;

    assert!(!output.article.is_empty());
}

#[tokio::test]
async fn test_context_sharing_workflow() {
    #[derive(Deserialize)]
    struct Output {
        conversation_continue: String,
    }

    let output = workflow!("../workflows/context_sharing.ai" => Output).await;

    assert!(!output.conversation_continue.is_empty());
}

#[tokio::test]
async fn test_for_each_workflow() {
    #[derive(Deserialize)]
    struct Output {
        doubled: Vec<f64>,
        numbers: Vec<f64>,
    }

    let output = workflow!("../workflows/for_each.ai" => Output).await;

    assert_eq!(output.doubled.len(), 3);
    assert_eq!(output.numbers.len(), 3);

    for (index, value) in output.doubled.iter().enumerate() {
        let original = output.numbers[index];
        assert_eq!(*value, original * 2.0);
    }
}

#[tokio::test]
async fn test_string_interpolation_workflow() {
    #[derive(Deserialize)]
    struct Output {
        story: String,
    }

    let output = workflow!("../workflows/string_interpolation.ai" => Output).await;

    assert!(!output.story.is_empty());
}

#[tokio::test]
async fn test_schema_descriptions_workflow() {
    #[derive(Deserialize)]
    struct Output {
        user: User,
    }

    #[derive(Deserialize)]
    struct User {
        username: String,
        email: String,
        age: u32,
    }

    let output = workflow!("../workflows/schema_descriptions.ai" => Output).await;

    assert!(output.user.username.len() >= 3 && output.user.username.len() <= 20);
    assert!(output.user.email.contains('@'));
    assert!(output.user.age >= 13 && output.user.age <= 120);
}

#[tokio::test]
async fn test_multiline_prompt_workflow() {
    #[derive(Deserialize)]
    struct Output {
        story: String,
    }

    let output = workflow!("../workflows/multiline_prompt.ai" => Output).await;

    assert!(!output.story.is_empty());
    assert!(output.story.len() < 1000);
}

#[tokio::test]
async fn test_nullable_schema_workflow() {
    #[derive(Deserialize)]
    struct Output {
        person: Person,
    }

    #[derive(Deserialize)]
    struct Person {
        name: String,
        age: u32,
        #[allow(dead_code)]
        nickname: Option<String>,
        #[allow(dead_code)]
        email: Option<String>,
    }

    let output = workflow!("../workflows/nullable_schema.ai" => Output).await;

    assert!(!output.person.name.is_empty());
    assert!(output.person.age > 0);
}

#[tokio::test]
async fn test_compact_syntax_workflow() {
    #[derive(Deserialize)]
    struct Output {
        single_context_summary: Vec<Value>,
        multi_context_summary: Vec<Value>,
    }

    let output = workflow!("../workflows/compact_syntax_test.ai" => Output).await;

    assert!(!output.single_context_summary.is_empty());
    assert!(!output.multi_context_summary.is_empty());
}

#[tokio::test]
async fn test_auto_unwrap_workflow() {
    #[derive(Deserialize)]
    struct Output {
        single_unwrapped: String,
        single_explicit: String,
        multi_full: MultiField,
        multi_name: String,
        multi_age: u32,
    }

    #[derive(Deserialize)]
    struct MultiField {
        name: String,
        age: u32,
    }

    let output = workflow!("../workflows/auto_unwrap_test.ai" => Output).await;

    assert!(!output.single_unwrapped.is_empty());
    assert!(!output.single_explicit.is_empty());
    assert_eq!(output.single_unwrapped, output.single_explicit);
    assert!(!output.multi_full.name.is_empty());
    assert_eq!(output.multi_full.name, output.multi_name);
    assert_eq!(output.multi_full.age, output.multi_age);
}

#[tokio::test]
async fn test_agent_loop_workflow() {
    #[derive(Deserialize)]
    struct Output {
        result: String,
    }

    let output = workflow!("../workflows/agent_loop_test.ai" => Output).await;

    assert!(!output.result.is_empty());
}

#[tokio::test]
async fn test_no_schema_done_workflow() {
    #[derive(Deserialize)]
    struct Output {
        simple: String,
    }

    let output = workflow!("../workflows/no_schema_done.ai" => Output).await;

    assert!(!output.simple.is_empty());
}

#[tokio::test]
async fn test_terminal_with_output_workflow() {
    let inputs = input!(user_name: "Alice");

    #[derive(Deserialize)]
    struct Output {
        user: String,
        timestamp: String,
        greeting: String,
    }

    let output = workflow!(inputs => "../workflows/terminal_with_output.ai" => Output).await;

    assert_eq!(output.user, "Alice");
    assert!(!output.timestamp.is_empty());
    assert!(!output.greeting.is_empty());
}

#[tokio::test]
async fn test_multiple_terminal_workflow() {
    #[derive(Deserialize)]
    struct Output {
        joke: String,
        fact: String,
    }

    let output = workflow!("../workflows/multiple_terminal.ai" => Output).await;

    assert!(!output.joke.is_empty());
    assert!(!output.fact.is_empty());
}

#[tokio::test]
async fn test_compact_context_workflow() {
    let inputs = input!(topic: "artificial intelligence");

    #[derive(Deserialize)]
    struct Output {
        topic: String,
        summary: Vec<Value>,
        research_context: Vec<Value>,
    }

    let output = workflow!(inputs => "../workflows/compact_context.ai" => Output).await;

    assert_eq!(output.topic, "artificial intelligence");
    assert!(!output.summary.is_empty());
    assert!(!output.research_context.is_empty());
}

#[tokio::test]
async fn test_simple_inline_type_workflow() {
    #[derive(Deserialize)]
    struct Output {
        sum: u32,
        greeting: String,
        is_hundred: bool,
    }

    let output = workflow!("../workflows/simple_inline_type.ai" => Output).await;

    assert_eq!(output.sum, 100);
    assert!(!output.greeting.is_empty());
    assert!(output.is_hundred);
}

#[tokio::test]
async fn test_inline_type_demo_workflow() {
    #[derive(Deserialize)]
    struct Output {
        calculation: u32,
        greeting: String,
        is_large: bool,
        languages: Vec<String>,
        summary: String,
    }

    let output = workflow!("../workflows/inline_type_demo.ai" => Output).await;

    assert_eq!(output.calculation, 105);
    assert!(!output.greeting.is_empty());
    assert!(output.is_large);
    assert_eq!(output.languages.len(), 5);
    assert!(!output.summary.is_empty());
}

#[tokio::test]
async fn test_for_each_context_workflow() {
    #[derive(Deserialize)]
    struct Output {
        items: Vec<String>,
        descriptions: Vec<Description>,
        descriptions_context: Vec<Value>,
    }

    #[derive(Deserialize)]
    struct Description {
        description: String,
    }

    let output = workflow!("../workflows/for_each_context_test.ai" => Output).await;

    assert_eq!(output.items.len(), 2);
    assert_eq!(output.descriptions.len(), 2);
    assert!(!output.descriptions_context.is_empty());
}
