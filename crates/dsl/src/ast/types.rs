#[derive(Debug, Clone, PartialEq)]
pub enum TypeExpression {
    Array(Box<TypeExpression>),
    FixedArray { item_type: Box<TypeExpression>, length: usize },
    NamedSchema(String),
    Null,
    Object(Vec<TypeField>),
    Primitive(PrimitiveType),
    StringLiteral(String),
    Tuple(Vec<TypeExpression>),
    Union(Vec<TypeExpression>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveType {
    Boolean,
    Float,
    Number,
    String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeField {
    pub name: String,
    pub value_type: TypeExpression,
    pub description: Option<String>,
}
