use super::{McpToolSource, ObjectField, SourceSpan, TypedField};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDeclaration {
    pub name: String,
    pub description: Option<String>,
    pub max_calls: Option<u64>,
    pub source: Option<ToolSource>,
    pub imported: bool,
    pub input_fields: Vec<TypedField>,
    pub binding_fields: Vec<TypedField>,
    pub fixed_binding_fields: Vec<ObjectField>,
    pub output_fields: Vec<TypedField>,
    pub span: SourceSpan,
}

impl ToolDeclaration {
    #[must_use]
    pub fn has_untyped_mcp_output(&self) -> bool {
        self.output_fields.is_empty() && matches!(self.source, Some(ToolSource::Mcp(_)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolSource {
    Mcp(McpToolSource),
}

impl ToolSource {
    #[must_use]
    pub fn mcp_tool_name(&self) -> Option<&str> {
        match self {
            Self::Mcp(mcp_tool_source) => Some(mcp_tool_source.tool_name.as_str()),
        }
    }
}
