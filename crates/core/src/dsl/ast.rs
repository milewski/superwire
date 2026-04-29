use std::ops::Range;
use strsim::levenshtein;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourcePosition {
    pub line: usize,
    pub column: usize,
}

impl SourcePosition {
    #[must_use]
    pub fn to_byte_offset(self, source_text: &str) -> Option<usize> {
        if self.line == 0 || self.column == 0 {
            return None;
        }

        let mut current_line_number = 1_usize;
        let mut current_column_number = 1_usize;

        for (byte_offset, character) in source_text.char_indices() {
            if current_line_number == self.line && current_column_number == self.column {
                return Some(byte_offset);
            }

            if character == '\n' {
                current_line_number += 1;
                current_column_number = 1;

                continue;
            }

            current_column_number += 1;
        }

        if current_line_number == self.line && current_column_number == self.column {
            return Some(source_text.len());
        }

        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceSpan {
    pub start: SourcePosition,
    pub end: SourcePosition,
}

impl SourceSpan {
    #[must_use]
    pub fn to_byte_range(self, source_text: &str) -> Option<Range<usize>> {
        let start_byte_offset = self.start.to_byte_offset(source_text)?;
        let mut end_byte_offset = self.end.to_byte_offset(source_text)?;

        if end_byte_offset < start_byte_offset {
            return None;
        }

        if end_byte_offset == start_byte_offset {
            if let Some(character_at_start) = source_text[start_byte_offset..].chars().next() {
                end_byte_offset = start_byte_offset + character_at_start.len_utf8();
            }
        }

        Some(start_byte_offset..end_byte_offset)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workflow {
    pub declarations: Vec<Declaration>,
    pub source_text: Option<String>,
}

impl Workflow {
    #[must_use]
    pub fn declarations(&self) -> &[Declaration] {
        &self.declarations
    }

    #[must_use]
    pub fn source_text(&self) -> Option<&str> {
        self.source_text.as_deref()
    }

    #[must_use]
    pub fn with_source_text(mut self, source_text: impl Into<String>) -> Self {
        self.source_text = Some(source_text.into());

        self
    }

    #[must_use]
    pub fn find_provider(&self, provider_name: &str) -> Option<&ProviderDeclaration> {
        self.declarations.iter().find_map(|declaration| match declaration {
            Declaration::Provider(provider_declaration) if provider_declaration.name == provider_name => Some(provider_declaration),
            _ => None,
        })
    }

    #[must_use]
    pub fn find_secrets(&self) -> Option<&SecretsDeclaration> {
        self.declarations.iter().find_map(|declaration| match declaration {
            Declaration::Secrets(secrets_declaration) => Some(secrets_declaration),
            _ => None,
        })
    }

    #[must_use]
    pub fn find_input(&self) -> Option<&InputDeclaration> {
        self.declarations.iter().find_map(|declaration| match declaration {
            Declaration::Input(input_declaration) => Some(input_declaration),
            _ => None,
        })
    }

    #[must_use]
    pub fn find_schema(&self, schema_name: &str) -> Option<&SchemaDeclaration> {
        self.declarations.iter().find_map(|declaration| match declaration {
            Declaration::Schema(schema_declaration) if schema_declaration.name == schema_name => Some(schema_declaration),
            _ => None,
        })
    }

    #[must_use]
    pub fn find_tool(&self, tool_name: &str) -> Option<&ToolDeclaration> {
        self.declarations.iter().find_map(|declaration| match declaration {
            Declaration::Tool(tool_declaration) if tool_declaration.name == tool_name => Some(tool_declaration),
            _ => None,
        })
    }

    #[must_use]
    pub fn find_agent(&self, agent_name: &str) -> Option<&AgentDeclaration> {
        self.declarations.iter().find_map(|declaration| match declaration {
            Declaration::Agent(agent_declaration) if agent_declaration.name == agent_name => Some(agent_declaration),
            _ => None,
        })
    }

    #[must_use]
    pub fn find_output(&self) -> Option<&OutputDeclaration> {
        self.declarations.iter().find_map(|declaration| match declaration {
            Declaration::Output(output_declaration) => Some(output_declaration),
            _ => None,
        })
    }

    pub fn dynamic_blocks(&self) -> impl Iterator<Item = &DynamicBlock> {
        self.declarations.iter().filter_map(|declaration| match declaration {
            Declaration::Dynamic(dynamic_block) => Some(dynamic_block),
            Declaration::Provider(_)
            | Declaration::Secrets(_)
            | Declaration::Input(_)
            | Declaration::Schema(_)
            | Declaration::Tool(_)
            | Declaration::Agent(_)
            | Declaration::Output(_) => None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Declaration {
    Provider(ProviderDeclaration),
    Secrets(SecretsDeclaration),
    Input(InputDeclaration),
    Schema(SchemaDeclaration),
    Tool(ToolDeclaration),
    Dynamic(DynamicBlock),
    Agent(AgentDeclaration),
    Output(OutputDeclaration),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeclarationKeyword {
    Provider,
    Secrets,
    Input,
    Schema,
    Tool,
    Dynamic,
    Agent,
    Output,
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

impl DeclarationKeyword {
    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "provider" => Some(Self::Provider),
            "secrets" => Some(Self::Secrets),
            "input" => Some(Self::Input),
            "schema" => Some(Self::Schema),
            "tool" => Some(Self::Tool),
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
            Self::Secrets => "secrets",
            Self::Input => "input",
            Self::Schema => "schema",
            Self::Tool => "tool",
            Self::Dynamic => "dynamic",
            Self::Agent => "agent",
            Self::Output => "output",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderDeclaration {
    pub name: String,
    pub properties: Vec<ObjectField>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretsDeclaration {
    pub fields: Vec<TypedField>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDeclaration {
    pub fields: Vec<TypedField>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaDeclaration {
    pub name: String,
    pub fields: Vec<TypedField>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDeclaration {
    pub name: String,
    pub description: Option<String>,
    pub input_fields: Vec<TypedField>,
    pub binding_fields: Vec<TypedField>,
    pub output_fields: Vec<TypedField>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicBlock {
    pub fields: Vec<ObjectField>,
    pub span: SourceSpan,
}

impl DynamicBlock {
    #[must_use]
    pub fn field(&self, field_name: &str) -> Option<&ObjectField> {
        self.fields.iter().find(|field| field.name == field_name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDeclaration {
    pub name: String,
    pub for_loop: Option<AgentForLoop>,
    pub properties: Vec<AgentProperty>,
    pub span: SourceSpan,
}

impl AgentDeclaration {
    pub fn dynamic_blocks(&self) -> impl Iterator<Item = &DynamicBlock> {
        self.properties.iter().filter_map(|property| match property {
            AgentProperty::Dynamic(dynamic_block) => Some(dynamic_block),
            AgentProperty::Model(_)
            | AgentProperty::Prompt(_)
            | AgentProperty::Output {
                output_type_expression: _,
                description: _,
            }
            | AgentProperty::Context(_)
            | AgentProperty::Inference(_)
            | AgentProperty::Tools(_) => None,
        })
    }

    #[must_use]
    pub fn expression_property(&self, property_name: AgentExpressionPropertyName) -> Option<&Expression> {
        for agent_property in &self.properties {
            match agent_property {
                AgentProperty::Model(expression) if property_name == AgentExpressionPropertyName::Model => return Some(expression),
                AgentProperty::Prompt(expression) if property_name == AgentExpressionPropertyName::Prompt => return Some(expression),
                AgentProperty::Context(expression) if property_name == AgentExpressionPropertyName::Context => return Some(expression),
                AgentProperty::Inference(expression) if property_name == AgentExpressionPropertyName::Inference => return Some(expression),
                AgentProperty::Tools(expression) if property_name == AgentExpressionPropertyName::Tools => return Some(expression),
                AgentProperty::Dynamic(_) => {}
                AgentProperty::Model(_)
                | AgentProperty::Prompt(_)
                | AgentProperty::Output {
                    output_type_expression: _,
                    description: _,
                }
                | AgentProperty::Context(_)
                | AgentProperty::Inference(_)
                | AgentProperty::Tools(_) => {}
            }
        }

        None
    }

    pub fn required_expression_property(
        &self,
        property_name: AgentExpressionPropertyName,
    ) -> Result<&Expression, AgentExpressionPropertyName> {
        self.expression_property(property_name).ok_or(property_name)
    }

    #[must_use]
    pub fn output_type(&self) -> Option<&TypeExpression> {
        for agent_property in &self.properties {
            if let AgentProperty::Output {
                output_type_expression,
                description: _,
            } = agent_property
            {
                return Some(output_type_expression);
            }
        }

        None
    }

    #[must_use]
    pub fn inferred_iteration_output_type_expression(&self) -> TypeExpression {
        self.output_type().cloned().unwrap_or(TypeExpression::String)
    }

    #[must_use]
    pub fn inferred_final_output_type_expression(&self) -> TypeExpression {
        let iteration_output_type_expression = self.inferred_iteration_output_type_expression();

        if self.for_loop.is_some() {
            return TypeExpression::Array {
                item_type: Box::new(iteration_output_type_expression),
                fixed_length: None,
            };
        }

        iteration_output_type_expression
    }

    #[must_use]
    pub fn output_description(&self) -> Option<&str> {
        for agent_property in &self.properties {
            if let AgentProperty::Output {
                output_type_expression: _,
                description: Some(output_description),
            } = agent_property
            {
                return Some(output_description.as_str());
            }
        }

        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentForLoop {
    pub pattern: AgentForLoopPattern,
    pub iterable: Expression,
}

impl AgentForLoop {
    #[must_use]
    pub fn bound_identifier_names(&self) -> Vec<&str> {
        self.pattern.bound_identifier_names()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentForLoopPattern {
    Identifier(String),
    ObjectDestructuring(Vec<String>),
}

impl AgentForLoopPattern {
    #[must_use]
    pub fn bound_identifier_names(&self) -> Vec<&str> {
        match self {
            Self::Identifier(identifier) => vec![identifier.as_str()],
            Self::ObjectDestructuring(field_names) => field_names.iter().map(String::as_str).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentProperty {
    Dynamic(DynamicBlock),
    Model(Expression),
    Prompt(Expression),
    Output {
        output_type_expression: TypeExpression,
        description: Option<String>,
    },
    Context(Expression),
    Inference(Expression),
    Tools(Expression),
}

impl AgentProperty {
    #[must_use]
    pub fn name(&self) -> AgentPropertyName {
        match self {
            Self::Dynamic(_) => AgentPropertyName::Dynamic,
            Self::Model(_) => AgentPropertyName::Model,
            Self::Prompt(_) => AgentPropertyName::Prompt,
            Self::Output {
                output_type_expression: _,
                description: _,
            } => AgentPropertyName::Output,
            Self::Context(_) => AgentPropertyName::Context,
            Self::Inference(_) => AgentPropertyName::Inference,
            Self::Tools(_) => AgentPropertyName::Tools,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentPropertyName {
    Dynamic,
    Model,
    Prompt,
    Output,
    Context,
    Inference,
    Tools,
}

impl AgentPropertyName {
    #[must_use]
    pub fn all() -> [Self; 7] {
        [
            Self::Dynamic,
            Self::Model,
            Self::Prompt,
            Self::Output,
            Self::Context,
            Self::Inference,
            Self::Tools,
        ]
    }

    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "model" => Some(Self::Model),
            "dynamic" => Some(Self::Dynamic),
            "prompt" => Some(Self::Prompt),
            "output" => Some(Self::Output),
            "context" => Some(Self::Context),
            "inference" => Some(Self::Inference),
            "tools" => Some(Self::Tools),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::Dynamic => "dynamic",
            Self::Prompt => "prompt",
            Self::Output => "output",
            Self::Context => "context",
            Self::Inference => "inference",
            Self::Tools => "tools",
        }
    }

    #[must_use]
    pub fn suggested_from_identifier(identifier: &str) -> Option<Self> {
        if identifier.is_empty() {
            return None;
        }

        let mut closest_property_name = None;
        let mut closest_distance = usize::MAX;

        for property_name in Self::all() {
            let candidate_distance = levenshtein(identifier, property_name.as_str());

            if candidate_distance < closest_distance {
                closest_property_name = Some(property_name);
                closest_distance = candidate_distance;
            }
        }

        if closest_distance > Self::max_typo_distance(identifier) {
            return None;
        }

        closest_property_name
    }

    #[must_use]
    pub fn rendered_values() -> String {
        let mut rendered_property_names = Self::all()
            .into_iter()
            .map(|property_name| format!("`{}`", property_name.as_str()))
            .collect::<Vec<_>>();

        let last_property_name = rendered_property_names
            .pop()
            .expect("agent property names should include a last value");

        format!("{} or {last_property_name}", rendered_property_names.join(", "))
    }

    fn max_typo_distance(identifier: &str) -> usize {
        let identifier_length = identifier.chars().count();

        if identifier_length <= 4 {
            return 1;
        }

        if identifier_length <= 8 {
            return 2;
        }

        3
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentExpressionPropertyName {
    Model,
    Prompt,
    Context,
    Inference,
    Tools,
}

impl AgentExpressionPropertyName {
    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "model" => Some(Self::Model),
            "prompt" => Some(Self::Prompt),
            "context" => Some(Self::Context),
            "inference" => Some(Self::Inference),
            "tools" => Some(Self::Tools),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::Prompt => "prompt",
            Self::Context => "context",
            Self::Inference => "inference",
            Self::Tools => "tools",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputDeclaration {
    pub fields: Vec<ObjectField>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedField {
    pub name: String,
    pub field_type: TypeExpression,
    pub description: Option<String>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeExpression {
    String,
    Number,
    Float,
    Boolean,
    Null,
    SchemaReference(String),
    StringEnum(String),
    StringEnumReference(Reference),
    Array {
        item_type: Box<TypeExpression>,
        fixed_length: Option<u64>,
    },
    Tuple(Vec<TypeExpression>),
    Object(Vec<TypedField>),
    Union(Vec<TypeExpression>),
}

impl TypeExpression {
    #[must_use]
    pub fn can_be_null(&self) -> bool {
        match self {
            Self::Null => true,
            Self::Union(type_expressions) => type_expressions.iter().any(Self::can_be_null),
            Self::String
            | Self::Number
            | Self::Float
            | Self::Boolean
            | Self::SchemaReference(_)
            | Self::StringEnum(_)
            | Self::StringEnumReference(_)
            | Self::Array {
                item_type: _,
                fixed_length: _,
            }
            | Self::Tuple(_)
            | Self::Object(_) => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expression {
    StringLiteral(String),
    StringTemplate(StringTemplate),
    NumberLiteral(String),
    BooleanLiteral(bool),
    NullLiteral,
    Reference(Reference),
    FunctionCall(FunctionCall),
    ToolCall(ToolCall),
    ArrayLiteral(Vec<Expression>),
    ObjectLiteral(Vec<ObjectField>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub callee: Reference,
    pub input_fields: Vec<ObjectField>,
    pub binding_fields: Vec<ObjectField>,
    pub span: SourceSpan,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub root: ReferenceRoot,
    pub accesses: Vec<ReferenceAccess>,
    pub span: SourceSpan,
}

impl Reference {
    #[must_use]
    pub fn root_keyword(&self) -> Option<ReferenceKeyword> {
        self.root.keyword()
    }

    #[must_use]
    pub fn is_keyword_root(&self, reference_keyword: ReferenceKeyword) -> bool {
        self.root_keyword() == Some(reference_keyword)
    }

    #[must_use]
    pub fn is_agent_root(&self) -> bool {
        self.is_keyword_root(ReferenceKeyword::Agent)
    }

    #[must_use]
    pub fn first_access(&self) -> Option<&ReferenceAccess> {
        self.accesses.first()
    }

    #[must_use]
    pub fn first_access_field(&self) -> Option<&str> {
        self.first_access().map(|reference_access| reference_access.field.as_str())
    }

    #[must_use]
    pub fn render_path(&self) -> String {
        let mut rendered_reference = if let Some(reference_root_keyword) = self.root_keyword() {
            reference_root_keyword.as_str().to_owned()
        } else {
            self.root
                .as_identifier()
                .expect("non-keyword reference root should be identifier")
                .to_owned()
        };

        for reference_access in &self.accesses {
            if reference_access.optional {
                rendered_reference.push_str("?.");
                rendered_reference.push_str(reference_access.field.as_str());

                continue;
            }

            rendered_reference.push('.');
            rendered_reference.push_str(reference_access.field.as_str());
        }

        rendered_reference
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ReferenceRoot {
    Keyword(ReferenceKeyword),
    Identifier(String),
}

impl ReferenceRoot {
    #[must_use]
    pub fn from_identifier(identifier: String) -> Self {
        if let Some(keyword) = ReferenceKeyword::from_identifier(identifier.as_str()) {
            Self::Keyword(keyword)
        } else {
            Self::Identifier(identifier)
        }
    }

    #[must_use]
    pub fn as_identifier(&self) -> Option<&str> {
        match self {
            Self::Identifier(identifier) => Some(identifier),
            Self::Keyword(_) => None,
        }
    }

    #[must_use]
    pub fn keyword(&self) -> Option<ReferenceKeyword> {
        match self {
            Self::Keyword(keyword) => Some(*keyword),
            Self::Identifier(_) => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReferenceKeyword {
    Agent,
    Dynamic,
    Input,
    Secrets,
    Tool,
}

impl ReferenceKeyword {
    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "agent" => Some(Self::Agent),
            "dynamic" => Some(Self::Dynamic),
            "input" => Some(Self::Input),
            "secrets" => Some(Self::Secrets),
            "tool" => Some(Self::Tool),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Dynamic => "dynamic",
            Self::Input => "input",
            Self::Secrets => "secrets",
            Self::Tool => "tool",
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceAccess {
    pub field: String,
    pub optional: bool,
}

#[cfg(test)]
mod tests {
    use super::{AgentPropertyName, ForClauseKeyword, SourcePosition, SourceSpan};

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

    #[test]
    fn maps_source_position_to_byte_offset() {
        let source_text = "alpha\nbeta\n";

        assert_eq!(SourcePosition { line: 1, column: 1 }.to_byte_offset(source_text), Some(0));
        assert_eq!(SourcePosition { line: 2, column: 1 }.to_byte_offset(source_text), Some(6));
        assert_eq!(SourcePosition { line: 2, column: 5 }.to_byte_offset(source_text), Some(10));
        assert_eq!(SourcePosition { line: 3, column: 1 }.to_byte_offset(source_text), Some(11));
    }

    #[test]
    fn maps_source_span_to_byte_range() {
        let source_text = "agent greeting";
        let source_span = SourceSpan {
            start: SourcePosition { line: 1, column: 7 },
            end: SourcePosition { line: 1, column: 15 },
        };

        assert_eq!(source_span.to_byte_range(source_text), Some(6..14));
    }

    #[test]
    fn suggests_closest_agent_property_name_for_typos() {
        assert_eq!(
            AgentPropertyName::suggested_from_identifier("prom_t"),
            Some(AgentPropertyName::Prompt)
        );

        assert_eq!(
            AgentPropertyName::suggested_from_identifier("modle"),
            Some(AgentPropertyName::Model)
        );
    }

    #[test]
    fn does_not_suggest_agent_property_name_for_distant_identifier() {
        assert_eq!(AgentPropertyName::suggested_from_identifier("retries"), None);
    }
}
