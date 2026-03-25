mod ast;
pub mod macros;
mod parser;
mod validation;
mod visitor;

pub use ast::{
    AgentDeclaration, AgentForLoop, AgentProperty, CallArgument, Declaration, Expression, FunctionCall, InputDeclaration, NamedArgument,
    ObjectField, OutputDeclaration, ProviderDeclaration, Reference, ReferenceAccess, ReferenceKeyword, ReferenceRoot, SchemaDeclaration,
    SecretsDeclaration, SourcePosition, SourceSpan, StringTemplate, StringTemplatePart, TypeExpression, TypedField, Workflow,
};
pub use parser::{parse_workflow, DslParseError};
pub use validation::{validate_workflow, SingletonDeclarationKind, ValidationContext, ValidationIssue, ValidationReport};
