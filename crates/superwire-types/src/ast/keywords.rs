use crate::structure::{self, DslProperty, PropertyDefinition as DslPropertyDefinition};

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
pub enum ExpressionKeyword {
    Asset,
    Compact,
    Context,
}

impl ExpressionKeyword {
    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "asset" => Some(Self::Asset),
            "compact" => Some(Self::Compact),
            "context" => Some(Self::Context),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Asset => "asset",
            Self::Compact => "compact",
            Self::Context => "context",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentContextPropertyName {
    Instruction,
    Model,
}

impl AgentContextPropertyName {
    #[must_use]
    pub fn all() -> [Self; 2] {
        [Self::Instruction, Self::Model]
    }

    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        Self::all().into_iter().find(|property_name| property_name.as_str() == identifier)
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Instruction => "instruction",
            Self::Model => "model",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelDeclarationPropertyName {
    Id,
    Inference,
    Assets,
}

impl ModelDeclarationPropertyName {
    #[must_use]
    pub fn all() -> [Self; 3] {
        [Self::Id, Self::Inference, Self::Assets]
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
            Self::Assets => "assets",
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
            Self::Assets => structure::Model::new()
                .assets
                .expect("model structure should include assets")
                .definition(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelAssetKind {
    Image,
    Document,
    Video,
}

impl ModelAssetKind {
    #[must_use]
    pub fn all() -> [Self; 3] {
        [Self::Image, Self::Document, Self::Video]
    }

    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        Self::all().into_iter().find(|asset_kind| asset_kind.as_str() == identifier)
    }

    #[must_use]
    pub fn from_media_type(media_type: &str) -> Option<Self> {
        if media_type.starts_with("image/") {
            return Some(Self::Image);
        }

        if media_type.starts_with("video/") {
            return Some(Self::Video);
        }

        Some(Self::Document)
    }

    #[must_use]
    pub fn from_source(source: &str) -> Option<Self> {
        let normalized_source = source.split('?').next().unwrap_or(source).to_ascii_lowercase();

        if let Some(media_type) = source
            .strip_prefix("data:")
            .and_then(|data_source| data_source.split_once(';').map(|(media_type, _)| media_type))
        {
            return Self::from_media_type(media_type);
        }

        if [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".tiff", ".heic"]
            .iter()
            .any(|extension| normalized_source.ends_with(extension))
        {
            return Some(Self::Image);
        }

        if [".mp4", ".mpeg", ".mov", ".webm", ".avi", ".mkv"]
            .iter()
            .any(|extension| normalized_source.ends_with(extension))
        {
            return Some(Self::Video);
        }

        if [".pdf", ".txt", ".md", ".csv", ".json", ".xml", ".html", ".doc", ".docx"]
            .iter()
            .any(|extension| normalized_source.ends_with(extension))
        {
            return Some(Self::Document);
        }

        None
    }

    #[must_use]
    pub fn media_type_from_source(source: &str) -> Option<&'static str> {
        let normalized_source = source.split('?').next().unwrap_or(source).to_ascii_lowercase();

        if let Some(media_type) = source
            .strip_prefix("data:")
            .and_then(|data_source| data_source.split_once(';').map(|(media_type, _)| media_type))
        {
            return Some(match media_type {
                "image/png" => "image/png",
                "image/jpeg" => "image/jpeg",
                "image/gif" => "image/gif",
                "image/webp" => "image/webp",
                "image/bmp" => "image/bmp",
                "image/tiff" => "image/tiff",
                "image/heic" => "image/heic",
                "video/mp4" => "video/mp4",
                "video/mpeg" => "video/mpeg",
                "video/quicktime" => "video/quicktime",
                "video/webm" => "video/webm",
                "video/x-msvideo" => "video/x-msvideo",
                "video/x-matroska" => "video/x-matroska",
                "application/pdf" => "application/pdf",
                "text/plain" => "text/plain",
                "text/markdown" => "text/markdown",
                "text/csv" => "text/csv",
                "application/json" => "application/json",
                "application/xml" => "application/xml",
                "text/html" => "text/html",
                _ => return None,
            });
        }

        match normalized_source.rsplit_once('.').map(|(_, extension)| extension) {
            Some("png") => Some("image/png"),
            Some("jpg" | "jpeg") => Some("image/jpeg"),
            Some("gif") => Some("image/gif"),
            Some("webp") => Some("image/webp"),
            Some("bmp") => Some("image/bmp"),
            Some("tiff") => Some("image/tiff"),
            Some("heic") => Some("image/heic"),
            Some("mp4") => Some("video/mp4"),
            Some("mpeg") => Some("video/mpeg"),
            Some("mov") => Some("video/quicktime"),
            Some("webm") => Some("video/webm"),
            Some("avi") => Some("video/x-msvideo"),
            Some("mkv") => Some("video/x-matroska"),
            Some("pdf") => Some("application/pdf"),
            Some("txt") => Some("text/plain"),
            Some("md") => Some("text/markdown"),
            Some("csv") => Some("text/csv"),
            Some("json") => Some("application/json"),
            Some("xml") => Some("application/xml"),
            Some("html") => Some("text/html"),
            Some("doc") => Some("application/msword"),
            Some("docx") => Some("application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Document => "document",
            Self::Video => "video",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssetPropertyName {
    Type,
    MediaType,
    Title,
    Context,
    Citations,
}

impl AssetPropertyName {
    #[must_use]
    pub fn all() -> [Self; 5] {
        [Self::Type, Self::MediaType, Self::Title, Self::Context, Self::Citations]
    }

    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        Self::all()
            .into_iter()
            .find(|asset_property_name| asset_property_name.as_str() == identifier)
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Type => "type",
            Self::MediaType => "media_type",
            Self::Title => "title",
            Self::Context => "context",
            Self::Citations => "citations",
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
    Template,
}

impl BuiltinFunctionName {
    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "template" => Some(Self::Template),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Template => "template",
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

#[cfg(test)]
mod tests {
    use super::ForClauseKeyword;

    #[test]
    fn parses_for_clause_keywords_from_identifier() {
        assert_eq!(ForClauseKeyword::from_identifier("for"), Some(ForClauseKeyword::For));
        assert_eq!(ForClauseKeyword::from_identifier("in"), Some(ForClauseKeyword::In));
        assert_eq!(ForClauseKeyword::from_identifier("agent"), None);
    }

    #[test]
    fn renders_for_clause_keywords_as_str() {
        assert_eq!(ForClauseKeyword::For.as_str(), "for");
        assert_eq!(ForClauseKeyword::In.as_str(), "in");
    }
}
