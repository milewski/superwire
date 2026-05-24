mod formatter;
pub mod macros;
mod parser;
mod visitor;

mod ast;
pub mod diagnostic;
#[cfg(test)]
pub mod testing;

pub mod structure {
    pub use superwire_types::structure::*;
}

pub use ast::{
    AgentDeclaration, AgentExpressionPropertyName, AgentForLoop, AgentForLoopPattern, AgentProperty, Asset, AssetPropertyName,
    BuiltinFunctionArgumentName, BuiltinFunctionName, CallArgument, Declaration, DeclarationKeyword, DynamicBlock, Expression,
    ExpressionKeyword, ForClauseKeyword, FunctionCall, ImportKeyword, InputDeclaration, MatchBranch, MatchExpression, McpCall,
    McpCallOperation, McpImportBindingEvaluationKind, McpImportBindings, McpImportKind, McpImportPropertyName, McpImportSource,
    McpPromptImportDeclaration, McpResourceImportDeclaration, McpServerDeclaration, McpServerPropertyName, McpToolBatchImportDeclaration,
    McpToolBatchImportItem, McpToolSource, ModelAssetKind, ModelAssetKindParseError, ModelCallArgumentName, ModelDeclaration,
    ModelDeclarationPropertyName, ModelUsage, ModelUsagePropertyName, NamedArgument, NullFallbackExpression, ObjectField,
    OutputDeclaration, ProviderDeclaration, Reference, ReferenceAccess, ReferenceAccessKind, ReferenceKeyword, ReferenceRoot,
    SchemaDeclaration, SecretsDeclaration, SourcePosition, SourceSpan, StringTemplate, StringTemplatePart, ToolCall, ToolCallKeyword,
    ToolDeclaration, ToolDeclarationIter, ToolPropertyName, ToolSource, TypeExpression, TypeExpressionFieldCache, TypedField, VariantCase,
    VariantProjectionExpression, Workflow,
};
pub use formatter::{format_workflow_source, DslFormatError};
pub use parser::{parse_workflow, DslParseError};
pub use superwire_types::{DslProperty, PropertyDefinition, PropertyValueKind};
