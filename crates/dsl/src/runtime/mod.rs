mod dynamic_executor;
mod interpolation;
mod provider_factory;
mod runner;
mod tool_binding;
mod value;

use crate::ast::ModelSelector;
use engine_ai_agent::Context;
use serde_json::Value;
use std::collections::BTreeMap;

pub use provider_factory::{DefaultProviderFactory, ProviderFactory};
pub use runner::WorkflowRunner;

#[derive(Debug, Clone)]
pub(crate) enum StoredContext {
    Many(Vec<Context>),
    Single(Context),
}

#[derive(Debug, Clone)]
pub struct WorkflowAgentResult {
    pub context: StoredContext,
    pub model: ModelSelector,
    pub output: Value,
}

#[derive(Debug, Clone)]
pub(crate) struct WorkflowState {
    pub agent_results: BTreeMap<String, WorkflowAgentResult>,
    pub inputs: Value,
    pub secrets: BTreeMap<String, Value>,
}
