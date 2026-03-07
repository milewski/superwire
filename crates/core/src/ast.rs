use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub agents: HashMap<String, Agent>,
    pub schemas: HashMap<String, Schema>,
    pub providers: HashMap<String, Provider>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub name: String,
    pub model: Option<String>,
    pub tools: Vec<String>,
    pub context: Option<ContextRef>,
    pub output: Option<SchemaRef>,
    pub prompt: PromptValue,
    pub for_each: Option<ForEach>,
    pub is_terminal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContextRef {
    Full(String),    // agent.name.context
    Summary(String), // agent.name.context.summary
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SchemaRef {
    Named(String),
    Inline(Schema),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schema {
    pub name: Option<String>,
    pub fields: HashMap<String, SchemaType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SchemaType {
    String,
    Number,
    Boolean,
    Null,
    Array(Box<SchemaType>),
    Enum(Vec<String>),
    Union(Vec<SchemaType>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PromptValue {
    Inline(String),
    Multiline(String),
    Function(FunctionCall),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub args: HashMap<String, FunctionArg>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FunctionArg {
    String(String),
    Function(Box<FunctionCall>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForEach {
    pub collection: Expression,
    pub item_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Expression {
    Literal(Vec<serde_json::Value>),
    Reference(String), // e.g., "hobbies.hobbies"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub name: String,
    pub driver: String,
    pub api_endpoint: String,
    pub models: Vec<String>,
}
