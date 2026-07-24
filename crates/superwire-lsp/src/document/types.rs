use lsp_types::{CompletionItemKind, DiagnosticSeverity, Range, SymbolKind};

use crate::diagnostic_code::DiagnosticCode;

#[derive(Debug, Clone)]
pub struct CodeActionSuggestion {
    pub title: String,
    pub edit: CodeActionEdit,
}

#[derive(Debug, Clone)]
pub struct CodeActionEdit {
    pub range: Range,
    pub new_text: String,
}

#[derive(Debug, Clone)]
pub struct DocumentDiagnosticRelated {
    pub range: Range,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct DocumentDiagnostic {
    pub range: Range,
    pub severity: DiagnosticSeverity,
    pub code: DiagnosticCode,
    pub message: String,
    pub related: Vec<DocumentDiagnosticRelated>,
    pub notes: Vec<String>,
    pub help: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CompletionSuggestion {
    pub label: String,
    pub kind: CompletionItemKind,
    pub detail: String,
    pub documentation: String,
    pub insert_text: String,
}

#[derive(Debug, Clone)]
pub struct DocumentSymbolNode {
    pub name: String,
    pub detail: Option<String>,
    pub kind: SymbolKind,
    pub range: Range,
    pub selection_range: Range,
    pub children: Vec<DocumentSymbolNode>,
}

impl DocumentSymbolNode {
    pub fn collect_workspace_symbols(
        &self,
        document_uri: &str,
        container_name: Option<&str>,
        workspace_symbols: &mut Vec<WorkspaceSymbolMatch>,
    ) {
        workspace_symbols.push(WorkspaceSymbolMatch {
            name: self.name.clone(),
            kind: self.kind,
            range: self.selection_range,
            container_name: container_name.map(ToOwned::to_owned),
            document_uri: document_uri.to_string(),
        });

        for child_symbol in &self.children {
            child_symbol.collect_workspace_symbols(document_uri, Some(&self.name), workspace_symbols);
        }
    }
}

#[derive(Debug, Clone)]
pub struct WorkspaceSymbolMatch {
    pub name: String,
    pub kind: SymbolKind,
    pub range: Range,
    pub container_name: Option<String>,
    pub document_uri: String,
}

impl WorkspaceSymbolMatch {
    #[must_use]
    pub fn matches_query(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }

        let lowercase_query = query.to_ascii_lowercase();
        let lowercase_name = self.name.to_ascii_lowercase();

        lowercase_name.contains(&lowercase_query)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FoldingRangeBlock {
    pub start_line: u32,
    pub start_character: u32,
    pub end_line: u32,
    pub end_character: u32,
}

#[derive(Debug, Clone)]
pub struct DocumentFormattingEdit {
    pub range: Range,
    pub new_text: String,
}

#[derive(Debug, Clone)]
pub struct CodeLensHint {
    pub range: Range,
    pub title: String,
    pub command: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticHighlightKind {
    Keyword,
    Type,
    Class,
    Property,
    Function,
    Variable,
    String,
    Number,
    Comment,
    Operator,
    EnumMember,
    Namespace,
}

impl SemanticHighlightKind {
    #[must_use]
    pub const fn legend_index(self) -> u32 {
        match self {
            Self::Keyword => 0,
            Self::Type => 1,
            Self::Class => 2,
            Self::Property => 3,
            Self::Function => 4,
            Self::Variable => 5,
            Self::String => 6,
            Self::Number => 7,
            Self::Comment => 8,
            Self::Operator => 9,
            Self::EnumMember => 10,
            Self::Namespace => 11,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SemanticHighlight {
    pub range: Range,
    pub kind: SemanticHighlightKind,
}
