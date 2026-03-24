mod graph;
mod schema;
mod template;
mod types;
mod validator;

use crate::ast::{Expression, InferenceProperty, ModelSelector, ObjectField, PromptValue, ToolUsage, TypeExpression, TypeField};
use schemars::Schema;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub use graph::DependencyGraph;
pub use schema::{build_object_schema, build_type_schema};
pub use template::TemplateDocument;
pub use validator::compile_workflow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderDriver {
    Ollama,
    OpenAi,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledProvider {
    pub api_key_secret_name: Option<String>,
    pub driver: ProviderDriver,
    pub endpoint: Option<String>,
    pub models: Vec<String>,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompiledAgent {
    pub context: Option<Expression>,
    pub dependencies: BTreeSet<String>,
    pub for_loop: Option<crate::ast::ForLoopBinding>,
    pub inference: Vec<InferenceProperty>,
    pub model: ModelSelector,
    pub name: String,
    pub output_type: TypeExpression,
    pub prompt: PromptValue,
    pub tools: Vec<ToolUsage>,
}

#[derive(Debug, Clone)]
pub struct CompiledWorkflow {
    pub agents: Vec<CompiledAgent>,
    pub base_path: PathBuf,
    pub dependency_graph: DependencyGraph,
    pub input_fields: Vec<TypeField>,
    pub input_schema: Option<Schema>,
    pub output_fields: Vec<ObjectField>,
    pub providers: BTreeMap<String, CompiledProvider>,
    pub schemas: BTreeMap<String, TypeExpression>,
    pub secret_fields: Vec<TypeField>,
    pub secret_schema: Option<Schema>,
}
