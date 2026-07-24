use lsp_types::SymbolKind;
use superwire_dsl::{
    AgentDeclaration, AgentProperty, Declaration, DeclarationKeyword, Expression, ObjectField, ToolDeclaration, TypeExpression, TypedField,
    VariantCase, Workflow,
};

use super::position::LineIndex;
use super::semantic_index::SemanticIndex;
use super::syntax::SyntaxSnapshot;
use super::{CodeLensHint, DocumentState, DocumentSymbolNode, RenderTypeExpression, WorkspaceSymbolMatch};

impl DocumentState {
    #[must_use]
    pub fn document_symbols(&self) -> Vec<DocumentSymbolNode> {
        if let Some(workflow) = self.semantic_snapshot.workflow_document().workflow() {
            return workflow.document_symbol_nodes(&self.text, &self.line_index);
        }

        self.semantic_snapshot
            .semantic_index
            .fallback_document_symbols(&self.text, &self.line_index, &self.syntax_snapshot)
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
        self.semantic_snapshot
            .semantic_index
            .generated_output_marks(&self.text, &self.line_index)
    }
}

trait WorkflowDocumentSymbolExt {
    fn document_symbol_nodes(&self, source_text: &str, line_index: &LineIndex) -> Vec<DocumentSymbolNode>;
}

impl WorkflowDocumentSymbolExt for Workflow {
    fn document_symbol_nodes(&self, source_text: &str, line_index: &LineIndex) -> Vec<DocumentSymbolNode> {
        self.declarations
            .iter()
            .map(|declaration| declaration.document_symbol_node(source_text, line_index))
            .collect()
    }
}

trait DeclarationDocumentSymbolExt {
    fn document_symbol_node(&self, source_text: &str, line_index: &LineIndex) -> DocumentSymbolNode;
}

