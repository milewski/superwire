use lsp_types::SymbolKind;
use superwire_core::dsl::{parse_workflow, Declaration, DeclarationKeyword, ToolDeclaration, TypedField, Workflow};

use super::position::source_span_to_range;
use super::semantic_index::SemanticIndex;
use super::{CodeLensHint, DocumentState, DocumentSymbolNode, RenderTypeExpression, WorkspaceSymbolMatch};

impl DocumentState {
    #[must_use]
    pub fn document_symbols(&self) -> Vec<DocumentSymbolNode> {
        if let Ok(workflow) = parse_workflow(&self.text) {
            return workflow.document_symbol_nodes(&self.text);
        }

        self.semantic_snapshot.semantic_index.fallback_document_symbols(&self.text)
    }

    #[must_use]
    pub fn workspace_symbols(&self, document_uri: &str, query: &str) -> Vec<WorkspaceSymbolMatch> {
        let mut workspace_symbols = Vec::new();

        for top_level_symbol in self.document_symbols() {
            top_level_symbol.collect_workspace_symbols(document_uri, None, &mut workspace_symbols);
        }

        workspace_symbols
            .into_iter()
            .filter(|workspace_symbol| workspace_symbol.matches_query(query))
            .collect()
    }

    #[must_use]
    pub fn generated_output_marks(&self) -> Vec<CodeLensHint> {
        self.semantic_snapshot.semantic_index.generated_output_marks(&self.text)
    }
}

trait WorkflowDocumentSymbolExt {
    fn document_symbol_nodes(&self, source_text: &str) -> Vec<DocumentSymbolNode>;
}

impl WorkflowDocumentSymbolExt for Workflow {
    fn document_symbol_nodes(&self, source_text: &str) -> Vec<DocumentSymbolNode> {
        self.declarations
            .iter()
            .map(|declaration| declaration.document_symbol_node(source_text))
            .collect()
    }
}

trait DeclarationDocumentSymbolExt {
    fn document_symbol_node(&self, source_text: &str) -> DocumentSymbolNode;
}

