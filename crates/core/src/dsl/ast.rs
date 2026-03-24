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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretsDeclaration {
    pub fields: Vec<TypedField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDeclaration {
    pub fields: Vec<TypedField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaDeclaration {
    pub name: String,
    pub fields: Vec<TypedField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDeclaration {
    pub name: String,
    pub for_loop: Option<AgentForLoop>,
    pub properties: Vec<AgentProperty>,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedField {
    pub name: String,
    pub field_type: TypeExpression,
    pub description: Option<String>,
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
    NumberLiteral(String),
    BooleanLiteral(bool),
    NullLiteral,
    Reference(Reference),
    FunctionCall(FunctionCall),
    ArrayLiteral(Vec<Expression>),
    ObjectLiteral(Vec<ObjectField>),
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallArgument {
    Positional(Expression),
    Named(NamedArgument),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedArgument {
    pub name: String,
    pub value: Expression,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub root: String,
    pub accesses: Vec<ReferenceAccess>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceAccess {
    pub field: String,
    pub optional: bool,
}
