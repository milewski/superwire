use engine_ai_core::dsl::{DeclarationKeyword, ReferenceKeyword, SingletonDeclarationKind};
use engine_ai_core::runtime::ProviderDriver;

use crate::protocol::Position;

use super::reference::ReferenceCompletionPath;
use super::semantic_index::SemanticIndex;
use super::{CompletionKind, CompletionSuggestion, DocumentState, RenderTypeExpression};

impl DocumentState {
    #[must_use]
    pub fn hover_markdown(&self, position: Position) -> Option<String> {
        let hovered_symbol = self.symbol_at(position)?;

        if let Some(symbol_markdown) = builtin_symbol_markdown(&hovered_symbol) {
            return Some(symbol_markdown);
        }

        self.semantic_snapshot.semantic_index.hover_markdown(&hovered_symbol)
    }

    fn symbol_at(&self, position: Position) -> Option<String> {
        let line_text = self.text.lines().nth(position.line as usize)?;
        let line_characters: Vec<char> = line_text.chars().collect();

        if line_characters.is_empty() {
            return None;
        }

        let mut cursor_index = usize::min(position.character as usize, line_characters.len().saturating_sub(1));

        if !is_symbol_character(line_characters[cursor_index]) {
            if cursor_index == 0 || !is_symbol_character(line_characters[cursor_index - 1]) {
                return None;
            }

            cursor_index -= 1;
        }

        let mut start_index = cursor_index;

        while start_index > 0 && is_symbol_character(line_characters[start_index - 1]) {
            start_index -= 1;
        }

        let mut end_index = cursor_index + 1;

        while end_index < line_characters.len() && is_symbol_character(line_characters[end_index]) {
            end_index += 1;
        }

        Some(line_characters[start_index..end_index].iter().collect())
    }
}

