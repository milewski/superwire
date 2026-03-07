use thiserror::Error;

#[derive(Debug, Error)]
pub enum UtilsError {
    #[error("utility implementation is not available yet")]
    Unimplemented,
    #[error("template parse error: {message}")]
    TemplateParse { message: String },
    #[error("missing template variables: {variables:?}")]
    MissingTemplateVariables { variables: Vec<String> },
    #[error("unused template bindings: {bindings:?}")]
    UnusedTemplateBindings { bindings: Vec<String> },
    #[error("file read error: {message}")]
    FileRead { message: String },
}
