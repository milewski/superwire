use crate::protocol::Range;

#[derive(Debug, Clone)]
pub struct DocumentDiagnostic {
    pub range: Range,
    pub severity: DiagnosticSeverity,
    pub code: crate::protocol::DiagnosticCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
}

impl DiagnosticSeverity {
    #[must_use]
    pub fn as_lsp_severity(self) -> u32 {
        match self {
            Self::Error => 1,
            Self::Warning => 2,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompletionSuggestion {
    pub label: String,
    pub kind: CompletionKind,
    pub detail: String,
    pub documentation: String,
    pub insert_text: String,
}

#[derive(Debug, Clone, Copy)]
pub enum CompletionKind {
    Keyword,
    Function,
    Module,
    Property,
    Variable,
    Type,
    Value,
}

impl CompletionKind {
    #[must_use]
    pub fn as_lsp_kind(self) -> u32 {
        match self {
            Self::Keyword => 14,
            Self::Function => 3,
            Self::Module => 9,
            Self::Property => 10,
            Self::Variable => 6,
            Self::Type => 13,
            Self::Value => 12,
        }
    }
}
