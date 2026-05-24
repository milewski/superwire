use super::{
    AssetPropertyName, BuiltinFunctionArgumentName, BuiltinFunctionName, ModelCallArgumentName, Reference, ReferenceKeyword, SourceSpan,
    TypeExpression, TypedField,
};
use std::collections::HashSet;
use std::hash::BuildHasher;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expression {
    StringLiteral(String),
    StringTemplate(StringTemplate),
    NumberLiteral(String),
    BooleanLiteral(bool),
    NullLiteral,
    Reference(Reference),
    FunctionCall(FunctionCall),
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchExpression {
    pub value: Box<Expression>,
    pub branches: Vec<MatchBranch>,
    pub span: SourceSpan,
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
    pub fn direct_reference(&self) -> Option<&Reference> {
        match self {
            Self::Reference(reference) => Some(reference),
            Self::ToolCall(tool_call) => Some(&tool_call.callee),
            Self::FunctionCall(_)
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
            Self::Asset(_)
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StringTemplatePart {
    Text(String),
    Interpolation(Expression),
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
