mod formatter;
mod parser;
mod validation;
mod visitor;

mod ast;

pub mod structure {
    pub use superwire_types::structure::*;
}

pub use ast::{
    AgentContext, AgentContextPropertyName, AgentContextReference, AgentDeclaration, AgentExpressionPropertyName, AgentFile,
    AgentFilePropertyName, AgentForLoop, AgentForLoopPattern, AgentProperty, Asset, AssetPropertyName, BuiltinFunctionArgumentName,
    BuiltinFunctionName, CallArgument, CompactAgentContext, Declaration, DeclarationKeyword, DynamicBlock, Expression, ExpressionKeyword,
    ForClauseKeyword, FunctionCall, ImportKeyword, InputDeclaration, MatchBranch, MatchExpression, McpCall, McpCallOperation,
    McpImportBindingEvaluationKind, McpImportBindings, McpImportKind, McpImportPropertyName, McpImportSource, McpPromptImportDeclaration,
    McpResourceImportDeclaration, McpServerDeclaration, McpServerPropertyName, McpToolBatchImportDeclaration, McpToolBatchImportItem,
    McpToolSchema, McpToolSource, ModelAssetKind, ModelAssetKindSupportError, ModelCallArgumentName, ModelDeclaration,
    ModelDeclarationPropertyName, ModelUsage, ModelUsagePropertyName, ModelWireApi, NamedArgument, NullFallbackExpression, ObjectField,
    OutputDeclaration, ProviderDeclaration, Reference, ReferenceAccess, ReferenceAccessKind, ReferenceKeyword, ReferenceRoot,
    ScalarTypeKeyword, SchemaDeclaration, SecretsDeclaration, SourcePosition, SourceSpan, StringTemplate, StringTemplatePart, ToolCall,
    ToolCallKeyword, ToolDeclaration, ToolDeclarationIter, ToolPropertyName, ToolSchemaIssue, ToolSource, TypeExpression,
    TypeExpressionFieldCache, TypedField, VariantCase, VariantProjectionExpression, VariantProjectionOutcome, Workflow,
};
pub use formatter::{format_workflow_source, DslFormatError};
pub use parser::{parse_workflow, DslParseError};
pub use superwire_types::{DslProperty, PropertyDefinition, PropertyValueKind};
pub use validation::{
    validate_workflow, SingletonDeclarationKind, ValidationContext, ValidationIssue, ValidationReport, WorkflowValidation,
    WorkflowValidationExt,
};
