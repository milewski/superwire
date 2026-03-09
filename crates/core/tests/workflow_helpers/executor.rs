use engine_ai_core::ast::Workflow;
use engine_ai_core::execution::engine::ExecutionEngine;
use engine_ai_core::parser::AstBuilder;
use engine_ai_core::providers::{CachedProvider, ProviderFactory, ProviderRegistry};
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