impl DeclarationDocumentSymbolExt for Declaration {
    #[allow(clippy::too_many_lines)]
    fn document_symbol_node(&self, source_text: &str) -> DocumentSymbolNode {
        match self {
            Self::Provider(provider_declaration) => {
                let declaration_range = source_span_to_range(source_text, provider_declaration.span);

                DocumentSymbolNode {
                    name: provider_declaration.name.clone(),
                    detail: Some("provider declaration".to_string()),
                    kind: SymbolKind::MODULE,
                    range: declaration_range,
                    selection_range: declaration_range,
                    children: Vec::new(),
                }
            }
            Self::Model(model_declaration) => {
                let declaration_range = source_span_to_range(source_text, model_declaration.span);

                DocumentSymbolNode {
                    name: model_declaration.name.clone(),
                    detail: Some("model declaration".to_string()),
                    kind: SymbolKind::MODULE,
                    range: declaration_range,
                    selection_range: declaration_range,
                    children: Vec::new(),
                }
            }
            Self::McpServer(mcp_server_declaration) => {
                let declaration_range = source_span_to_range(source_text, mcp_server_declaration.span);

                DocumentSymbolNode {
                    name: mcp_server_declaration.name.clone(),
                    detail: Some("MCP server declaration".to_string()),
                    kind: SymbolKind::MODULE,
                    range: declaration_range,
                    selection_range: declaration_range,
                    children: Vec::new(),
                }
            }
            Self::Schema(schema_declaration) => {
                let declaration_range = source_span_to_range(source_text, schema_declaration.span);
                let child_symbols = schema_declaration
                    .fields
                    .iter()
                    .map(|typed_field| typed_field.document_symbol_node(source_text))
                    .collect();

                DocumentSymbolNode {
                    name: schema_declaration.name.clone(),
                    detail: Some("schema declaration".to_string()),
                    kind: SymbolKind::STRUCT,
                    range: declaration_range,
                    selection_range: declaration_range,
                    children: child_symbols,
                }
            }
            Self::Tool(tool_declaration) => tool_declaration.document_symbol_node(source_text),
            Self::McpToolBatch(tool_batch_import_declaration) => {
                let declaration_range = source_span_to_range(source_text, tool_batch_import_declaration.span);
                let child_symbols = tool_batch_import_declaration
                    .tools
                    .iter()
                    .map(|tool_declaration| tool_declaration.document_symbol_node(source_text))
                    .collect();

                DocumentSymbolNode {
                    name: format!("mcp.{}.tool", tool_batch_import_declaration.server_name),
                    detail: Some("MCP tool batch import".to_string()),
                    kind: SymbolKind::MODULE,
                    range: declaration_range,
                    selection_range: declaration_range,
                    children: child_symbols,
                }
            }
            Self::McpBatch(batch_import_declaration) => {
                let declaration_range = source_span_to_range(source_text, batch_import_declaration.span);
                let mut child_symbols = batch_import_declaration
                    .tools
                    .iter()
                    .map(|tool_declaration| tool_declaration.document_symbol_node(source_text))
                    .collect::<Vec<_>>();

                child_symbols.extend(batch_import_declaration.resources.iter().map(|resource_import_declaration| {
                    let declaration_range = source_span_to_range(source_text, resource_import_declaration.span);

                    DocumentSymbolNode {
                        name: resource_import_declaration.name.clone(),
                        detail: Some("MCP resource import".to_string()),
                        kind: SymbolKind::OBJECT,
                        range: declaration_range,
                        selection_range: declaration_range,
                        children: Vec::new(),
                    }
                }));

                child_symbols.extend(batch_import_declaration.prompts.iter().map(|prompt_import_declaration| {
                    let declaration_range = source_span_to_range(source_text, prompt_import_declaration.span);

                    DocumentSymbolNode {
                        name: prompt_import_declaration.name.clone(),
                        detail: Some("MCP prompt import".to_string()),
                        kind: SymbolKind::FIELD,
                        range: declaration_range,
                        selection_range: declaration_range,
                        children: Vec::new(),
                    }
                }));

                DocumentSymbolNode {
                    name: format!("mcp.{}", batch_import_declaration.server_name),
                    detail: Some("MCP batch import".to_string()),
                    kind: SymbolKind::MODULE,
                    range: declaration_range,
                    selection_range: declaration_range,
                    children: child_symbols,
                }
            }
            Self::McpResource(resource_import_declaration) => {
                let declaration_range = source_span_to_range(source_text, resource_import_declaration.span);

                DocumentSymbolNode {
                    name: resource_import_declaration.name.clone(),
                    detail: Some("MCP resource import".to_string()),
                    kind: SymbolKind::OBJECT,
                    range: declaration_range,
                    selection_range: declaration_range,
                    children: Vec::new(),
                }
            }
            Self::McpResourceBatch(resource_batch_import_declaration) => {
                let declaration_range = source_span_to_range(source_text, resource_batch_import_declaration.span);
                let child_symbols = resource_batch_import_declaration
                    .resources
                    .iter()
                    .map(|resource_import_declaration| {
                        let declaration_range = source_span_to_range(source_text, resource_import_declaration.span);

                        DocumentSymbolNode {
                            name: resource_import_declaration.name.clone(),
                            detail: Some("MCP resource import".to_string()),
                            kind: SymbolKind::OBJECT,
                            range: declaration_range,
                            selection_range: declaration_range,
                            children: Vec::new(),
                        }
                    })
                    .collect();

                DocumentSymbolNode {
                    name: format!("mcp.{}.resource", resource_batch_import_declaration.server_name),
                    detail: Some("MCP resource batch import".to_string()),
                    kind: SymbolKind::MODULE,
                    range: declaration_range,
                    selection_range: declaration_range,
                    children: child_symbols,
                }
            }
            Self::McpPrompt(prompt_import_declaration) => {
                let declaration_range = source_span_to_range(source_text, prompt_import_declaration.span);

                DocumentSymbolNode {
                    name: prompt_import_declaration.name.clone(),
                    detail: Some("MCP prompt import".to_string()),
                    kind: SymbolKind::FIELD,
                    range: declaration_range,
                    selection_range: declaration_range,
                    children: Vec::new(),
                }
            }
            Self::McpPromptBatch(prompt_batch_import_declaration) => {
                let declaration_range = source_span_to_range(source_text, prompt_batch_import_declaration.span);
                let child_symbols = prompt_batch_import_declaration
                    .prompts
                    .iter()
                    .map(|prompt_import_declaration| {
                        let declaration_range = source_span_to_range(source_text, prompt_import_declaration.span);

                        DocumentSymbolNode {
                            name: prompt_import_declaration.name.clone(),
                            detail: Some("MCP prompt import".to_string()),
                            kind: SymbolKind::FIELD,
                            range: declaration_range,
                            selection_range: declaration_range,
                            children: Vec::new(),
                        }
                    })
                    .collect();

                DocumentSymbolNode {
                    name: format!("mcp.{}.prompt", prompt_batch_import_declaration.server_name),
                    detail: Some("MCP prompt batch import".to_string()),
                    kind: SymbolKind::MODULE,
                    range: declaration_range,
                    selection_range: declaration_range,
                    children: child_symbols,
                }
            }
            Self::Input(input_declaration) => {
                let declaration_range = source_span_to_range(source_text, input_declaration.span);
                let child_symbols = input_declaration
                    .fields
                    .iter()
                    .map(|typed_field| typed_field.document_symbol_node(source_text))
                    .collect();

                DocumentSymbolNode {
                    name: DeclarationKeyword::Input.as_str().to_string(),
                    detail: Some("input declaration".to_string()),
                    kind: SymbolKind::OBJECT,
                    range: declaration_range,
                    selection_range: declaration_range,
                    children: child_symbols,
                }
            }
            Self::Secrets(secrets_declaration) => {
                let declaration_range = source_span_to_range(source_text, secrets_declaration.span);
                let child_symbols = secrets_declaration
                    .fields
                    .iter()
                    .map(|typed_field| typed_field.document_symbol_node(source_text))
                    .collect();

                DocumentSymbolNode {
                    name: DeclarationKeyword::Secrets.as_str().to_string(),
                    detail: Some("secrets declaration".to_string()),
                    kind: SymbolKind::OBJECT,
                    range: declaration_range,
                    selection_range: declaration_range,
                    children: child_symbols,
                }
            }
            Self::Agent(agent_declaration) => {
                let declaration_range = source_span_to_range(source_text, agent_declaration.span);

                DocumentSymbolNode {
                    name: agent_declaration.name.clone(),
                    detail: Some("agent declaration".to_string()),
                    kind: SymbolKind::FUNCTION,
                    range: declaration_range,
                    selection_range: declaration_range,
                    children: Vec::new(),
                }
            }
            Self::Dynamic(dynamic_block) => {
                let declaration_range = source_span_to_range(source_text, dynamic_block.span);

                DocumentSymbolNode {
                    name: DeclarationKeyword::Dynamic.as_str().to_string(),
                    detail: Some("dynamic declaration".to_string()),
                    kind: SymbolKind::FIELD,
                    range: declaration_range,
                    selection_range: declaration_range,
                    children: Vec::new(),
                }
            }
            Self::Output(output_declaration) => {
                let declaration_range = source_span_to_range(source_text, output_declaration.span);

                DocumentSymbolNode {
                    name: DeclarationKeyword::Output.as_str().to_string(),
                    detail: Some("output declaration".to_string()),
                    kind: SymbolKind::OBJECT,
                    range: declaration_range,
                    selection_range: declaration_range,
                    children: Vec::new(),
                }
            }
        }
    }
}

