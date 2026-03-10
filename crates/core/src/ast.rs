use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}

impl Span {
    #[must_use]
    pub const fn new(start: usize, end: usize, line: usize, column: usize) -> Self {
        Self {
            start,
            end,
            line,
            column,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workflow {
    pub providers: Vec<Provider>,
    pub schemas: Vec<NamedSchema>,
    pub agents: Vec<Agent>,
    pub input: Option<InputBlock>,
    pub output: Option<OutputBlock>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provider {
    pub name: String,
    pub driver: String,
    pub api_endpoint: Option<String>,
    pub models: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamedSchema {
    pub name: String,
    pub schema: Schema,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Schema {
    pub fields: Vec<SchemaField>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemaField {
    pub name: String,
    pub field_type: SchemaType,
    pub description: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SchemaType {
    String,
    Number,
    Boolean,
    Null,
    Array(Box<Self>),
    Enum(Vec<String>),
    Object(Vec<SchemaField>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Agent {
    pub name: String,
    pub is_terminal: bool,
    pub properties: Vec<AgentProperty>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AgentProperty {
    Model {
        value: Value,
        span: Span,
    },
    Tools {
        value: Value,
        span: Span,
    },
    Context {
        value: Value,
        span: Span,
    },
    Output {
        value: SchemaReference,
        span: Span,
    },
    Prompt {
        value: Value,
        span: Span,
    },
    ForEach {
        collection: Value,
        identifier: String,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SchemaReference {
    Named(String),
    Inline(Schema),
    InlineType {
        schema_type: SchemaType,
        description: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    String(String),
    Number(f64),
    Boolean(bool),
    Null,
    Array(Vec<Self>),
    Object(HashMap<String, Self>),
    Reference(Reference),
    FunctionCall(FunctionCall),
    Interpolated(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reference {
    Agent { agent: String, field: String },
    AgentOutput { agent: String },
    AgentContext { agent: String },
    Input { field: String },
    Schema { name: String },
    Tool { name: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: HashMap<String, Value>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputBlock {
    pub fields: Vec<InputField>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputField {
    pub name: String,
    pub field_type: SchemaType,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputBlock {
    pub fields: Vec<OutputField>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputField {
    pub name: String,
    pub value: Value,
    pub span: Span,
}

impl fmt::Display for Workflow {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Workflow(providers: {}, schemas: {}, agents: {})",
            self.providers.len(),
            self.schemas.len(),
            self.agents.len()
        )
    }
}

impl fmt::Display for Agent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Agent(name: {}, terminal: {}, properties: {})",
            self.name,
            self.is_terminal,
            self.properties.len()
        )
    }
}

impl fmt::Display for Provider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Provider(name: {}, driver: {}, models: {})",
            self.name,
            self.driver,
            self.models.len()
        )
    }
}

impl fmt::Display for Schema {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Schema(fields: {})", self.fields.len())
    }
}
