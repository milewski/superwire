use crate::dsl::structure::{self, DslProperty, PropertyDefinition as DslPropertyDefinition};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeclarationKeyword {
    Provider,
    Model,
    Mcp,
    Secrets,
    Input,
    Schema,
    Tool,
    Resource,
    Prompt,
    Dynamic,
    Agent,
    Output,
}

impl DeclarationKeyword {
    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "provider" => Some(Self::Provider),
            "model" => Some(Self::Model),
            "mcp" => Some(Self::Mcp),
            "secrets" => Some(Self::Secrets),
            "input" => Some(Self::Input),
            "schema" => Some(Self::Schema),
            "tool" => Some(Self::Tool),
            "resource" => Some(Self::Resource),
            "prompt" => Some(Self::Prompt),
            "dynamic" => Some(Self::Dynamic),
            "agent" => Some(Self::Agent),
            "output" => Some(Self::Output),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Provider => "provider",
            Self::Model => "model",
            Self::Mcp => "mcp",
            Self::Secrets => "secrets",
            Self::Input => "input",
            Self::Schema => "schema",
            Self::Tool => "tool",
            Self::Resource => "resource",
            Self::Prompt => "prompt",
            Self::Dynamic => "dynamic",
            Self::Agent => "agent",
            Self::Output => "output",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ForClauseKeyword {
    For,
    In,
}

impl ForClauseKeyword {
    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "for" => Some(Self::For),
            "in" => Some(Self::In),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::For => "for",
            Self::In => "in",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportKeyword {
    From,
    As,
}

impl ImportKeyword {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::From => "from",
            Self::As => "as",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelDeclarationPropertyName {
    Id,
    Inference,
}

impl ModelDeclarationPropertyName {
    #[must_use]
    pub fn all() -> [Self; 2] {
        [Self::Id, Self::Inference]
    }

    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        Self::all().into_iter().find(|property_name| property_name.as_str() == identifier)
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Id => "id",
            Self::Inference => "inference",
        }
    }