trait ToolDeclarationDocumentSymbolExt {
    fn document_symbol_node(&self, source_text: &str) -> DocumentSymbolNode;
}

impl ToolDeclarationDocumentSymbolExt for ToolDeclaration {
    fn document_symbol_node(&self, source_text: &str) -> DocumentSymbolNode {
        let declaration_range = source_span_to_range(source_text, self.span);
        let child_symbols = self
            .input_fields
            .iter()
            .chain(self.binding_fields.iter())
            .chain(self.output_fields.iter())
            .map(|typed_field| typed_field.document_symbol_node(source_text))
            .collect();

        DocumentSymbolNode {
            name: self.name.clone(),
            detail: Some("tool declaration".to_string()),
            kind: SymbolKind::FUNCTION,
            range: declaration_range,
            selection_range: declaration_range,
            children: child_symbols,
        }
    }
}

trait TypedFieldDocumentSymbolExt {
    fn document_symbol_node(&self, source_text: &str) -> DocumentSymbolNode;
}

impl TypedFieldDocumentSymbolExt for TypedField {
    fn document_symbol_node(&self, source_text: &str) -> DocumentSymbolNode {
        let field_range = source_span_to_range(source_text, self.span);

        DocumentSymbolNode {
            name: self.name.clone(),
            detail: Some(format!("field: {}", self.field_type.render_type())),
            kind: SymbolKind::FIELD,
            range: field_range,
            selection_range: field_range,
            children: Vec::new(),
        }
    }
}