impl SemanticIndex {
    pub(super) fn hover_markdown(&self, hovered_symbol: &str) -> Option<String> {
        if let Some(provider_summary) = self.providers.get(hovered_symbol) {
            let provider_driver_name = provider_summary.driver.map_or("unknown", ProviderDriver::as_str);

            return Some(format!(
                "**provider {hovered_symbol}**\n\nDriver: `{provider_driver_name}`\n\nDeclared models: {}",
                if provider_summary.models.is_empty() {
                    "none".to_string()
                } else {
                    provider_summary.models.join(", ")
                }
            ));
        }

        let reference_completion_path = ReferenceCompletionPath::from_token(hovered_symbol)?;
        let mut resolved_accesses = reference_completion_path.complete_accesses.clone();

        if !reference_completion_path.pending_prefix.is_empty() {
            resolved_accesses.push(reference_completion_path.pending_prefix.clone());
        }

        if reference_completion_path.is_schema_root() {
            let schema_name = resolved_accesses.first()?;
            let schema_summary = self.schemas.get(schema_name)?;

            return Some(format!(
                "**schema.{schema_name}**\n\nFields: {}",
                schema_summary
                    .fields
                    .iter()
                    .map(|(field_name, field_type)| format!("`{field_name}: {}`", field_type.render_type()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        match reference_completion_path.root_keyword() {
            Some(ReferenceKeyword::Input) => {
                let field_type = self.resolve_singleton_reference_type(&self.input_fields, resolved_accesses.as_slice())?;

                Some(format!("**{}**\n\nType: `{}`", hovered_symbol, field_type.render_type()))
            }
            Some(ReferenceKeyword::Secrets) => {
                let field_type = self.resolve_singleton_reference_type(&self.secrets_fields, resolved_accesses.as_slice())?;

                Some(format!("**{}**\n\nType: `{}`", hovered_symbol, field_type.render_type()))
            }
            Some(ReferenceKeyword::Agent) => {
                let agent_name = resolved_accesses.first()?;
                let agent_summary = self.agents.get(agent_name)?;

                let agent_output_type = agent_summary.output_type.as_ref()?;

                if resolved_accesses.len() == 1 {
                    return Some(format!(
                        "**agent.{agent_name}**\n\nOutput type: `{}`",
                        agent_output_type.render_type()
                    ));
                }

                let candidate_types = self.resolve_access_path(vec![agent_output_type.clone()], &resolved_accesses[1..]);
                let final_type = candidate_types.first()?;

                Some(format!("**{}**\n\nType: `{}`", hovered_symbol, final_type.render_type()))
            }
            Some(ReferenceKeyword::Tool) | None => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct BuiltinSymbolDoc {
    label: &'static str,
    kind: CompletionKind,
    detail: &'static str,
    documentation: &'static str,
}

const BUILTIN_SYMBOL_DOCS: [BuiltinSymbolDoc; 8] = [
    BuiltinSymbolDoc {
        label: "tool",
        kind: CompletionKind::Module,
        detail: "Tool namespace",
        documentation: "Use `tool.<name>` to reference declared tools.",
    },
    BuiltinSymbolDoc {
        label: "context",
        kind: CompletionKind::Function,
        detail: "Builtin function",
        documentation: "Returns serialized context for `agent.<name>`.",
    },
    BuiltinSymbolDoc {
        label: "template",
        kind: CompletionKind::Function,
        detail: "Builtin function",
        documentation: "Renders a string template from source and bindings.",
    },
    BuiltinSymbolDoc {
        label: "compact",
        kind: CompletionKind::Function,
        detail: "Builtin function",
        documentation: "Compacts nullable values in object-like data.",
    },
    BuiltinSymbolDoc {
        label: "string",
        kind: CompletionKind::Type,
        detail: "Primitive type",
        documentation: "String type.",
    },
    BuiltinSymbolDoc {
        label: "number",
        kind: CompletionKind::Type,
        detail: "Primitive type",
        documentation: "Integer number type.",
    },
    BuiltinSymbolDoc {
        label: "float",
        kind: CompletionKind::Type,
        detail: "Primitive type",
        documentation: "Floating-point number type.",
    },
    BuiltinSymbolDoc {
        label: "boolean",
        kind: CompletionKind::Type,
        detail: "Primitive type",
        documentation: "Boolean type.",
    },
];

trait DeclarationKeywordCompletionDoc {
    fn completion_detail(self) -> &'static str;

    fn completion_documentation(self) -> &'static str;
}

impl DeclarationKeywordCompletionDoc for DeclarationKeyword {
    fn completion_detail(self) -> &'static str {
        match self {
            DeclarationKeyword::Provider => "Provider declaration",
            DeclarationKeyword::Secrets => "Secrets declaration",
            DeclarationKeyword::Input => "Input declaration",
            DeclarationKeyword::Schema => "Schema declaration",
            DeclarationKeyword::Agent => "Agent declaration",
            DeclarationKeyword::Output => "Output declaration",
        }
    }

    fn completion_documentation(self) -> &'static str {
        match self {
            DeclarationKeyword::Provider => "Declares a provider configuration block.",
            DeclarationKeyword::Secrets => "Declares workflow secret fields.",
            DeclarationKeyword::Input => "Declares workflow input fields.",
            DeclarationKeyword::Schema => "Declares a reusable named schema type.",
            DeclarationKeyword::Agent => "Declares an executable workflow agent.",
            DeclarationKeyword::Output => "Declares final workflow output fields.",
        }
    }
}

pub(super) fn builtin_symbol_suggestions(include_builtin_function_suggestions: bool) -> Vec<CompletionSuggestion> {
    builtin_symbol_docs()
        .filter(|builtin_symbol_doc| include_builtin_function_suggestions || !matches!(builtin_symbol_doc.kind, CompletionKind::Function))
        .map(|builtin_symbol_doc| CompletionSuggestion {
            label: builtin_symbol_doc.label.to_string(),
            kind: builtin_symbol_doc.kind,
            detail: builtin_symbol_doc.detail.to_string(),
            documentation: builtin_symbol_doc.documentation.to_string(),
            insert_text: builtin_symbol_doc.label.to_string(),
        })
        .collect()
}

fn builtin_symbol_markdown(symbol_name: &str) -> Option<String> {
    let direct_match = find_builtin_symbol_doc(symbol_name).or_else(|| symbol_name.rsplit('.').next().and_then(find_builtin_symbol_doc))?;

    Some(format!(
        "**{}**\n\n{}\n\n{}",
        direct_match.label, direct_match.detail, direct_match.documentation
    ))
}

fn declaration_builtin_symbol_docs() -> [BuiltinSymbolDoc; 6] {
    [
        BuiltinSymbolDoc {
            label: DeclarationKeyword::Provider.as_str(),
            kind: CompletionKind::Keyword,
            detail: DeclarationKeyword::Provider.completion_detail(),
            documentation: DeclarationKeyword::Provider.completion_documentation(),
        },
        BuiltinSymbolDoc {
            label: DeclarationKeyword::Agent.as_str(),
            kind: CompletionKind::Keyword,
            detail: DeclarationKeyword::Agent.completion_detail(),
            documentation: DeclarationKeyword::Agent.completion_documentation(),
        },
        BuiltinSymbolDoc {
            label: DeclarationKeyword::Schema.as_str(),
            kind: CompletionKind::Keyword,
            detail: DeclarationKeyword::Schema.completion_detail(),
            documentation: DeclarationKeyword::Schema.completion_documentation(),
        },
        BuiltinSymbolDoc {
            label: SingletonDeclarationKind::Input.as_str(),
            kind: CompletionKind::Keyword,
            detail: DeclarationKeyword::Input.completion_detail(),
            documentation: DeclarationKeyword::Input.completion_documentation(),
        },
        BuiltinSymbolDoc {
            label: SingletonDeclarationKind::Secrets.as_str(),
            kind: CompletionKind::Keyword,
            detail: DeclarationKeyword::Secrets.completion_detail(),
            documentation: DeclarationKeyword::Secrets.completion_documentation(),
        },
        BuiltinSymbolDoc {
            label: SingletonDeclarationKind::Output.as_str(),
            kind: CompletionKind::Keyword,
            detail: DeclarationKeyword::Output.completion_detail(),
            documentation: DeclarationKeyword::Output.completion_documentation(),
        },
    ]
}

fn builtin_symbol_docs() -> impl Iterator<Item = BuiltinSymbolDoc> {
    declaration_builtin_symbol_docs().into_iter().chain(BUILTIN_SYMBOL_DOCS)
}

fn find_builtin_symbol_doc(symbol_name: &str) -> Option<BuiltinSymbolDoc> {
    builtin_symbol_docs().find(|builtin_symbol_doc| builtin_symbol_doc.label == symbol_name)
}

fn is_symbol_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_' || character == '.' || character == '?'
}