impl DeclarationDocumentSymbolExt for Declaration {
    #[allow(clippy::too_many_lines)]
    fn document_symbol_node(&self, source_text: &str, line_index: &LineIndex) -> DocumentSymbolNode {
        match self {
            Self::Provider(provider_declaration) => {
                let declaration_range = line_index.source_span_range(source_text, provider_declaration.span);

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
                let declaration_range = line_index.source_span_range(source_text, model_declaration.span);

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
                let declaration_range = line_index.source_span_range(source_text, mcp_server_declaration.span);

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
                let declaration_range = line_index.source_span_range(source_text, schema_declaration.span);
                let mut child_symbols = schema_declaration
                    .fields
                    .iter()
                    .map(|typed_field| typed_field.document_symbol_node(source_text, line_index))
                    .collect::<Vec<_>>();

                if let Some(root_variant) = &schema_declaration.root_variant {
                    child_symbols.extend(root_variant.document_symbol_nodes(source_text, line_index));
                }

                DocumentSymbolNode {
                    name: schema_declaration.name.clone(),
                    detail: Some("schema declaration".to_string()),
                    kind: SymbolKind::STRUCT,
                    range: declaration_range,
                    selection_range: declaration_range,
                    children: child_symbols,
                }
            }
            Self::Tool(tool_declaration) => tool_declaration.document_symbol_node(source_text, line_index),
            Self::McpToolBatch(tool_batch_import_declaration) => {
                let declaration_range = line_index.source_span_range(source_text, tool_batch_import_declaration.span);
                let child_symbols = tool_batch_import_declaration
                    .tools
                    .iter()
                    .map(|tool_declaration| tool_declaration.document_symbol_node(source_text, line_index))
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
                let declaration_range = line_index.source_span_range(source_text, batch_import_declaration.span);
                let mut child_symbols = batch_import_declaration
                    .tools
                    .iter()
                    .map(|tool_declaration| tool_declaration.document_symbol_node(source_text, line_index))
                    .collect::<Vec<_>>();

                child_symbols.extend(batch_import_declaration.resources.iter().map(|resource_import_declaration| {
                    let declaration_range = line_index.source_span_range(source_text, resource_import_declaration.span);

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
                    let declaration_range = line_index.source_span_range(source_text, prompt_import_declaration.span);

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
                let declaration_range = line_index.source_span_range(source_text, resource_import_declaration.span);

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
                let declaration_range = line_index.source_span_range(source_text, resource_batch_import_declaration.span);
                let child_symbols = resource_batch_import_declaration
                    .resources
                    .iter()
                    .map(|resource_import_declaration| {
                        let declaration_range = line_index.source_span_range(source_text, resource_import_declaration.span);

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
                let declaration_range = line_index.source_span_range(source_text, prompt_import_declaration.span);

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
                let declaration_range = line_index.source_span_range(source_text, prompt_batch_import_declaration.span);
                let child_symbols = prompt_batch_import_declaration
                    .prompts
                    .iter()
                    .map(|prompt_import_declaration| {
                        let declaration_range = line_index.source_span_range(source_text, prompt_import_declaration.span);

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
                let declaration_range = line_index.source_span_range(source_text, input_declaration.span);
                let child_symbols = input_declaration
                    .fields
                    .iter()
                    .map(|typed_field| typed_field.document_symbol_node(source_text, line_index))
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
                let declaration_range = line_index.source_span_range(source_text, secrets_declaration.span);
                let child_symbols = secrets_declaration
                    .fields
                    .iter()
                    .map(|typed_field| typed_field.document_symbol_node(source_text, line_index))
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
            Self::Agent(agent_declaration) => agent_declaration.document_symbol_node(source_text, line_index),
            Self::Dynamic(dynamic_block) => {
                let declaration_range = line_index.source_span_range(source_text, dynamic_block.span);
                let child_symbols = dynamic_block
                    .fields
                    .iter()
                    .map(|object_field| object_field.document_symbol_node(source_text, line_index))
                    .collect();

                DocumentSymbolNode {
                    name: DeclarationKeyword::Dynamic.as_str().to_string(),
                    detail: Some("dynamic declaration".to_string()),
                    kind: SymbolKind::FIELD,
                    range: declaration_range,
                    selection_range: declaration_range,
                    children: child_symbols,
                }
            }
            Self::Output(output_declaration) => {
                let declaration_range = line_index.source_span_range(source_text, output_declaration.span);
                let child_symbols = output_declaration
                    .fields
                    .iter()
                    .map(|object_field| object_field.document_symbol_node(source_text, line_index))
                    .collect();

                DocumentSymbolNode {
                    name: DeclarationKeyword::Output.as_str().to_string(),
                    detail: Some("output declaration".to_string()),
                    kind: SymbolKind::OBJECT,
                    range: declaration_range,
                    selection_range: declaration_range,
                    children: child_symbols,
                }
            }
        }
    }
}

trait ToolDeclarationDocumentSymbolExt {
    fn document_symbol_node(&self, source_text: &str, line_index: &LineIndex) -> DocumentSymbolNode;
}

impl ToolDeclarationDocumentSymbolExt for ToolDeclaration {
    fn document_symbol_node(&self, source_text: &str, line_index: &LineIndex) -> DocumentSymbolNode {
        let declaration_range = line_index.source_span_range(source_text, self.span);
        let child_symbols = self
            .input_fields
            .iter()
            .chain(self.binding_fields.iter())
            .chain(self.output_fields.iter())
            .map(|typed_field| typed_field.document_symbol_node(source_text, line_index))
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
    fn document_symbol_node(&self, source_text: &str, line_index: &LineIndex) -> DocumentSymbolNode;
}

impl TypedFieldDocumentSymbolExt for TypedField {
    fn document_symbol_node(&self, source_text: &str, line_index: &LineIndex) -> DocumentSymbolNode {
        let field_range = line_index.source_span_range(source_text, self.span);

        DocumentSymbolNode {
            name: self.name.clone(),
            detail: Some(format!("field: {}", self.field_type.render_type())),
            kind: SymbolKind::FIELD,
            range: field_range,
            selection_range: field_range,
            children: self.field_type.document_symbol_nodes(source_text, line_index),
        }
    }
}

trait TypeExpressionDocumentSymbolExt {
    fn document_symbol_nodes(&self, source_text: &str, line_index: &LineIndex) -> Vec<DocumentSymbolNode>;
}

impl TypeExpressionDocumentSymbolExt for TypeExpression {
    fn document_symbol_nodes(&self, source_text: &str, line_index: &LineIndex) -> Vec<DocumentSymbolNode> {
        match self {
            Self::Object(typed_fields) => typed_fields
                .iter()
                .map(|typed_field| typed_field.document_symbol_node(source_text, line_index))
                .collect(),
            Self::Variant { discriminator: _, cases } => cases
                .iter()
                .map(|variant_case| variant_case.document_symbol_node(source_text, line_index))
                .collect(),
            Self::Array {
                item_type,
                fixed_length: _,
            } => item_type.document_symbol_nodes(source_text, line_index),
            Self::Tuple(type_expressions) | Self::Union(type_expressions) => type_expressions
                .iter()
                .flat_map(|type_expression| type_expression.document_symbol_nodes(source_text, line_index))
                .collect(),
            Self::String
            | Self::Number
            | Self::Float
            | Self::Boolean
            | Self::Null
            | Self::AnyObject
            | Self::SchemaReference(_)
            | Self::StringEnum(_)
            | Self::StringEnumReference(_) => Vec::new(),
        }
    }
}

trait VariantCaseDocumentSymbolExt {
    fn document_symbol_node(&self, source_text: &str, line_index: &LineIndex) -> DocumentSymbolNode;
}

impl VariantCaseDocumentSymbolExt for VariantCase {
    fn document_symbol_node(&self, source_text: &str, line_index: &LineIndex) -> DocumentSymbolNode {
        let variant_case_range = line_index.source_span_range(source_text, self.span);

        DocumentSymbolNode {
            name: self.name.clone(),
            detail: Some("variant case".to_string()),
            kind: SymbolKind::ENUM_MEMBER,
            range: variant_case_range,
            selection_range: variant_case_range,
            children: self
                .fields
                .iter()
                .map(|typed_field| typed_field.document_symbol_node(source_text, line_index))
                .collect(),
        }
    }
}

trait ObjectFieldDocumentSymbolExt {
    fn document_symbol_node(&self, source_text: &str, line_index: &LineIndex) -> DocumentSymbolNode;
}

impl ObjectFieldDocumentSymbolExt for ObjectField {
    fn document_symbol_node(&self, source_text: &str, line_index: &LineIndex) -> DocumentSymbolNode {
        let object_field_range = line_index.source_span_range(source_text, self.span);

        DocumentSymbolNode {
            name: self.name.clone(),
            detail: Some("property".to_string()),
            kind: SymbolKind::PROPERTY,
            range: object_field_range,
            selection_range: object_field_range,
            children: self.value.document_symbol_nodes(source_text, line_index),
        }
    }
}

trait ExpressionDocumentSymbolExt {
    fn document_symbol_nodes(&self, source_text: &str, line_index: &LineIndex) -> Vec<DocumentSymbolNode>;
}

impl ExpressionDocumentSymbolExt for Expression {
    fn document_symbol_nodes(&self, source_text: &str, line_index: &LineIndex) -> Vec<DocumentSymbolNode> {
        match self {
            Self::ObjectLiteral(object_fields) => object_fields
                .iter()
                .map(|object_field| object_field.document_symbol_node(source_text, line_index))
                .collect(),
            Self::ArrayLiteral(array_items) => array_items
                .iter()
                .flat_map(|array_item| array_item.document_symbol_nodes(source_text, line_index))
                .collect(),
            Self::NullFallback(null_fallback_expression) => null_fallback_expression
                .value
                .document_symbol_nodes(source_text, line_index)
                .into_iter()
                .chain(null_fallback_expression.fallback.document_symbol_nodes(source_text, line_index))
                .collect(),
            Self::StringLiteral(_)
            | Self::StringTemplate(_)
            | Self::NumberLiteral(_)
            | Self::BooleanLiteral(_)
            | Self::NullLiteral
            | Self::Reference(_)
            | Self::FunctionCall(_)
            | Self::AgentContext(_)
            | Self::Asset(_)
            | Self::ToolCall(_)
            | Self::McpCall(_)
            | Self::VariantProjection(_)
            | Self::Match(_) => Vec::new(),
        }
    }
}

trait AgentDeclarationDocumentSymbolExt {
    fn document_symbol_node(&self, source_text: &str, line_index: &LineIndex) -> DocumentSymbolNode;
}

impl AgentDeclarationDocumentSymbolExt for AgentDeclaration {
    fn document_symbol_node(&self, source_text: &str, line_index: &LineIndex) -> DocumentSymbolNode {
        let declaration_range = line_index.source_span_range(source_text, self.span);
        let children = self
            .properties
            .iter()
            .filter_map(|agent_property| {
                let (property_name, property_span, child_symbols) = match agent_property {
                    AgentProperty::Dynamic(dynamic_block) => (
                        DeclarationKeyword::Dynamic.as_str().to_string(),
                        dynamic_block.span,
                        dynamic_block
                            .fields
                            .iter()
                            .map(|object_field| object_field.document_symbol_node(source_text, line_index))
                            .collect(),
                    ),
                    AgentProperty::Model(model_usage) => (
                        agent_property.name()?.to_string(),
                        model_usage.span,
                        model_usage
                            .properties
                            .iter()
                            .map(|object_field| object_field.document_symbol_node(source_text, line_index))
                            .collect(),
                    ),
                    AgentProperty::File(agent_file) => (
                        agent_property.name()?.to_string(),
                        agent_file.span,
                        agent_file
                            .fields
                            .iter()
                            .map(|object_field| object_field.document_symbol_node(source_text, line_index))
                            .collect(),
                    ),
                    AgentProperty::Output { fields, span } => (
                        agent_property.name()?.to_string(),
                        *span,
                        fields
                            .iter()
                            .map(|typed_field| typed_field.document_symbol_node(source_text, line_index))
                            .collect(),
                    ),
                    AgentProperty::Context(agent_context) => (agent_property.name()?.to_string(), agent_context.span(), Vec::new()),
                    AgentProperty::Unknown { name, span } => (name.clone(), *span, Vec::new()),
                    AgentProperty::InvalidModel(_) | AgentProperty::Instruction(_) | AgentProperty::Uses(_) => return None,
                };
                let property_range = line_index.source_span_range(source_text, property_span);

                Some(DocumentSymbolNode {
                    name: property_name,
                    detail: Some("agent property".to_string()),
                    kind: SymbolKind::PROPERTY,
                    range: property_range,
                    selection_range: property_range,
                    children: child_symbols,
                })
            })
            .collect();

        DocumentSymbolNode {
            name: self.name.clone(),
            detail: Some("agent declaration".to_string()),
            kind: SymbolKind::FUNCTION,
            range: declaration_range,
            selection_range: declaration_range,
            children,
        }
    }
}

impl SemanticIndex {
    fn fallback_document_symbols(
        &self,
        source_text: &str,
        line_index: &LineIndex,
        syntax_snapshot: &SyntaxSnapshot,
    ) -> Vec<DocumentSymbolNode> {
        let mut symbol_nodes = Vec::new();
        let mut append_named_symbols = |named_spans: &[super::semantic_index::NamedSpan], detail: &str, symbol_kind: SymbolKind| {
            symbol_nodes.extend(named_spans.iter().map(|named_span| {
                let symbol_range = line_index.source_span_range(source_text, named_span.span);

                DocumentSymbolNode {
                    name: named_span.name.clone(),
                    detail: Some(detail.to_string()),
                    kind: symbol_kind,
                    range: symbol_range,
                    selection_range: symbol_range,
                    children: Vec::new(),
                }
            }));
        };

        append_named_symbols(&self.provider_locations, "provider declaration", SymbolKind::MODULE);
        append_named_symbols(&self.model_locations, "model declaration", SymbolKind::CLASS);
        append_named_symbols(&self.mcp_server_locations, "MCP server declaration", SymbolKind::MODULE);
        append_named_symbols(&self.schema_locations, "schema declaration", SymbolKind::STRUCT);
        append_named_symbols(&self.tool_locations, "tool declaration", SymbolKind::FUNCTION);
        append_named_symbols(&self.resource_locations, "MCP resource import", SymbolKind::OBJECT);
        append_named_symbols(&self.prompt_locations, "MCP prompt import", SymbolKind::FIELD);
        append_named_symbols(&self.agent_locations, "agent declaration", SymbolKind::FUNCTION);

        for output_location in &self.output_locations {
            let output_range = line_index.source_span_range(source_text, *output_location);

            symbol_nodes.push(DocumentSymbolNode {
                name: DeclarationKeyword::Output.as_str().to_string(),
                detail: Some("output declaration".to_string()),
                kind: SymbolKind::OBJECT,
                range: output_range,
                selection_range: output_range,
                children: Vec::new(),
            });
        }

        for recovered_declaration in syntax_snapshot.recovered_declarations() {
            let Some(recovered_range) = line_index.range(
                source_text,
                recovered_declaration.byte_range.start,
                recovered_declaration.byte_range.end,
            ) else {
                continue;
            };

            if symbol_nodes
                .iter()
                .any(|symbol_node| symbol_node.range.start == recovered_range.start)
            {
                continue;
            }

            let (detail, symbol_kind) = match recovered_declaration.keyword {
                DeclarationKeyword::Provider => ("provider declaration", SymbolKind::MODULE),
                DeclarationKeyword::Model => ("model declaration", SymbolKind::CLASS),
                DeclarationKeyword::Mcp => ("MCP server declaration", SymbolKind::MODULE),
                DeclarationKeyword::Secrets => ("secrets declaration", SymbolKind::OBJECT),
                DeclarationKeyword::Input => ("input declaration", SymbolKind::OBJECT),
                DeclarationKeyword::Schema => ("schema declaration", SymbolKind::STRUCT),
                DeclarationKeyword::Tool => ("tool declaration", SymbolKind::FUNCTION),
                DeclarationKeyword::Resource => ("MCP resource import", SymbolKind::OBJECT),
                DeclarationKeyword::Prompt => ("MCP prompt import", SymbolKind::FIELD),
                DeclarationKeyword::Dynamic => ("dynamic declaration", SymbolKind::FIELD),
                DeclarationKeyword::Agent => ("agent declaration", SymbolKind::FUNCTION),
                DeclarationKeyword::Output => ("output declaration", SymbolKind::OBJECT),
            };

            symbol_nodes.push(DocumentSymbolNode {
                name: recovered_declaration
                    .name
                    .clone()
                    .unwrap_or_else(|| recovered_declaration.keyword.as_str().to_string()),
                detail: Some(detail.to_string()),
                kind: symbol_kind,
                range: recovered_range,
                selection_range: recovered_range,
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

    fn generated_output_marks(&self, source_text: &str, line_index: &LineIndex) -> Vec<CodeLensHint> {
        self.output_locations
            .iter()
            .map(|output_location| {
                let output_range = line_index.source_span_range(source_text, *output_location);

                CodeLensHint {
                    range: output_range,
                    title: "Generated output".to_string(),
                    command: "superwire.generated.output".to_string(),
                }
            })
            .collect()
    }
}