impl SemanticIndex {
    fn fallback_document_symbols(&self, source_text: &str) -> Vec<DocumentSymbolNode> {
        let mut symbol_nodes = Vec::new();

        for provider_location in &self.provider_locations {
            let provider_range = source_span_to_range(source_text, provider_location.span);

            symbol_nodes.push(DocumentSymbolNode {
                name: provider_location.name.clone(),
                detail: Some("provider declaration".to_string()),
                kind: SymbolKind::MODULE,
                range: provider_range,
                selection_range: provider_range,
                children: Vec::new(),
            });
        }

        for schema_location in &self.schema_locations {
            let schema_range = source_span_to_range(source_text, schema_location.span);

            symbol_nodes.push(DocumentSymbolNode {
                name: schema_location.name.clone(),
                detail: Some("schema declaration".to_string()),
                kind: SymbolKind::STRUCT,
                range: schema_range,
                selection_range: schema_range,
                children: Vec::new(),
            });
        }

        for agent_location in &self.agent_locations {
            let agent_range = source_span_to_range(source_text, agent_location.span);

            symbol_nodes.push(DocumentSymbolNode {
                name: agent_location.name.clone(),
                detail: Some("agent declaration".to_string()),
                kind: SymbolKind::FUNCTION,
                range: agent_range,
                selection_range: agent_range,
                children: Vec::new(),
            });
        }

        for output_location in &self.output_locations {
            let output_range = source_span_to_range(source_text, *output_location);

            symbol_nodes.push(DocumentSymbolNode {
                name: DeclarationKeyword::Output.as_str().to_string(),
                detail: Some("output declaration".to_string()),
                kind: SymbolKind::OBJECT,
                range: output_range,
                selection_range: output_range,
                children: Vec::new(),
            });
        }

        symbol_nodes.sort_by(|left_symbol, right_symbol| {
            left_symbol
                .range
                .start
                .line
                .cmp(&right_symbol.range.start.line)
                .then(left_symbol.range.start.character.cmp(&right_symbol.range.start.character))
        });

        symbol_nodes
    }

    fn generated_output_marks(&self, source_text: &str) -> Vec<CodeLensHint> {
        self.output_locations
            .iter()
            .map(|output_location| {
                let output_range = source_span_to_range(source_text, *output_location);

                CodeLensHint {
                    range: output_range,
                    title: "Generated output".to_string(),
                    command: "superwire.generated.output".to_string(),
                }
            })
            .collect()
    }
}
