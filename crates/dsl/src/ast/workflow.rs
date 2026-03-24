use crate::ast::expression::{Expression, ModelSelector, PromptValue, ToolUsage};
use crate::ast::types::TypeExpression;

#[derive(Debug, Clone, PartialEq)]
pub struct Workflow {
    pub agents: Vec<AgentDeclaration>,
    pub input_fields: Vec<crate::ast::types::TypeField>,
    pub output_fields: Vec<crate::ast::expression::ObjectField>,
    pub providers: Vec<ProviderDeclaration>,
    pub schemas: Vec<(String, Vec<crate::ast::types::TypeField>)>,
    pub secret_fields: Vec<crate::ast::types::TypeField>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderDeclaration {
    pub name: String,
    pub properties: Vec<ProviderProperty>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProviderProperty {
    ApiKey(Expression),
    Driver(String),
    Endpoint(String),
    Models(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentDeclaration {
    pub name: String,
    pub for_loop: Option<ForLoopBinding>,
    pub properties: Vec<AgentProperty>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ForLoopBinding {
    pub item_name: String,
    pub source: Expression,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentProperty {
    Context(Expression),
    Inference(Vec<InferenceProperty>),
    Model(ModelSelector),
    Output(TypeExpression),
    Prompt(PromptValue),
    Tools(Vec<ToolUsage>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum InferenceProperty {
    FrequencyPenalty(String),
    MaxTokens(usize),
    PresencePenalty(String),
    RepeatPenalty(String),
    Seed(i32),
    StopSequences(Vec<String>),
    Temperature(String),
    TopK(u32),
    TopP(String),
}
