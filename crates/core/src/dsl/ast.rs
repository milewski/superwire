#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourcePosition {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceSpan {
    pub start: SourcePosition,
    pub end: SourcePosition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workflow {
    pub declarations: Vec<Declaration>,
}

impl Workflow {
    #[must_use]
    pub fn declarations(&self) -> &[Declaration] {
        &self.declarations
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Declaration {
    Provider(ProviderDeclaration),
    Secrets(SecretsDeclaration),
    Input(InputDeclaration),
    Schema(SchemaDeclaration),
    Agent(AgentDeclaration),
    Output(OutputDeclaration),
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
pub struct AgentDeclaration {
    pub name: String,
    pub for_loop: Option<AgentForLoop>,
    pub properties: Vec<AgentProperty>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentForLoop {
    pub iterator_name: String,
    pub iterable: Expression,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentProperty {
    Model(Expression),
    Prompt(Expression),
    Output(TypeExpression),
    Context(Expression),
    Inference(Expression),
    Tools(Expression),
    Custom { name: String, value: Expression },
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
    Array {
        item_type: Box<TypeExpression>,
        fixed_length: Option<u64>,
    },
    Tuple(Vec<TypeExpression>),
    Object(Vec<TypedField>),
    Union(Vec<TypeExpression>),
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
    ArrayLiteral(Vec<Expression>),
    ObjectLiteral(Vec<ObjectField>),
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
    Input,
    Secrets,
    Tool,
}

impl ReferenceKeyword {
    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "agent" => Some(Self::Agent),
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
