use crate::protocol::Range;

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

#[derive(Debug, Clone, Copy)]
pub enum SymbolKind {
    Module,
    Function,
    Object,
    Field,
    Struct,
}

impl SymbolKind {
    #[must_use]
    pub fn as_lsp_kind(self) -> u32 {
        match self {
            Self::Module => 2,
            Self::Function => 12,
            Self::Object => 19,
            Self::Field => 8,
            Self::Struct => 23,
        }
    }
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
