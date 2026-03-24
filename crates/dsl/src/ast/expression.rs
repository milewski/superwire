#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    Boolean(bool),
    Function(FunctionExpression),
    Null,
    Number(String),
    Object(Vec<ObjectField>),
    Reference(ReferenceExpression),
    String(StringTemplate),
    Array(Vec<Expression>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum FunctionExpression {
    Compact(CompactExpression),
    Context(ReferenceExpression),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompactExpression {
    pub arguments: Vec<CompactArgument>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CompactArgument {
    Agent(ReferenceExpression),
    Inference(Vec<crate::ast::workflow::InferenceProperty>),
    Model(ModelSelector),
    Prompt(StringTemplate),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectField {
    pub name: String,
    pub value: Expression,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PromptValue {
    Inline(StringTemplate),
    Template { path: String, bindings: Vec<Binding> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Binding {
    pub name: String,
    pub value: Expression,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelSelector {
    pub provider_name: String,
    pub model_name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolUsage {
    pub name: String,
    pub arguments: Vec<Binding>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StringTemplate {
    pub raw: String,
    pub fragments: Vec<StringFragment>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StringFragment {
    Expression(Expression),
    Text(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceExpression {
    pub root: ReferenceRoot,
    pub path: Vec<PathSegment>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReferenceRoot {
    Agent(String),
    Input(String),
    Local(String),
    Secrets(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct PathSegment {
    pub operator: AccessOperator,
    pub property_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessOperator {
    Direct,
    Safe,
}
