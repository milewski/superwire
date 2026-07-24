use super::{
    AgentContext, AssetPropertyName, BuiltinFunctionArgumentName, BuiltinFunctionName, ModelCallArgumentName, Reference, ReferenceKeyword,
    SourceSpan, TypeExpression, TypedField,
};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::hash::BuildHasher;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expression {
    StringLiteral(String),
    StringTemplate(StringTemplate),
    NumberLiteral(String),
    BooleanLiteral(bool),
    NullLiteral,
    Reference(Reference),
    FunctionCall(FunctionCall),
    AgentContext(AgentContext),
    Asset(Asset),
    ToolCall(ToolCall),
    McpCall(McpCall),
    NullFallback(NullFallbackExpression),
    VariantProjection(VariantProjectionExpression),
    Match(MatchExpression),
    ArrayLiteral(Vec<Expression>),
    ObjectLiteral(Vec<ObjectField>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub source: Box<Expression>,
    pub options: Vec<ObjectField>,
    pub span: SourceSpan,
}

impl Asset {
    #[must_use]
    pub fn option(&self, option_name: AssetPropertyName) -> Option<&ObjectField> {
        self.options
            .iter()
            .find(|option| AssetPropertyName::from_identifier(option.name.as_str()) == Some(option_name))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NullFallbackExpression {
    pub value: Box<Expression>,
    pub fallback: Box<Expression>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantProjectionExpression {
    pub value: Reference,
    pub case_name: String,
    pub field_path: Vec<String>,
    pub span: SourceSpan,
    resolved_discriminator: OnceLock<String>,
}

impl VariantProjectionExpression {
    #[must_use]
    pub fn new(value: Reference, case_name: String, field_path: Vec<String>, span: SourceSpan) -> Self {
        Self {
            value,
            case_name,
            field_path,
            span,
            resolved_discriminator: OnceLock::new(),
        }
    }

    #[must_use]
    pub fn resolve_discriminator(&self, discriminator: &str) -> bool {
        if let Some(resolved_discriminator) = self.resolved_discriminator.get() {
            return resolved_discriminator == discriminator;
        }

        self.resolved_discriminator.set(discriminator.to_string()).is_ok()
    }

    #[must_use]
    pub fn resolved_discriminator(&self) -> Option<&str> {
        self.resolved_discriminator.get().map(String::as_str)
    }

    #[must_use]
    pub fn project_value(&self, value: Value) -> Option<VariantProjectionOutcome> {
        let discriminator = self.resolved_discriminator()?;

        Some(Self::project_resolved_value(
            value,
            discriminator,
            &self.case_name,
            &self.field_path,
        ))
    }

    fn project_resolved_value(value: Value, discriminator: &str, case_name: &str, field_path: &[String]) -> VariantProjectionOutcome {
        let Some(object_fields) = value.as_object() else {
            return VariantProjectionOutcome::NoMatch;
        };
        let has_matching_discriminator = object_fields.get(discriminator).and_then(Value::as_str) == Some(case_name);

        if !has_matching_discriminator {
            return VariantProjectionOutcome::NoMatch;
        }

        let Some((first_field_name, remaining_field_path)) = field_path.split_first() else {
            return VariantProjectionOutcome::Matched(value);
        };
        let Some(mut current_value) = object_fields.get(first_field_name) else {
            return VariantProjectionOutcome::Matched(Value::Null);
        };

        for field_name in remaining_field_path {
            let Some(current_object_fields) = current_value.as_object() else {
                return VariantProjectionOutcome::Matched(Value::Null);
            };
            let Some(next_value) = current_object_fields.get(field_name) else {
                return VariantProjectionOutcome::Matched(Value::Null);
            };

            current_value = next_value;
        }

        VariantProjectionOutcome::Matched(current_value.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchBranchStructureError {
    DuplicateVariant {
        case_name: String,
        first_span: SourceSpan,
        duplicate_span: SourceSpan,
    },
    MultipleFallback {
        first_span: SourceSpan,
        duplicate_span: SourceSpan,
    },
    NonFinalFallback {
        span: SourceSpan,
    },
}

impl MatchBranchStructureError {
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::DuplicateVariant { case_name, .. } => format!("duplicate match case `{case_name}`"),
            Self::MultipleFallback { .. } => "match expression has more than one fallback branch".to_string(),
            Self::NonFinalFallback { .. } => "match fallback branch must be last".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchExpression {
    pub value: Box<Expression>,
    pub branches: Vec<MatchBranch>,
    pub span: SourceSpan,
    resolved_discriminator: OnceLock<String>,
}

impl MatchExpression {
    #[must_use]
    pub fn new(value: Expression, branches: Vec<MatchBranch>, span: SourceSpan) -> Self {
        Self {
            value: Box::new(value),
            branches,
            span,
            resolved_discriminator: OnceLock::new(),
        }
    }

    #[must_use]
    pub fn resolve_discriminator(&self, discriminator: &str) -> bool {
        if let Some(resolved_discriminator) = self.resolved_discriminator.get() {
            return resolved_discriminator == discriminator;
        }

        self.resolved_discriminator.set(discriminator.to_string()).is_ok()
    }

    #[must_use]
    pub fn resolved_discriminator(&self) -> Option<&str> {
        self.resolved_discriminator.get().map(String::as_str)
    }

    #[must_use]
    pub fn project_variant_branch(&self, value: Value, case_name: &str, field_path: &[String]) -> Option<VariantProjectionOutcome> {
        let discriminator = self.resolved_discriminator()?;

        Some(VariantProjectionExpression::project_resolved_value(
            value,
            discriminator,
            case_name,
            field_path,
        ))
    }

    pub fn validate_branch_structure(&self) -> Result<(), MatchBranchStructureError> {
        let mut first_variant_spans = HashMap::new();
        let mut first_fallback_span = None;

        for branch in &self.branches {
            match branch {
                MatchBranch::Variant {
                    case_name,
                    field_path: _,
                    span,
                } => {
                    if let Some(first_span) = first_variant_spans.get(case_name) {
                        return Err(MatchBranchStructureError::DuplicateVariant {
                            case_name: case_name.clone(),
                            first_span: *first_span,
                            duplicate_span: *span,
                        });
                    }

                    first_variant_spans.insert(case_name.clone(), *span);
                }
                MatchBranch::Fallback { value: _, span } => {
                    if let Some(first_span) = first_fallback_span {
                        return Err(MatchBranchStructureError::MultipleFallback {
                            first_span,
                            duplicate_span: *span,
                        });
                    }

                    first_fallback_span = Some(*span);
                }
            }
        }

        if !self.branches.last().is_some_and(MatchBranch::is_fallback) {
            if let Some(span) = first_fallback_span {
                return Err(MatchBranchStructureError::NonFinalFallback { span });
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchBranch {
    Variant {
        case_name: String,
        field_path: Vec<String>,
        span: SourceSpan,
    },
    Fallback {
        value: Expression,
        span: SourceSpan,
    },
}

impl MatchBranch {
    #[must_use]
    pub fn is_fallback(&self) -> bool {
        matches!(self, Self::Fallback { value: _, span: _ })
    }

    #[must_use]
    pub fn case_name(&self) -> Option<&str> {
        match self {
            Self::Variant {
                case_name,
                field_path: _,
                span: _,
            } => Some(case_name),
            Self::Fallback { value: _, span: _ } => None,
        }
    }

    #[must_use]
    pub fn fallback_value(&self) -> Option<&Expression> {
        match self {
            Self::Fallback { value, span: _ } => Some(value),
            Self::Variant {
                case_name: _,
                field_path: _,
                span: _,
            } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariantProjectionOutcome {
    NoMatch,
    Matched(Value),
}

impl Expression {
    #[must_use]
    pub fn referenced_names_for_keyword(&self, reference_keyword: ReferenceKeyword) -> Vec<String> {
        let Self::ArrayLiteral(expressions) = self else {
            return Vec::new();
        };

        expressions
            .iter()
            .filter_map(|expression| expression.direct_name_for_keyword(reference_keyword))
            .collect()
    }

    #[must_use]
    pub fn source_span(&self) -> Option<SourceSpan> {
        match self {
            Self::Reference(reference) => Some(reference.span),
            Self::FunctionCall(function_call) => Some(function_call.callee.span),
            Self::AgentContext(agent_context) => Some(agent_context.span()),
            Self::Asset(asset) => Some(asset.span),
            Self::ToolCall(tool_call) => Some(tool_call.span),
            Self::McpCall(mcp_call) => Some(mcp_call.span),
            Self::VariantProjection(variant_projection) => Some(variant_projection.span),
            Self::Match(match_expression) => Some(match_expression.span),
            Self::ObjectLiteral(object_fields) => object_fields.first().map(|object_field| object_field.span),
            Self::StringTemplate(string_template) => string_template.parts.iter().find_map(|string_template_part| {
                let StringTemplatePart::Interpolation(interpolation_expression) = string_template_part else {
                    return None;
                };

                interpolation_expression.source_span()
            }),
            Self::NullFallback(null_fallback) => null_fallback.value.source_span().or_else(|| null_fallback.fallback.source_span()),
            Self::ArrayLiteral(expressions) => expressions.iter().find_map(Self::source_span),
            Self::StringLiteral(_) | Self::NumberLiteral(_) | Self::BooleanLiteral(_) | Self::NullLiteral => None,
        }
    }

    #[must_use]
    pub fn direct_reference(&self) -> Option<&Reference> {
        match self {
            Self::Reference(reference) => Some(reference),
            Self::ToolCall(tool_call) => Some(&tool_call.callee),
            Self::FunctionCall(_)
            | Self::AgentContext(_)
            | Self::Asset(_)
            | Self::McpCall(_)
            | Self::NullFallback(_)
            | Self::VariantProjection(_)
            | Self::Match(_)
            | Self::StringLiteral(_)
            | Self::StringTemplate(_)
            | Self::NumberLiteral(_)
            | Self::BooleanLiteral(_)
            | Self::NullLiteral
            | Self::ArrayLiteral(_)
            | Self::ObjectLiteral(_) => None,
        }
    }

    #[must_use]
    pub fn direct_tool_name(&self) -> Option<&str> {
        self.direct_reference_for_keyword(ReferenceKeyword::Tool)
            .and_then(Reference::tool_name)
    }

    #[must_use]
    pub fn direct_reference_for_keyword(&self, reference_keyword: ReferenceKeyword) -> Option<&Reference> {
        let reference = match self {
            Self::Reference(reference) => reference,
            Self::FunctionCall(function_call) => &function_call.callee,
            Self::ToolCall(tool_call) => &tool_call.callee,
            Self::AgentContext(_)
            | Self::Asset(_)
            | Self::McpCall(_)
            | Self::NullFallback(_)
            | Self::VariantProjection(_)
            | Self::Match(_)
            | Self::StringLiteral(_)
            | Self::StringTemplate(_)
            | Self::NumberLiteral(_)
            | Self::BooleanLiteral(_)
            | Self::NullLiteral
            | Self::ArrayLiteral(_)
            | Self::ObjectLiteral(_) => return None,
        };

        reference
            .is_direct_required_reference_to_keyword(reference_keyword)
            .then_some(reference)
    }

    #[must_use]
    pub fn direct_name_for_keyword(&self, reference_keyword: ReferenceKeyword) -> Option<String> {
        self.direct_reference_for_keyword(reference_keyword)
            .and_then(|reference| reference.direct_required_name_for_keyword(reference_keyword))
            .map(str::to_string)
    }

    #[must_use]
    pub fn agent_tool_binding_fields(&self) -> &[ObjectField] {
        match self {
            Self::ToolCall(tool_call) => tool_call.agent_tool_binding_fields(),
            Self::Reference(_)
            | Self::FunctionCall(_)
            | Self::AgentContext(_)
            | Self::Asset(_)
            | Self::McpCall(_)
            | Self::NullFallback(_)
            | Self::VariantProjection(_)
            | Self::Match(_)
            | Self::StringLiteral(_)
            | Self::StringTemplate(_)
            | Self::NumberLiteral(_)
            | Self::BooleanLiteral(_)
            | Self::NullLiteral
            | Self::ArrayLiteral(_)
            | Self::ObjectLiteral(_) => &[],
        }
    }

    #[must_use]
    pub fn max_calls_override(&self) -> Option<u64> {
        match self {
            Self::ToolCall(tool_call) => tool_call.max_calls,
            Self::Reference(_)
            | Self::FunctionCall(_)
            | Self::AgentContext(_)
            | Self::Asset(_)
            | Self::McpCall(_)
            | Self::NullFallback(_)
            | Self::VariantProjection(_)
            | Self::Match(_)
            | Self::StringLiteral(_)
            | Self::StringTemplate(_)
            | Self::NumberLiteral(_)
            | Self::BooleanLiteral(_)
            | Self::NullLiteral
            | Self::ArrayLiteral(_)
            | Self::ObjectLiteral(_) => None,
        }
    }

    #[must_use]
    pub fn tool_calls(&self) -> Vec<&ToolCall> {
        let mut tool_calls = Vec::new();
        self.collect_tool_calls(&mut tool_calls);

        tool_calls
    }

    #[must_use]
    pub fn tool_references(&self) -> Vec<&Reference> {
        let mut tool_references = Vec::new();
        self.collect_tool_references(&mut tool_references);

        tool_references
    }

    pub fn collect_tool_references<'expression>(&'expression self, tool_references: &mut Vec<&'expression Reference>) {
        match self {
            Self::Reference(reference) => {
                if reference.is_direct_required_reference_to_keyword(ReferenceKeyword::Tool) {
                    tool_references.push(reference);
                }
            }
            Self::ToolCall(tool_call) => {
                if tool_call.callee.is_direct_required_reference_to_keyword(ReferenceKeyword::Tool) {
                    tool_references.push(&tool_call.callee);
                }

                for input_field in &tool_call.input_fields {
                    input_field.value.collect_tool_references(tool_references);
                }

                for binding_field in &tool_call.binding_fields {
                    binding_field.value.collect_tool_references(tool_references);
                }
            }
            Self::FunctionCall(function_call) => {
                if function_call.callee.is_direct_required_reference_to_keyword(ReferenceKeyword::Tool) {
                    tool_references.push(&function_call.callee);
                }

                for call_argument in &function_call.arguments {
                    call_argument.expression().collect_tool_references(tool_references);
                }
            }
            Self::AgentContext(agent_context) => {
                if agent_context
                    .reference()
                    .is_direct_required_reference_to_keyword(ReferenceKeyword::Tool)
                {
                    tool_references.push(agent_context.reference());
                }

                if let Self::AgentContext(AgentContext::Compact(compact_agent_context)) = self {
                    for property in &compact_agent_context.properties {
                        property.value.collect_tool_references(tool_references);
                    }
                }
            }
            Self::Asset(asset) => {
                asset.source.collect_tool_references(tool_references);

                for option in &asset.options {
                    option.value.collect_tool_references(tool_references);
                }
            }
            Self::McpCall(mcp_call) => {
                if mcp_call.callee.is_direct_required_reference_to_keyword(ReferenceKeyword::Tool) {
                    tool_references.push(&mcp_call.callee);
                }

                for parameter_field in &mcp_call.parameter_fields {
                    parameter_field.value.collect_tool_references(tool_references);
                }
            }
            Self::NullFallback(null_fallback) => {
                null_fallback.value.collect_tool_references(tool_references);
                null_fallback.fallback.collect_tool_references(tool_references);
            }
            Self::Match(match_expression) => {
                match_expression.value.collect_tool_references(tool_references);

                for match_branch in &match_expression.branches {
                    if let MatchBranch::Fallback { value, span: _ } = match_branch {
                        value.collect_tool_references(tool_references);
                    }
                }
            }
            Self::ArrayLiteral(item_expressions) => {
                for item_expression in item_expressions {
                    item_expression.collect_tool_references(tool_references);
                }
            }
            Self::ObjectLiteral(object_fields) => {
                for object_field in object_fields {
                    object_field.value.collect_tool_references(tool_references);
                }
            }
            Self::StringTemplate(string_template) => {
                for string_template_part in &string_template.parts {
                    if let StringTemplatePart::Interpolation(interpolation_expression) = string_template_part {
                        interpolation_expression.collect_tool_references(tool_references);
                    }
                }
            }
            Self::VariantProjection(variant_projection) => {
                if variant_projection
                    .value
                    .is_direct_required_reference_to_keyword(ReferenceKeyword::Tool)
                {
                    tool_references.push(&variant_projection.value);
                }
            }
            Self::NumberLiteral(_) | Self::BooleanLiteral(_) | Self::NullLiteral | Self::StringLiteral(_) => {}
        }
    }

    #[must_use]
    pub fn tool_names(&self) -> Vec<&str> {
        self.tool_references().into_iter().filter_map(Reference::tool_name).collect()
    }

    #[must_use]
    pub fn references_secret(&self) -> bool {
        match self {
            Self::Reference(reference) => reference.is_secret_reference(),
            Self::FunctionCall(function_call) => {
                function_call.callee.is_secret_reference()
                    || function_call
                        .arguments
                        .iter()
                        .any(|call_argument| call_argument.expression().references_secret())
            }
            Self::AgentContext(agent_context) => agent_context.references_secret(),
            Self::Asset(asset) => asset.source.references_secret() || asset.options.iter().any(|option| option.value.references_secret()),
            Self::ToolCall(tool_call) => {
                tool_call.callee.is_secret_reference()
                    || tool_call
                        .input_fields
                        .iter()
                        .any(|input_field| input_field.value.references_secret())
                    || tool_call
                        .binding_fields
                        .iter()
                        .any(|binding_field| binding_field.value.references_secret())
            }
            Self::McpCall(mcp_call) => {
                mcp_call.callee.is_secret_reference()
                    || mcp_call
                        .parameter_fields
                        .iter()
                        .any(|parameter_field| parameter_field.value.references_secret())
            }
            Self::NullFallback(null_fallback) => null_fallback.value.references_secret() || null_fallback.fallback.references_secret(),
            Self::VariantProjection(variant_projection) => variant_projection.value.is_secret_reference(),
            Self::Match(match_expression) => {
                match_expression.value.references_secret()
                    || match_expression.branches.iter().any(|match_branch| match match_branch {
                        MatchBranch::Fallback { value, span: _ } => value.references_secret(),
                        MatchBranch::Variant {
                            case_name: _,
                            field_path: _,
                            span: _,
                        } => false,
                    })
            }
            Self::ArrayLiteral(item_expressions) => item_expressions.iter().any(Self::references_secret),
            Self::ObjectLiteral(object_fields) => object_fields.iter().any(|object_field| object_field.value.references_secret()),
            Self::StringTemplate(string_template) => string_template.parts.iter().any(|string_template_part| match string_template_part {
                StringTemplatePart::Interpolation(interpolation_expression) => interpolation_expression.references_secret(),
                StringTemplatePart::Text(_) => false,
            }),
            Self::StringLiteral(_) | Self::NumberLiteral(_) | Self::BooleanLiteral(_) | Self::NullLiteral => false,
        }
    }

    fn collect_tool_calls<'expression>(&'expression self, tool_calls: &mut Vec<&'expression ToolCall>) {
        match self {
            Self::ToolCall(tool_call) => {
                tool_calls.push(tool_call);

                for input_field in &tool_call.input_fields {
                    input_field.value.collect_tool_calls(tool_calls);
                }

                for binding_field in &tool_call.binding_fields {
                    binding_field.value.collect_tool_calls(tool_calls);
                }
            }
            Self::StringTemplate(string_template) => {
                for string_template_part in &string_template.parts {
                    if let StringTemplatePart::Interpolation(interpolation_expression) = string_template_part {
                        interpolation_expression.collect_tool_calls(tool_calls);
                    }
                }
            }
            Self::FunctionCall(function_call) => {
                for call_argument in &function_call.arguments {
                    call_argument.expression().collect_tool_calls(tool_calls);
                }
            }
            Self::Asset(asset) => {
                asset.source.collect_tool_calls(tool_calls);

                for option in &asset.options {
                    option.value.collect_tool_calls(tool_calls);
                }
            }
            Self::McpCall(mcp_call) => {
                for parameter_field in &mcp_call.parameter_fields {
                    parameter_field.value.collect_tool_calls(tool_calls);
                }
            }
            Self::NullFallback(null_fallback) => {
                null_fallback.value.collect_tool_calls(tool_calls);
                null_fallback.fallback.collect_tool_calls(tool_calls);
            }
            Self::Match(match_expression) => {
                match_expression.value.collect_tool_calls(tool_calls);

                for match_branch in &match_expression.branches {
                    if let MatchBranch::Fallback { value, .. } = match_branch {
                        value.collect_tool_calls(tool_calls);
                    }
                }
            }
            Self::ArrayLiteral(item_expressions) => {
                for item_expression in item_expressions {
                    item_expression.collect_tool_calls(tool_calls);
                }
            }
            Self::ObjectLiteral(object_fields) => {
                for object_field in object_fields {
                    object_field.value.collect_tool_calls(tool_calls);
                }
            }
            Self::AgentContext(agent_context) => {
                if let AgentContext::Compact(compact_agent_context) = agent_context {
                    for property in &compact_agent_context.properties {
                        property.value.collect_tool_calls(tool_calls);
                    }
                }
            }
            Self::NumberLiteral(_)
            | Self::BooleanLiteral(_)
            | Self::NullLiteral
            | Self::StringLiteral(_)
            | Self::Reference(_)
            | Self::VariantProjection(_) => {}
        }
    }

    pub fn collect_agent_dependencies<HashBuilder: BuildHasher>(&self, agent_dependencies: &mut HashSet<String, HashBuilder>) {
        match self {
            Self::Reference(reference) => {
                reference.collect_agent_dependency(agent_dependencies);
            }
            Self::FunctionCall(function_call) => {
                function_call.callee.collect_agent_dependency(agent_dependencies);

                for call_argument in &function_call.arguments {
                    call_argument.expression().collect_agent_dependencies(agent_dependencies);
                }
            }
            Self::AgentContext(agent_context) => {
                agent_context.collect_agent_dependencies(agent_dependencies);
            }
            Self::Asset(asset) => {
                asset.source.collect_agent_dependencies(agent_dependencies);

                for option in &asset.options {
                    option.value.collect_agent_dependencies(agent_dependencies);
                }
            }
            Self::ToolCall(tool_call) => {
                tool_call.callee.collect_agent_dependency(agent_dependencies);

                for object_field in &tool_call.input_fields {
                    object_field.value.collect_agent_dependencies(agent_dependencies);
                }

                for object_field in &tool_call.binding_fields {
                    object_field.value.collect_agent_dependencies(agent_dependencies);
                }
            }
            Self::McpCall(mcp_call) => {
                mcp_call.callee.collect_agent_dependency(agent_dependencies);

                for object_field in &mcp_call.parameter_fields {
                    object_field.value.collect_agent_dependencies(agent_dependencies);
                }
            }
            Self::NullFallback(null_fallback) => {
                null_fallback.value.collect_agent_dependencies(agent_dependencies);
                null_fallback.fallback.collect_agent_dependencies(agent_dependencies);
            }
            Self::VariantProjection(variant_projection) => {
                variant_projection.value.collect_agent_dependency(agent_dependencies);
            }
            Self::Match(match_expression) => {
                match_expression.value.collect_agent_dependencies(agent_dependencies);

                for branch in &match_expression.branches {
                    if let MatchBranch::Fallback { value, span: _ } = branch {
                        value.collect_agent_dependencies(agent_dependencies);
                    }
                }
            }
            Self::ArrayLiteral(array_items) => {
                for array_item in array_items {
                    array_item.collect_agent_dependencies(agent_dependencies);
                }
            }
            Self::ObjectLiteral(object_fields) => {
                for object_field in object_fields {
                    object_field.value.collect_agent_dependencies(agent_dependencies);
                }
            }
            Self::StringTemplate(string_template) => {
                for template_part in &string_template.parts {
                    if let StringTemplatePart::Interpolation(interpolation_expression) = template_part {
                        interpolation_expression.collect_agent_dependencies(agent_dependencies);
                    }
                }
            }
            Self::StringLiteral(_) | Self::NumberLiteral(_) | Self::BooleanLiteral(_) | Self::NullLiteral => {}
        }
    }

    #[must_use]
    pub fn references_runtime(&self) -> bool {
        let mut runtime_dependencies = HashSet::new();
        self.collect_runtime_dependencies(&mut runtime_dependencies);

        !runtime_dependencies.is_empty()
    }

    pub fn collect_runtime_dependencies<HashBuilder: BuildHasher>(
        &self,
        runtime_dependencies: &mut HashSet<ReferenceKeyword, HashBuilder>,
    ) {
        match self {
            Self::Reference(reference) => {
                reference.collect_runtime_dependency(runtime_dependencies);
            }
            Self::FunctionCall(function_call) => {
                function_call.callee.collect_runtime_dependency(runtime_dependencies);

                for call_argument in &function_call.arguments {
                    call_argument.expression().collect_runtime_dependencies(runtime_dependencies);
                }
            }
            Self::AgentContext(agent_context) => {
                agent_context.collect_runtime_dependencies(runtime_dependencies);
            }
            Self::Asset(asset) => {
                asset.source.collect_runtime_dependencies(runtime_dependencies);

                for option in &asset.options {
                    option.value.collect_runtime_dependencies(runtime_dependencies);
                }
            }
            Self::ToolCall(tool_call) => {
                tool_call.callee.collect_runtime_dependency(runtime_dependencies);

                for input_field in &tool_call.input_fields {
                    input_field.value.collect_runtime_dependencies(runtime_dependencies);
                }

                for binding_field in &tool_call.binding_fields {
                    binding_field.value.collect_runtime_dependencies(runtime_dependencies);
                }
            }
            Self::McpCall(mcp_call) => {
                mcp_call.callee.collect_runtime_dependency(runtime_dependencies);

                for parameter_field in &mcp_call.parameter_fields {
                    parameter_field.value.collect_runtime_dependencies(runtime_dependencies);
                }
            }
            Self::NullFallback(null_fallback) => {
                null_fallback.value.collect_runtime_dependencies(runtime_dependencies);
                null_fallback.fallback.collect_runtime_dependencies(runtime_dependencies);
            }
            Self::VariantProjection(variant_projection) => {
                variant_projection.value.collect_runtime_dependency(runtime_dependencies);
            }
            Self::Match(match_expression) => {
                match_expression.value.collect_runtime_dependencies(runtime_dependencies);

                for branch in &match_expression.branches {
                    if let MatchBranch::Fallback { value, span: _ } = branch {
                        value.collect_runtime_dependencies(runtime_dependencies);
                    }
                }
            }
            Self::ArrayLiteral(item_expressions) => {
                for item_expression in item_expressions {
                    item_expression.collect_runtime_dependencies(runtime_dependencies);
                }
            }
            Self::ObjectLiteral(object_fields) => {
                for object_field in object_fields {
                    object_field.value.collect_runtime_dependencies(runtime_dependencies);
                }
            }
            Self::StringTemplate(string_template) => {
                for string_template_part in &string_template.parts {
                    if let StringTemplatePart::Interpolation(interpolation_expression) = string_template_part {
                        interpolation_expression.collect_runtime_dependencies(runtime_dependencies);
                    }
                }
            }
            Self::StringLiteral(_) | Self::NumberLiteral(_) | Self::BooleanLiteral(_) | Self::NullLiteral => {}
        }
    }

    #[must_use]
    pub fn to_type_expression(&self) -> Option<TypeExpression> {
        match self {
            Self::Reference(reference) => reference.to_type_expression(),
            Self::StringLiteral(string_value) => Some(TypeExpression::StringEnum(string_value.clone())),
            Self::ArrayLiteral(item_expressions) => {
                let [item_expression] = item_expressions.as_slice() else {
                    return None;
                };

                Some(TypeExpression::Array {
                    item_type: Box::new(item_expression.to_type_expression()?),
                    fixed_length: None,
                })
            }
            Self::ObjectLiteral(object_fields) => {
                let mut typed_fields = Vec::new();

                for object_field in object_fields {
                    typed_fields.push(TypedField {
                        name: object_field.name.clone(),
                        field_type: object_field.value.to_type_expression()?,
                        description: None,
                        span: object_field.span,
                    });
                }

                Some(TypeExpression::Object(typed_fields))
            }
            Self::StringTemplate(_)
            | Self::NumberLiteral(_)
            | Self::BooleanLiteral(_)
            | Self::NullLiteral
            | Self::FunctionCall(_)
            | Self::AgentContext(_)
            | Self::Asset(_)
            | Self::ToolCall(_)
            | Self::McpCall(_)
            | Self::NullFallback(_)
            | Self::VariantProjection(_)
            | Self::Match(_) => None,
        }
    }

    pub fn collect_dynamic_dependencies(&self, referenced_dynamic_fields: &mut HashSet<String>) {
        match self {
            Self::Reference(reference) => {
                reference.collect_dynamic_dependency(referenced_dynamic_fields);
            }
            Self::FunctionCall(function_call) => {
                function_call.callee.collect_dynamic_dependency(referenced_dynamic_fields);

                for call_argument in &function_call.arguments {
                    call_argument.expression().collect_dynamic_dependencies(referenced_dynamic_fields);
                }
            }
            Self::AgentContext(agent_context) => {
                agent_context.collect_dynamic_dependencies(referenced_dynamic_fields);
            }
            Self::Asset(asset) => {
                asset.source.collect_dynamic_dependencies(referenced_dynamic_fields);

                for option in &asset.options {
                    option.value.collect_dynamic_dependencies(referenced_dynamic_fields);
                }
            }
            Self::ToolCall(tool_call) => {
                tool_call.callee.collect_dynamic_dependency(referenced_dynamic_fields);

                for object_field in &tool_call.input_fields {
                    object_field.value.collect_dynamic_dependencies(referenced_dynamic_fields);
                }

                for object_field in &tool_call.binding_fields {
                    object_field.value.collect_dynamic_dependencies(referenced_dynamic_fields);
                }
            }
            Self::McpCall(mcp_call) => {
                mcp_call.callee.collect_dynamic_dependency(referenced_dynamic_fields);

                for object_field in &mcp_call.parameter_fields {
                    object_field.value.collect_dynamic_dependencies(referenced_dynamic_fields);
                }
            }
            Self::NullFallback(null_fallback) => {
                null_fallback.value.collect_dynamic_dependencies(referenced_dynamic_fields);
                null_fallback.fallback.collect_dynamic_dependencies(referenced_dynamic_fields);
            }
            Self::VariantProjection(variant_projection) => {
                variant_projection.value.collect_dynamic_dependency(referenced_dynamic_fields);
            }
            Self::Match(match_expression) => {
                match_expression.value.collect_dynamic_dependencies(referenced_dynamic_fields);

                for branch in &match_expression.branches {
                    if let MatchBranch::Fallback { value, span: _ } = branch {
                        value.collect_dynamic_dependencies(referenced_dynamic_fields);
                    }
                }
            }
            Self::ArrayLiteral(array_values) => {
                for array_value in array_values {
                    array_value.collect_dynamic_dependencies(referenced_dynamic_fields);
                }
            }
            Self::ObjectLiteral(object_fields) => {
                for object_field in object_fields {
                    object_field.value.collect_dynamic_dependencies(referenced_dynamic_fields);
                }
            }
            Self::StringTemplate(string_template) => {
                for string_template_part in &string_template.parts {
                    if let StringTemplatePart::Interpolation(interpolation_expression) = string_template_part {
                        interpolation_expression.collect_dynamic_dependencies(referenced_dynamic_fields);
                    }
                }
            }
            Self::StringLiteral(_) | Self::NumberLiteral(_) | Self::BooleanLiteral(_) | Self::NullLiteral => {}
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub callee: Reference,
    pub input_fields: Vec<ObjectField>,
    pub binding_fields: Vec<ObjectField>,
    pub max_calls: Option<u64>,
    pub span: SourceSpan,
}

impl ToolCall {
    #[must_use]
    pub fn agent_tool_binding_fields(&self) -> &[ObjectField] {
        self.binding_fields.as_slice()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpCall {
    pub operation: McpCallOperation,
    pub callee: Reference,
    pub parameter_fields: Vec<ObjectField>,
    pub span: SourceSpan,
}

impl McpCall {
    #[must_use]
    pub fn target_name(&self) -> Option<&str> {
        self.callee.first_access_field()
    }

    #[must_use]
    pub fn has_valid_callee(&self) -> bool {
        self.callee.is_direct_required_reference_to_keyword(self.operation.expected_root())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum McpCallOperation {
    Read,
    Render,
}

impl McpCallOperation {
    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "read" => Some(Self::Read),
            "render" => Some(Self::Render),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Render => "render",
        }
    }

    #[must_use]
    pub fn expected_root(self) -> ReferenceKeyword {
        match self {
            Self::Read => ReferenceKeyword::Resource,
            Self::Render => ReferenceKeyword::Prompt,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringTemplate {
    pub parts: Vec<StringTemplatePart>,
}

impl StringTemplate {
    #[must_use]
    pub fn normalized_multiline_indentation(self) -> Self {
        let mut template_lines = Vec::from([StringTemplateLine::default()]);

        for template_part in self.parts {
            template_part.push_split_lines(&mut template_lines);
        }

        let Some(first_content_line_index) = template_lines.iter().position(|template_line| !template_line.is_blank()) else {
            return Self { parts: Vec::new() };
        };

        let last_content_line_index = template_lines
            .iter()
            .rposition(|template_line| !template_line.is_blank())
            .expect("first content line should guarantee last content line");

        let common_indentation = template_lines[first_content_line_index..=last_content_line_index]
            .iter()
            .filter_map(StringTemplateLine::indentation_width)
            .min()
            .unwrap_or(0);

        let mut normalized_parts = Vec::new();

        for (line_index, template_line) in template_lines
            .into_iter()
            .enumerate()
            .filter(|(line_index, _)| *line_index >= first_content_line_index && *line_index <= last_content_line_index)
        {
            if line_index > first_content_line_index {
                StringTemplatePart::push_text(&mut normalized_parts, "\n");
            }

            let normalized_line = template_line.normalized_indentation(common_indentation);
            normalized_line.push_parts(&mut normalized_parts);
        }

        Self { parts: normalized_parts }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StringTemplatePart {
    Text(String),
    Interpolation(Expression),
}

impl StringTemplatePart {
    fn push_split_lines(self, template_lines: &mut Vec<StringTemplateLine>) {
        match self {
            Self::Text(text) => {
                let mut current_text = String::new();

                let mut characters = text.chars().peekable();

                while let Some(character) = characters.next() {
                    if character == '\r' && characters.peek() == Some(&'\n') {
                        continue;
                    }

                    if character == '\n' {
                        Self::push_text_to_last_line(template_lines, &current_text);
                        current_text.clear();
                        template_lines.push(StringTemplateLine::default());

                        continue;
                    }

                    current_text.push(character);
                }

                Self::push_text_to_last_line(template_lines, &current_text);
            }
            Self::Interpolation(expression) => {
                template_lines
                    .last_mut()
                    .expect("template line list should never be empty")
                    .parts
                    .push(Self::Interpolation(expression));
            }
        }
    }

    fn push_text_to_last_line(template_lines: &mut [StringTemplateLine], text: &str) {
        if text.is_empty() {
            return;
        }

        template_lines
            .last_mut()
            .expect("template line list should never be empty")
            .parts
            .push(Self::Text(text.to_string()));
    }

    fn push_text(template_parts: &mut Vec<Self>, text: &str) {
        if text.is_empty() {
            return;
        }

        match template_parts.last_mut() {
            Some(Self::Text(existing_text)) => existing_text.push_str(text),
            Some(Self::Interpolation(_)) | None => template_parts.push(Self::Text(text.to_string())),
        }
    }

    fn is_blank(&self) -> bool {
        match self {
            Self::Text(text) => text.chars().all(char::is_whitespace),
            Self::Interpolation(_) => false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct StringTemplateLine {
    parts: Vec<StringTemplatePart>,
}

impl StringTemplateLine {
    fn is_blank(&self) -> bool {
        self.parts.iter().all(StringTemplatePart::is_blank)
    }

    fn indentation_width(&self) -> Option<usize> {
        if self.is_blank() {
            return None;
        }

        let mut indentation_width = 0;

        for template_part in &self.parts {
            match template_part {
                StringTemplatePart::Text(text) => {
                    for character in text.chars() {
                        if character != ' ' && character != '\t' {
                            return Some(indentation_width);
                        }

                        indentation_width += 1;
                    }
                }
                StringTemplatePart::Interpolation(_) => return Some(indentation_width),
            }
        }

        Some(indentation_width)
    }

    fn normalized_indentation(mut self, common_indentation: usize) -> Self {
        if self.is_blank() {
            self.parts.clear();

            return self;
        }

        let mut remaining_indentation = common_indentation;

        for template_part in &mut self.parts {
            let StringTemplatePart::Text(text) = template_part else {
                break;
            };

            let mut split_byte_index = 0;
            let mut removed_indentation = 0;

            for (character_byte_index, character) in text.char_indices() {
                if removed_indentation == remaining_indentation || (character != ' ' && character != '\t') {
                    break;
                }

                split_byte_index = character_byte_index + character.len_utf8();
                removed_indentation += 1;
            }

            if split_byte_index > 0 {
                text.drain(..split_byte_index);
            }

            remaining_indentation -= removed_indentation;

            if remaining_indentation == 0 {
                break;
            }
        }

        self.parts
            .retain(|template_part| !matches!(template_part, StringTemplatePart::Text(text) if text.is_empty()));

        self
    }

    fn push_parts(self, template_parts: &mut Vec<StringTemplatePart>) {
        for template_part in self.parts {
            match template_part {
                StringTemplatePart::Text(text) => StringTemplatePart::push_text(template_parts, &text),
                StringTemplatePart::Interpolation(expression) => template_parts.push(StringTemplatePart::Interpolation(expression)),
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectField {
    pub name: String,
    pub value: Expression,
    pub span: SourceSpan,
}

impl ObjectField {
    #[must_use]
    pub fn merged_with_overrides(shared_fields: &[Self], local_fields: &[Self]) -> Vec<Self> {
        let mut merged_fields = shared_fields.to_vec();

        for local_field in local_fields {
            if let Some(existing_field_index) = merged_fields
                .iter()
                .position(|existing_field| existing_field.name == local_field.name)
            {
                merged_fields[existing_field_index] = local_field.clone();

                continue;
            }

            merged_fields.push(local_field.clone());
        }

        merged_fields
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionCall {
    pub callee: Reference,
    pub arguments: Vec<CallArgument>,
}

impl FunctionCall {
    #[must_use]
    pub fn identifier_name(&self) -> Option<&str> {
        self.callee.root.as_identifier()
    }

    #[must_use]
    pub fn builtin_function_name(&self) -> Option<BuiltinFunctionName> {
        self.identifier_name().and_then(BuiltinFunctionName::from_identifier)
    }

    #[must_use]
    pub fn argument_expression(&self, index: usize) -> Option<&Expression> {
        self.arguments.get(index).map(CallArgument::expression)
    }

    #[must_use]
    pub fn first_argument_expression(&self) -> Option<&Expression> {
        self.argument_expression(0)
    }

    #[must_use]
    pub fn named_argument_expression(&self, argument_name: &str) -> Option<&Expression> {
        for call_argument in &self.arguments {
            if call_argument.named_argument_name() == Some(argument_name) {
                return Some(call_argument.expression());
            }
        }

        None
    }

    #[must_use]
    pub fn builtin_named_argument_expression(&self, argument_name: BuiltinFunctionArgumentName) -> Option<&Expression> {
        self.named_argument_expression(argument_name.as_str())
    }

    #[must_use]
    pub fn model_named_argument_expression(&self, argument_name: ModelCallArgumentName) -> Option<&Expression> {
        self.named_argument_expression(argument_name.as_str())
    }

    #[must_use]
    pub fn model_argument_expressions(&self) -> Vec<&Expression> {
        let mut model_argument_expressions = Vec::new();

        for call_argument in &self.arguments {
            if call_argument.named_argument_name().is_none() {
                model_argument_expressions.push(call_argument.expression());

                continue;
            }

            if call_argument.named_argument_name() == Some(ModelCallArgumentName::Model.as_str()) {
                model_argument_expressions.push(call_argument.expression());
            }
        }

        model_argument_expressions
    }

    #[must_use]
    pub fn agent_argument_expression(&self) -> Option<&Expression> {
        for call_argument in &self.arguments {
            if call_argument.named_argument_name().is_none() {
                return Some(call_argument.expression());
            }

            if call_argument.named_argument_name() == Some(BuiltinFunctionArgumentName::Agent.as_str()) {
                return Some(call_argument.expression());
            }
        }

        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallArgument {
    Positional(Expression),
    Named(NamedArgument),
}

impl CallArgument {
    #[must_use]
    pub fn expression(&self) -> &Expression {
        match self {
            Self::Positional(expression) => expression,
            Self::Named(named_argument) => &named_argument.value,
        }
    }

    #[must_use]
    pub fn named_argument_name(&self) -> Option<&str> {
        match self {
            Self::Positional(_) => None,
            Self::Named(named_argument) => Some(named_argument.name.as_str()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedArgument {
    pub name: String,
    pub value: Expression,
}

#[cfg(test)]
mod tests {
    use super::super::{Reference, ReferenceAccess, ReferenceKeyword, ReferenceRoot, SourcePosition, SourceSpan};
    use super::{Expression, ObjectField, StringTemplate, StringTemplatePart, ToolCall};
    use std::collections::HashSet;

    #[test]
    fn expression_collects_nested_tool_references_and_direct_tool_metadata() {
        let direct_tool_call = Expression::ToolCall(ToolCall {
            callee: reference_with_accesses(ReferenceKeyword::Tool, [("fetch_task", false)]),
            input_fields: vec![ObjectField {
                name: "payload".to_string(),
                value: Expression::StringTemplate(StringTemplate {
                    parts: vec![StringTemplatePart::Interpolation(Expression::ToolCall(ToolCall {
                        callee: reference_with_accesses(ReferenceKeyword::Tool, [("audit_task", false)]),
                        input_fields: Vec::new(),
                        binding_fields: Vec::new(),
                        max_calls: None,
                        span: test_source_span(),
                    }))],
                }),
                span: test_source_span(),
            }],
            binding_fields: Vec::new(),
            max_calls: Some(2),
            span: test_source_span(),
        });
        let expression = Expression::ArrayLiteral(vec![direct_tool_call]);
        let tool_names = expression.tool_names();

        assert_eq!(tool_names, vec!["fetch_task", "audit_task"]);

        let Expression::ArrayLiteral(tool_expressions) = &expression else {
            panic!("expression should be an array literal");
        };

        assert_eq!(tool_expressions[0].direct_tool_name(), Some("fetch_task"));
        assert_eq!(tool_expressions[0].max_calls_override(), Some(2));
    }

    #[test]
    fn expression_detects_nested_secret_references_and_agent_dependencies() {
        let expression = Expression::ObjectLiteral(vec![ObjectField {
            name: "summary".to_string(),
            value: Expression::StringTemplate(StringTemplate {
                parts: vec![
                    StringTemplatePart::Interpolation(Expression::Reference(reference_with_accesses(
                        ReferenceKeyword::Agent,
                        [("writer", false), ("text", false)],
                    ))),
                    StringTemplatePart::Interpolation(Expression::Reference(reference_with_accesses(
                        ReferenceKeyword::Secrets,
                        [("api_key", false)],
                    ))),
                ],
            }),
            span: test_source_span(),
        }]);
        let mut agent_dependencies = HashSet::new();

        expression.collect_agent_dependencies(&mut agent_dependencies);

        assert!(expression.references_secret());
        assert!(agent_dependencies.contains("writer"));
    }

    fn reference_with_accesses<const ACCESS_COUNT: usize>(
        reference_keyword: ReferenceKeyword,
        accesses: [(&str, bool); ACCESS_COUNT],
    ) -> Reference {
        Reference {
            root: ReferenceRoot::Keyword(reference_keyword),
            accesses: accesses
                .into_iter()
                .map(|(field_name, optional)| {
                    if optional {
                        return ReferenceAccess::optional(field_name);
                    }

                    ReferenceAccess::required(field_name)
                })
                .collect(),
            span: test_source_span(),
        }
    }

    fn test_source_span() -> SourceSpan {
        SourceSpan {
            start: SourcePosition { line: 1, column: 1 },
            end: SourcePosition { line: 1, column: 1 },
        }
    }
}
