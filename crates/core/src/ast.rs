use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct WorkflowDocument {
    pub agents: Vec<AgentDefinition>,
    pub schemas: Vec<SchemaDefinition>,
    pub providers: Vec<ProviderDefinition>,
    pub input: Option<SchemaDefinition>,
    pub output: Option<Expression>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentDefinition {
    pub name: String,
    pub is_terminal: bool,
    pub model: Option<ModelReference>,
    pub tools: Vec<String>,
    pub context: Option<ContextSource>,
    pub output: Option<OutputDefinition>,
    pub prompt: Option<Expression>,
    pub for_each: Option<ForEachBinding>,
    pub properties: IndexMap<String, Expression>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OutputDefinition {
    SchemaReference(String),
    Inline(SchemaDefinition),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelReference {
    pub provider: String,
    pub model: String,
    pub raw: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ContextSource {
    Full(Reference),
    Summary(Reference),
    Expression(Box<Expression>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchemaDefinition {
    pub name: Option<String>,
    pub fields: Vec<SchemaField>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchemaField {
    pub name: String,
    pub ty: SchemaType,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SchemaType {
    String,
    Number,
    Boolean,
    Null,
    Array(Box<SchemaType>),
    Union(Vec<SchemaType>),
    LiteralString(String),
    Reference(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderDefinition {
    pub name: String,
    pub driver: String,
    pub api_endpoint: Option<String>,
    pub models: Vec<String>,
    pub properties: IndexMap<String, Expression>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Expression {
    String(String),
    MultilineString(String),
    Number(f64),
    Boolean(bool),
    Null,
    Array(Vec<Expression>),
    Object(IndexMap<String, Expression>),
    Identifier(String),
    Reference(Reference),
    FunctionCall(FunctionCall),
    InlineSchema(SchemaDefinition),
    ForEach(ForEachBinding),
    InterpolatedString(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Reference {
    pub segments: Vec<String>,
}

impl Reference {
    pub fn as_string(&self) -> String {
        self.segments.join(".")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FunctionCall {
    pub name: String,
    pub target: Box<Expression>,
    pub arguments: IndexMap<String, Expression>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ForEachBinding {
    pub collection: Box<Expression>,
    pub binding: String,
}