    #[must_use]
    pub fn definition(self) -> DslPropertyDefinition {
        match self {
            Self::Id => structure::Model::new().id.definition(),
            Self::Inference => structure::Model::new()
                .inference
                .expect("model structure should include inference")
                .definition(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelUsagePropertyName {
    Inference,
}

impl ModelUsagePropertyName {
    #[must_use]
    pub fn all() -> [Self; 1] {
        [Self::Inference]
    }

    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        Self::all().into_iter().find(|property_name| property_name.as_str() == identifier)
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Inference => "inference",
        }
    }

    #[must_use]
    pub fn definition(self) -> DslPropertyDefinition {
        match self {
            Self::Inference => structure::ModelUsage::new()
                .inference
                .expect("model usage structure should include inference")
                .definition(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum McpServerPropertyName {
    Endpoint,
    Headers,
}

impl McpServerPropertyName {
    #[must_use]
    pub fn all() -> [Self; 2] {
        [Self::Endpoint, Self::Headers]
    }

    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        Self::all()
            .into_iter()
            .find(|mcp_server_property_name| mcp_server_property_name.as_str() == identifier)
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Endpoint => "endpoint",
            Self::Headers => "headers",
        }
    }

    #[must_use]
    pub fn definition(self) -> DslPropertyDefinition {
        match self {
            Self::Endpoint => structure::McpServer::new().endpoint.definition(),
            Self::Headers => structure::McpServer::new()
                .headers
                .expect("mcp server structure should include headers")
                .definition(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolPropertyName {
    Description,
    MaxCalls,
    Input,
    Bindings,
    Output,
}

impl ToolPropertyName {
    #[must_use]
    pub fn all() -> [Self; 5] {
        [Self::Description, Self::MaxCalls, Self::Input, Self::Bindings, Self::Output]
    }

    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        Self::all()
            .into_iter()
            .find(|tool_property_name| tool_property_name.as_str() == identifier)
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Description => "description",
            Self::MaxCalls => "max_calls",
            Self::Input => "input",
            Self::Bindings => "bindings",
            Self::Output => "output",
        }
    }

    #[must_use]
    pub fn definition(self) -> DslPropertyDefinition {
        match self {
            Self::Description => structure::Tool::new()
                .description
                .expect("tool structure should include description")
                .definition(),
            Self::MaxCalls => structure::Tool::new()
                .max_calls
                .expect("tool structure should include max_calls")
                .definition(),
            Self::Input => structure::Tool::new()
                .input
                .expect("tool structure should include input")
                .definition(),
            Self::Bindings => structure::Tool::new()
                .bindings
                .expect("tool structure should include bindings")
                .definition(),
            Self::Output => structure::Tool::new()
                .output
                .expect("tool structure should include output")
                .definition(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum McpImportPropertyName {
    Bindings,
}

impl McpImportPropertyName {
    #[must_use]
    pub fn all() -> [Self; 1] {
        [Self::Bindings]
    }

    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        Self::all().into_iter().find(|property_name| property_name.as_str() == identifier)
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bindings => "bindings",
        }
    }

    #[must_use]
    pub fn definition(self) -> DslPropertyDefinition {
        match self {
            Self::Bindings => structure::McpImport::new()
                .bindings
                .expect("mcp import structure should include bindings")
                .definition(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolCallPropertyName {
    Input,
    Bindings,
    MaxCalls,
}

impl ToolCallPropertyName {
    #[must_use]
    pub fn all() -> [Self; 3] {
        [Self::Input, Self::Bindings, Self::MaxCalls]
    }

    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        Self::all().into_iter().find(|property_name| property_name.as_str() == identifier)
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Bindings => "bindings",
            Self::MaxCalls => "max_calls",
        }
    }

    #[must_use]
    pub fn definition(self) -> DslPropertyDefinition {
        match self {
            Self::Input => structure::ToolCall::new()
                .input
                .expect("tool call structure should include input")
                .definition(),
            Self::Bindings => structure::ToolCall::new()
                .bindings
                .expect("tool call structure should include bindings")
                .definition(),
            Self::MaxCalls => structure::ToolCall::new()
                .max_calls
                .expect("tool call structure should include max_calls")
                .definition(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum McpToolBatchImportPropertyName {
    Bindings,
    MaxCalls,
}

impl McpToolBatchImportPropertyName {
    #[must_use]
    pub fn all() -> [Self; 2] {
        [Self::Bindings, Self::MaxCalls]
    }

    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        Self::all().into_iter().find(|property_name| property_name.as_str() == identifier)
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bindings => ToolPropertyName::Bindings.as_str(),
            Self::MaxCalls => ToolPropertyName::MaxCalls.as_str(),
        }
    }

    #[must_use]
    pub fn definition(self) -> DslPropertyDefinition {
        match self {
            Self::Bindings => structure::McpToolBatchImport::new()
                .bindings
                .expect("mcp tool batch import structure should include bindings")
                .definition(),
            Self::MaxCalls => structure::McpToolBatchImport::new()
                .max_calls
                .expect("mcp tool batch import structure should include max_calls")
                .definition(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentExpressionPropertyName {
    Model,
    Instruction,
    Context,
    Uses,
}

impl AgentExpressionPropertyName {
    #[must_use]
    pub fn all() -> [Self; 4] {
        [Self::Model, Self::Instruction, Self::Context, Self::Uses]
    }

    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        Self::from_agent_property_identifier(identifier)
    }

    #[must_use]
    pub fn from_agent_property_identifier(identifier: &str) -> Option<Self> {
        let agent = structure::Agent::new();

        if agent.property_is_model(identifier) {
            return Some(Self::Model);
        }

        if agent.property_is_instruction(identifier) {
            return Some(Self::Instruction);
        }

        if agent.property_is_context(identifier) {
            return Some(Self::Context);
        }

        if agent.property_is_uses(identifier) {
            return Some(Self::Uses);
        }

        None
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::Instruction => "instruction",
            Self::Context => "context",
            Self::Uses => "uses",
        }
    }

    #[must_use]
    pub fn definition(self) -> DslPropertyDefinition {
        let agent = structure::Agent::new();

        match self {
            Self::Model => agent.model.definition(),
            Self::Instruction => agent.instruction.definition(),
            Self::Context => agent.context.expect("agent structure should include context").definition(),
            Self::Uses => agent.uses[0].definition(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReferenceKeyword {
    Agent,
    Dynamic,
    Input,
    Model,
    Secrets,
    Tool,
    Resource,
    Prompt,
}

impl ReferenceKeyword {
    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "agent" => Some(Self::Agent),
            "dynamic" => Some(Self::Dynamic),
            "input" => Some(Self::Input),
            "model" => Some(Self::Model),
            "secrets" => Some(Self::Secrets),
            "tool" => Some(Self::Tool),
            "resource" => Some(Self::Resource),
            "prompt" => Some(Self::Prompt),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Dynamic => "dynamic",
            Self::Input => "input",
            Self::Model => "model",
            Self::Secrets => "secrets",
            Self::Tool => "tool",
            Self::Resource => "resource",
            Self::Prompt => "prompt",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinFunctionName {
    Context,
    Template,
    Compact,
}

impl BuiltinFunctionName {
    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "context" => Some(Self::Context),
            "template" => Some(Self::Template),
            "compact" => Some(Self::Compact),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Context => "context",
            Self::Template => "template",
            Self::Compact => "compact",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolCallKeyword {
    Call,
}

impl ToolCallKeyword {
    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "call" => Some(Self::Call),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Call => "call",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinFunctionArgumentName {
    Agent,
}

impl BuiltinFunctionArgumentName {
    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "agent" => Some(Self::Agent),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelCallArgumentName {
    Model,
}

impl ModelCallArgumentName {
    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "model" => Some(Self::Model),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Model => "model",
        }
    }
}
