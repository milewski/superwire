use superwire_dsl::{
    BuiltinFunctionName, DeclarationKeyword, ExpressionKeyword, ImportKeyword, ReferenceKeyword, ScalarTypeKeyword,
    SingletonDeclarationKind,
};
use superwire_semantic::ProviderDriver;

use lsp_types::{CompletionItemKind, Position};

use super::position::DocumentPosition;
use super::reference::ReferenceCompletionPath;
use super::semantic_index::SemanticIndex;
use super::{CompletionSuggestion, DocumentState, RenderTypeExpression};

impl DocumentState {
    #[must_use]
    pub fn hover_markdown(&self, position: Position) -> Option<String> {
        let hovered_symbol = self.symbol_token_at(position)?;

        if let Some(position_context) = self.position_context(position) {
            if let Some(symbol_markdown) = self
                .semantic_snapshot
                .semantic_index
                .hover_markdown(&hovered_symbol, position_context)
            {
                return Some(symbol_markdown);
            }
        }

        builtin_symbol_markdown(&hovered_symbol)
    }
}

impl SemanticIndex {
    pub fn hover_markdown(&self, hovered_symbol: &str, position: DocumentPosition<'_>) -> Option<String> {
        if let Some(provider_summary) = self.providers.get(hovered_symbol) {
            let provider_driver_name = provider_summary.driver.map_or("unknown", ProviderDriver::as_str);

            return Some(format!("**provider {hovered_symbol}**\n\nDriver: `{provider_driver_name}`"));
        }

        if let Some(model_summary) = self.models.get(hovered_symbol) {
            return Some(format!("**model {hovered_symbol}**\n\nProvider: `{}`", model_summary.provider_name));
        }

        let reference_completion_path = ReferenceCompletionPath::from_token(hovered_symbol)?;
        let mut resolved_accesses = reference_completion_path.complete_accesses.clone();

        if !reference_completion_path.pending_prefix.is_empty() {
            resolved_accesses.push(reference_completion_path.pending_prefix.clone());
        }
        let resolved_reference_accesses = reference_completion_path.resolved_reference_accesses_with_pending();

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
            Some(ReferenceKeyword::Dynamic) => {
                let (dynamic_fields, _) = self.dynamic_scope_at_position(position);
                let field_type = self.resolve_singleton_reference_type(dynamic_fields, resolved_reference_accesses.as_slice())?;

                Some(format!("**{}**\n\nType: `{}`", hovered_symbol, field_type.render_type()))
            }
            Some(ReferenceKeyword::Input) => {
                let field_type = self.resolve_singleton_reference_type(&self.input_fields, resolved_reference_accesses.as_slice())?;

                Some(format!("**{}**\n\nType: `{}`", hovered_symbol, field_type.render_type()))
            }
            Some(ReferenceKeyword::Secrets) => {
                let field_type = self.resolve_singleton_reference_type(&self.secrets_fields, resolved_reference_accesses.as_slice())?;

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

                let candidate_types = self
                    .tooling_snapshot
                    .resolve_reference_access_path_types(vec![agent_output_type.clone()], &resolved_reference_accesses[1..]);
                let final_type = candidate_types.first()?;

                Some(format!("**{}**\n\nType: `{}`", hovered_symbol, final_type.render_type()))
            }
            Some(ReferenceKeyword::Tool) => {
                let tool_name = resolved_accesses.first()?;
                let tool_summary = self.tools.get(tool_name)?;
                let output_type = tool_summary.output_type_expression.as_ref()?;

                if resolved_accesses.len() == 1 {
                    return Some(format!("**tool.{tool_name}**\n\nOutput type: `{}`", output_type.render_type()));
                }

                let candidate_types = self
                    .tooling_snapshot
                    .resolve_reference_access_path_types(vec![output_type.clone()], &resolved_reference_accesses[1..]);
                let final_type = candidate_types.first()?;

                Some(format!("**{}**\n\nType: `{}`", hovered_symbol, final_type.render_type()))
            }
            Some(ReferenceKeyword::Model) => {
                let model_name = resolved_accesses.first()?;
                let model_summary = self.models.get(model_name)?;

                Some(format!("**model.{model_name}**\n\nProvider: `{}`", model_summary.provider_name))
            }
            Some(ReferenceKeyword::Resource) => Some(format!("**{hovered_symbol}**\n\nImported MCP resource reference.")),
            Some(ReferenceKeyword::Prompt) => Some(format!("**{hovered_symbol}**\n\nImported MCP prompt reference.")),
            None => {
                let binding_types = self.for_loop_binding_types_at_position(position, reference_completion_path.root_identifier())?;

                if resolved_reference_accesses.is_empty() {
                    return Some(format!(
                        "**{}**\n\nFor-loop binding type: `{}`",
                        reference_completion_path.root_identifier(),
                        binding_types
                            .iter()
                            .map(RenderTypeExpression::render_type)
                            .collect::<Vec<_>>()
                            .join(" | ")
                    ));
                }

                let candidate_types = self
                    .tooling_snapshot
                    .resolve_reference_access_path_types(binding_types.to_vec(), resolved_reference_accesses.as_slice());
                let final_type = candidate_types.first()?;

                Some(format!("**{}**\n\nType: `{}`", hovered_symbol, final_type.render_type()))
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct BuiltinSymbolDoc {
    label: &'static str,
    kind: CompletionItemKind,
    detail: &'static str,
    documentation: &'static str,
}

fn expression_builtin_symbol_docs() -> [BuiltinSymbolDoc; 12] {
    [
        BuiltinSymbolDoc {
            label: ImportKeyword::From.as_str(),
            kind: CompletionItemKind::KEYWORD,
            detail: "MCP batch import",
            documentation: "Imports multiple MCP items with shared bindings.",
        },
        BuiltinSymbolDoc {
            label: ImportKeyword::As.as_str(),
            kind: CompletionItemKind::KEYWORD,
            detail: "Import alias",
            documentation: "Aliases an item inside an MCP batch import.",
        },
        BuiltinSymbolDoc {
            label: ReferenceKeyword::Tool.as_str(),
            kind: CompletionItemKind::MODULE,
            detail: "Tool namespace",
            documentation: "References declared tools.",
        },
        BuiltinSymbolDoc {
            label: ReferenceKeyword::Resource.as_str(),
            kind: CompletionItemKind::MODULE,
            detail: "Resource namespace",
            documentation: "References imported MCP resources.",
        },
        BuiltinSymbolDoc {
            label: ReferenceKeyword::Prompt.as_str(),
            kind: CompletionItemKind::MODULE,
            detail: "Prompt namespace",
            documentation: "References imported MCP prompts.",
        },
        BuiltinSymbolDoc {
            label: ExpressionKeyword::Context.as_str(),
            kind: CompletionItemKind::FUNCTION,
            detail: "Builtin function",
            documentation: "Returns serialized context for an agent.",
        },
        BuiltinSymbolDoc {
            label: BuiltinFunctionName::Template.as_str(),
            kind: CompletionItemKind::FUNCTION,
            detail: "Builtin function",
            documentation: "Renders a string template from source and bindings.",
        },
        BuiltinSymbolDoc {
            label: ExpressionKeyword::Compact.as_str(),
            kind: CompletionItemKind::FUNCTION,
            detail: "Builtin function",
            documentation: "Compacts nullable values in object-like data.",
        },
        BuiltinSymbolDoc {
            label: ScalarTypeKeyword::String.as_str(),
            kind: CompletionItemKind::STRUCT,
            detail: "Primitive type",
            documentation: "String type.",
        },
        BuiltinSymbolDoc {
            label: ScalarTypeKeyword::Number.as_str(),
            kind: CompletionItemKind::STRUCT,
            detail: "Primitive type",
            documentation: "Integer number type.",
        },
        BuiltinSymbolDoc {
            label: ScalarTypeKeyword::Float.as_str(),
            kind: CompletionItemKind::STRUCT,
            detail: "Primitive type",
            documentation: "Floating-point number type.",
        },
        BuiltinSymbolDoc {
            label: ScalarTypeKeyword::Boolean.as_str(),
            kind: CompletionItemKind::STRUCT,
            detail: "Primitive type",
            documentation: "Boolean type.",
        },
    ]
}

trait DeclarationKeywordCompletionDoc {
    fn completion_detail(self) -> &'static str;

    fn completion_documentation(self) -> &'static str;
}

impl DeclarationKeywordCompletionDoc for DeclarationKeyword {
    fn completion_detail(self) -> &'static str {
        match self {
            DeclarationKeyword::Provider => "Provider declaration",
            DeclarationKeyword::Model => "Model declaration",
            DeclarationKeyword::Mcp => "MCP server declaration",
            DeclarationKeyword::Secrets => "Secrets declaration",
            DeclarationKeyword::Input => "Input declaration",
            DeclarationKeyword::Schema => "Schema declaration",
            DeclarationKeyword::Tool => "Tool declaration",
            DeclarationKeyword::Resource => "MCP resource import",
            DeclarationKeyword::Prompt => "MCP prompt import",
            DeclarationKeyword::Dynamic => "Dynamic declaration",
            DeclarationKeyword::Agent => "Agent declaration",
            DeclarationKeyword::Output => "Output declaration",
        }
    }

    fn completion_documentation(self) -> &'static str {
        match self {
            DeclarationKeyword::Provider => "Declares a provider configuration block.",
            DeclarationKeyword::Model => "Declares a reusable model profile.",
            DeclarationKeyword::Mcp => "Declares an MCP server used for tool discovery and execution.",
            DeclarationKeyword::Secrets => "Declares workflow secret fields.",
            DeclarationKeyword::Input => "Declares workflow input fields.",
            DeclarationKeyword::Schema => "Declares a reusable named schema type.",
            DeclarationKeyword::Tool => "Declares a tool schema that agents can reference.",
            DeclarationKeyword::Resource => "Imports an MCP resource into agent prompt context.",
            DeclarationKeyword::Prompt => "Imports an MCP prompt into agent prompt context.",
            DeclarationKeyword::Dynamic => "Declares dynamic values available through `dynamic.<field>` references.",
            DeclarationKeyword::Agent => "Declares an executable workflow agent.",
            DeclarationKeyword::Output => "Declares final workflow output fields.",
        }
    }
}

pub fn builtin_symbol_suggestions(include_builtin_function_suggestions: bool) -> Vec<CompletionSuggestion> {
    builtin_symbol_docs()
        .filter(|builtin_symbol_doc| {
            include_builtin_function_suggestions || !matches!(builtin_symbol_doc.kind, CompletionItemKind::FUNCTION)
        })
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

fn declaration_builtin_symbol_docs() -> [BuiltinSymbolDoc; 12] {
    [
        BuiltinSymbolDoc {
            label: DeclarationKeyword::Provider.as_str(),
            kind: CompletionItemKind::KEYWORD,
            detail: DeclarationKeyword::Provider.completion_detail(),
            documentation: DeclarationKeyword::Provider.completion_documentation(),
        },
        BuiltinSymbolDoc {
            label: DeclarationKeyword::Model.as_str(),
            kind: CompletionItemKind::KEYWORD,
            detail: DeclarationKeyword::Model.completion_detail(),
            documentation: DeclarationKeyword::Model.completion_documentation(),
        },
        BuiltinSymbolDoc {
            label: DeclarationKeyword::Mcp.as_str(),
            kind: CompletionItemKind::KEYWORD,
            detail: DeclarationKeyword::Mcp.completion_detail(),
            documentation: DeclarationKeyword::Mcp.completion_documentation(),
        },
        BuiltinSymbolDoc {
            label: DeclarationKeyword::Agent.as_str(),
            kind: CompletionItemKind::KEYWORD,
            detail: DeclarationKeyword::Agent.completion_detail(),
            documentation: DeclarationKeyword::Agent.completion_documentation(),
        },
        BuiltinSymbolDoc {
            label: DeclarationKeyword::Schema.as_str(),
            kind: CompletionItemKind::KEYWORD,
            detail: DeclarationKeyword::Schema.completion_detail(),
            documentation: DeclarationKeyword::Schema.completion_documentation(),
        },
        BuiltinSymbolDoc {
            label: DeclarationKeyword::Tool.as_str(),
            kind: CompletionItemKind::KEYWORD,
            detail: DeclarationKeyword::Tool.completion_detail(),
            documentation: DeclarationKeyword::Tool.completion_documentation(),
        },
        BuiltinSymbolDoc {
            label: DeclarationKeyword::Resource.as_str(),
            kind: CompletionItemKind::KEYWORD,
            detail: DeclarationKeyword::Resource.completion_detail(),
            documentation: DeclarationKeyword::Resource.completion_documentation(),
        },
        BuiltinSymbolDoc {
            label: DeclarationKeyword::Prompt.as_str(),
            kind: CompletionItemKind::KEYWORD,
            detail: DeclarationKeyword::Prompt.completion_detail(),
            documentation: DeclarationKeyword::Prompt.completion_documentation(),
        },
        BuiltinSymbolDoc {
            label: DeclarationKeyword::Dynamic.as_str(),
            kind: CompletionItemKind::KEYWORD,
            detail: DeclarationKeyword::Dynamic.completion_detail(),
            documentation: DeclarationKeyword::Dynamic.completion_documentation(),
        },
        BuiltinSymbolDoc {
            label: SingletonDeclarationKind::Input.as_str(),
            kind: CompletionItemKind::KEYWORD,
            detail: DeclarationKeyword::Input.completion_detail(),
            documentation: DeclarationKeyword::Input.completion_documentation(),
        },
        BuiltinSymbolDoc {
            label: SingletonDeclarationKind::Secrets.as_str(),
            kind: CompletionItemKind::KEYWORD,
            detail: DeclarationKeyword::Secrets.completion_detail(),
            documentation: DeclarationKeyword::Secrets.completion_documentation(),
        },
        BuiltinSymbolDoc {
            label: SingletonDeclarationKind::Output.as_str(),
            kind: CompletionItemKind::KEYWORD,
            detail: DeclarationKeyword::Output.completion_detail(),
            documentation: DeclarationKeyword::Output.completion_documentation(),
        },
    ]
}

fn builtin_symbol_docs() -> impl Iterator<Item = BuiltinSymbolDoc> {
    declaration_builtin_symbol_docs()
        .into_iter()
        .chain(expression_builtin_symbol_docs())
}

fn find_builtin_symbol_doc(symbol_name: &str) -> Option<BuiltinSymbolDoc> {
    builtin_symbol_docs().find(|builtin_symbol_doc| builtin_symbol_doc.label == symbol_name)
}
