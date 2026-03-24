#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workflow {
    pub declarations: Vec<Declaration>,
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
