use super::{ObjectField, SourceSpan, ToolDeclaration, ToolSource, TypedField};
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpBatchImportDeclaration {
    pub server_name: String,
    pub fixed_binding_fields: Vec<ObjectField>,
    pub input_fields: Vec<TypedField>,
    pub max_calls: Option<u64>,
    pub output_fields: Vec<TypedField>,
    pub tool_items: Vec<McpToolBatchImportItem>,
    pub resource_items: Vec<McpResourceBatchImportItem>,
    pub prompt_items: Vec<McpPromptBatchImportItem>,
    pub tools: Vec<ToolDeclaration>,
    pub resources: Vec<McpResourceImportDeclaration>,
    pub prompts: Vec<McpPromptImportDeclaration>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpToolBatchImportDeclaration {
    pub server_name: String,
    pub fixed_binding_fields: Vec<ObjectField>,
    pub input_fields: Vec<TypedField>,
    pub max_calls: Option<u64>,
    pub output_fields: Vec<TypedField>,
    pub items: Vec<McpToolBatchImportItem>,
    pub tools: Vec<ToolDeclaration>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpResourceBatchImportDeclaration {
    pub server_name: String,
    pub parameters: Vec<ObjectField>,
    pub items: Vec<McpResourceBatchImportItem>,
    pub resources: Vec<McpResourceImportDeclaration>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpResourceBatchImportItem {
    pub source_name: String,
    pub local_name: String,
    pub alias: Option<String>,
    pub parameters: Vec<ObjectField>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpPromptBatchImportDeclaration {
    pub server_name: String,
    pub parameters: Vec<ObjectField>,
    pub items: Vec<McpPromptBatchImportItem>,
    pub prompts: Vec<McpPromptImportDeclaration>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpPromptBatchImportItem {
    pub source_name: String,
    pub local_name: String,
    pub alias: Option<String>,
    pub parameters: Vec<ObjectField>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy)]
pub struct McpImportBindings<'binding> {
    shared_fields: &'binding [ObjectField],
    local_fields: &'binding [ObjectField],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpImportBindingEvaluationKind {
    ImportParameter,
    CallParameter,
    ImportBinding,
}

impl McpImportBindingEvaluationKind {
    #[must_use]
    pub fn field_context(self, import_name: &str, field_name: &str) -> String {
        match self {
            Self::ImportParameter => format!("MCP import `{import_name}` parameter `{field_name}`"),
            Self::CallParameter => format!("MCP call `{import_name}` parameter `{field_name}`"),
            Self::ImportBinding => format!("MCP import `{import_name}` binding `{field_name}`"),
        }
    }
}

impl<'binding> McpImportBindings<'binding> {
    #[must_use]
    pub fn new(shared_fields: &'binding [ObjectField], local_fields: &'binding [ObjectField]) -> Self {
        Self {
            shared_fields,
            local_fields,
        }
    }

    #[must_use]
    pub fn effective_fields(self) -> Vec<ObjectField> {
        ObjectField::merged_with_overrides(self.shared_fields, self.local_fields)
    }

    #[must_use]
    pub fn has_field(self, field_name: &str) -> bool {
        self.local_fields
            .iter()
            .chain(self.shared_fields.iter())
            .any(|binding_field| binding_field.name == field_name)
    }

    pub fn evaluate_json<Error>(
        self,
        import_name: &str,
        evaluation_kind: McpImportBindingEvaluationKind,
        mut evaluate_field: impl FnMut(&ObjectField, String) -> Result<Value, Error>,
    ) -> Result<Value, Error> {
        self.evaluate_json_with_local_kind(import_name, evaluation_kind, evaluation_kind, &mut evaluate_field)
    }

    pub fn evaluate_json_with_local_kind<Error>(
        self,
        import_name: &str,
        shared_evaluation_kind: McpImportBindingEvaluationKind,
        local_evaluation_kind: McpImportBindingEvaluationKind,
        mut evaluate_field: impl FnMut(&ObjectField, String) -> Result<Value, Error>,
    ) -> Result<Value, Error> {
        let mut evaluated_fields = Map::new();

        for (binding_field, evaluation_kind) in self.effective_fields_with_evaluation_kind(shared_evaluation_kind, local_evaluation_kind) {
            let field_context = evaluation_kind.field_context(import_name, &binding_field.name);
            let field_value = evaluate_field(&binding_field, field_context)?;
            evaluated_fields.insert(binding_field.name, field_value);
        }

        Ok(Value::Object(evaluated_fields))
    }

    fn effective_fields_with_evaluation_kind(
        self,
        shared_evaluation_kind: McpImportBindingEvaluationKind,
        local_evaluation_kind: McpImportBindingEvaluationKind,
    ) -> Vec<(ObjectField, McpImportBindingEvaluationKind)> {
        let mut merged_fields = self
            .shared_fields
            .iter()
            .cloned()
            .map(|binding_field| (binding_field, shared_evaluation_kind))
            .collect::<Vec<_>>();

        for local_field in self.local_fields {
            if let Some(existing_field_index) = merged_fields
                .iter()
                .position(|field_with_evaluation_kind| field_with_evaluation_kind.0.name == local_field.name)
            {
                merged_fields[existing_field_index] = (local_field.clone(), local_evaluation_kind);

                continue;
            }

            merged_fields.push((local_field.clone(), local_evaluation_kind));
        }

        merged_fields
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpToolBatchImportItem {
    pub source_name: String,
    pub local_name: String,
    pub alias: Option<String>,
    pub input_fields: Vec<TypedField>,
    pub max_calls: Option<u64>,
    pub fixed_binding_fields: Vec<ObjectField>,
    pub output_fields: Vec<TypedField>,
    pub span: SourceSpan,
}

impl McpToolBatchImportItem {
    #[must_use]
    pub fn new(
        source_name: String,
        local_name: Option<String>,
        input_fields: Vec<TypedField>,
        max_calls: Option<u64>,
        fixed_binding_fields: Vec<ObjectField>,
        output_fields: Vec<TypedField>,
        span: SourceSpan,
    ) -> Self {
        let alias = local_name;
        let local_name = alias.clone().unwrap_or_else(|| source_name.replace('-', "_"));

        Self {
            source_name,
            local_name,
            alias,
            input_fields,
            max_calls,
            fixed_binding_fields,
            output_fields,
            span,
        }
    }

    #[must_use]
    pub fn to_tool_declaration(
        &self,
        server_name: &str,
        input_fields: &[TypedField],
        fixed_binding_fields: &[ObjectField],
        max_calls: Option<u64>,
        output_fields: &[TypedField],
    ) -> ToolDeclaration {
        let fixed_binding_fields = McpImportBindings::new(fixed_binding_fields, &self.fixed_binding_fields).effective_fields();
        let input_fields = if self.input_fields.is_empty() {
            input_fields.to_vec()
        } else {
            self.input_fields.clone()
        };
        let output_fields = if self.output_fields.is_empty() {
            output_fields.to_vec()
        } else {
            self.output_fields.clone()
        };

        ToolDeclaration {
            name: self.local_name.clone(),
            description: None,
            max_calls: self.max_calls.or(max_calls),
            source: Some(ToolSource::Mcp(McpToolSource {
                server_name: Some(server_name.to_string()),
                tool_name: self.source_name.clone(),
                span: self.span,
            })),
            imported: true,
            input_fields,
            binding_fields: Vec::new(),
            fixed_binding_fields,
            output_fields,
            span: self.span,
        }
    }
}

impl McpResourceBatchImportItem {
    #[must_use]
    pub fn new(source_name: String, local_name: Option<String>, parameters: Vec<ObjectField>, span: SourceSpan) -> Self {
        let alias = local_name;
        let local_name = alias.clone().unwrap_or_else(|| source_name.replace('-', "_"));

        Self {
            source_name,
            local_name,
            alias,
            parameters,
            span,
        }
    }

    #[must_use]
    pub fn to_resource_import_declaration(&self, server_name: &str, shared_parameters: &[ObjectField]) -> McpResourceImportDeclaration {
        let parameters = McpImportBindings::new(shared_parameters, &self.parameters).effective_fields();

        McpResourceImportDeclaration {
            name: self.local_name.clone(),
            source: McpImportSource {
                server_name: server_name.to_string(),
                kind: McpImportKind::Resource,
                item_name: self.source_name.clone(),
                span: self.span,
            },
            parameters,
            span: self.span,
        }
    }
}

impl McpPromptBatchImportItem {
    #[must_use]
    pub fn new(source_name: String, local_name: Option<String>, parameters: Vec<ObjectField>, span: SourceSpan) -> Self {
        let alias = local_name;
        let local_name = alias.clone().unwrap_or_else(|| source_name.replace('-', "_"));

        Self {
            source_name,
            local_name,
            alias,
            parameters,
            span,
        }
    }

    #[must_use]
    pub fn to_prompt_import_declaration(&self, server_name: &str, shared_parameters: &[ObjectField]) -> McpPromptImportDeclaration {
        let parameters = McpImportBindings::new(shared_parameters, &self.parameters).effective_fields();

        McpPromptImportDeclaration {
            name: self.local_name.clone(),
            source: McpImportSource {
                server_name: server_name.to_string(),
                kind: McpImportKind::Prompt,
                item_name: self.source_name.clone(),
                span: self.span,
            },
            parameters,
            span: self.span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpToolSource {
    pub server_name: Option<String>,
    pub tool_name: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpResourceImportDeclaration {
    pub name: String,
    pub source: McpImportSource,
    pub parameters: Vec<ObjectField>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpPromptImportDeclaration {
    pub name: String,
    pub source: McpImportSource,
    pub parameters: Vec<ObjectField>,
    pub span: SourceSpan,
}

impl McpPromptImportDeclaration {
    #[must_use]
    pub fn has_parameter_binding(&self, parameter_name: &str) -> bool {
        McpImportBindings::new(&[], &self.parameters).has_field(parameter_name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpImportSource {
    pub server_name: String,
    pub kind: McpImportKind,
    pub item_name: String,
    pub span: SourceSpan,
}

impl McpImportSource {
    #[must_use]
    pub fn inferred_local_name(&self) -> String {
        self.item_name.replace('-', "_")
    }

    #[must_use]
    pub fn wire_item_name(&self) -> String {
        match self.kind {
            McpImportKind::Tool => self.item_name.replace('-', "_"),
            McpImportKind::Resource | McpImportKind::Prompt => self.item_name.clone(),
        }
    }

    #[must_use]
    pub fn render_path(&self) -> String {
        format!("mcp.{}.{}.{}", self.server_name, self.kind.as_str(), self.wire_item_name())
    }

    #[must_use]
    pub fn as_tool_source(&self) -> McpToolSource {
        McpToolSource {
            server_name: Some(self.server_name.clone()),
            tool_name: self.item_name.clone(),
            span: self.span,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum McpImportKind {
    Tool,
    Resource,
    Prompt,
}

impl McpImportKind {
    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "tool" => Some(Self::Tool),
            "resource" => Some(Self::Resource),
            "prompt" => Some(Self::Prompt),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tool => "tool",
            Self::Resource => "resource",
            Self::Prompt => "prompt",
        }
    }

    #[must_use]
    pub fn wire_tool_name_is_snake_case(tool_name: &str) -> bool {
        let mut characters = tool_name.chars();

        let Some(first_character) = characters.next() else {
            return false;
        };

        if !(first_character.is_ascii_lowercase() || first_character == '_') {
            return false;
        }

        characters.all(|character| character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_')
    }

    #[must_use]
    pub fn wire_item_name_is_snake_case(self, item_name: &str) -> bool {
        let _ = self;

        Self::wire_tool_name_is_snake_case(item_name)
    }

    #[must_use]
    pub fn normalize_tool_name_from_wire(self, wire_name: &str) -> String {
        wire_name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{McpImportBindingEvaluationKind, McpImportBindings};
    use crate::dsl::{Expression, ObjectField, SourcePosition, SourceSpan};
    use serde_json::{json, Value};

    #[test]
    fn merges_mcp_import_bindings_with_local_overrides_in_order() {
        let shared_fields = vec![
            ObjectField {
                name: "project_id".to_string(),
                value: Expression::NumberLiteral("1".to_string()),
                span: test_source_span(),
            },
            ObjectField {
                name: "type".to_string(),
                value: Expression::StringLiteral("shared".to_string()),
                span: test_source_span(),
            },
        ];
        let local_fields = vec![
            ObjectField {
                name: "type".to_string(),
                value: Expression::StringLiteral("local".to_string()),
                span: test_source_span(),
            },
            ObjectField {
                name: "task_id".to_string(),
                value: Expression::NumberLiteral("42".to_string()),
                span: test_source_span(),
            },
        ];

        let effective_fields = McpImportBindings::new(&shared_fields, &local_fields).effective_fields();
        let effective_field_names = effective_fields
            .iter()
            .map(|effective_field| effective_field.name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(effective_field_names, vec!["project_id", "type", "task_id"]);
        assert_eq!(effective_fields[0].value, Expression::NumberLiteral("1".to_string()));
        assert_eq!(effective_fields[1].value, Expression::StringLiteral("local".to_string()));
        assert_eq!(effective_fields[2].value, Expression::NumberLiteral("42".to_string()));
    }

    #[test]
    fn evaluates_mcp_import_bindings_with_source_specific_contexts() {
        let shared_fields = vec![
            ObjectField {
                name: "project_id".to_string(),
                value: Expression::NumberLiteral("1".to_string()),
                span: test_source_span(),
            },
            ObjectField {
                name: "type".to_string(),
                value: Expression::StringLiteral("shared".to_string()),
                span: test_source_span(),
            },
        ];
        let local_fields = vec![
            ObjectField {
                name: "type".to_string(),
                value: Expression::StringLiteral("local".to_string()),
                span: test_source_span(),
            },
            ObjectField {
                name: "task_id".to_string(),
                value: Expression::NumberLiteral("42".to_string()),
                span: test_source_span(),
            },
        ];
        let mut field_contexts = Vec::new();

        let evaluated_bindings = McpImportBindings::new(&shared_fields, &local_fields)
            .evaluate_json_with_local_kind(
                "summary_prompt",
                McpImportBindingEvaluationKind::ImportParameter,
                McpImportBindingEvaluationKind::ImportBinding,
                |binding_field, field_context| {
                    field_contexts.push(field_context);

                    Ok::<Value, ()>(match &binding_field.value {
                        Expression::NumberLiteral(number_literal) => {
                            Value::Number(number_literal.parse().expect("test number literal should parse"))
                        }
                        Expression::StringLiteral(string_literal) => Value::String(string_literal.clone()),
                        _ => Value::Null,
                    })
                },
            )
            .expect("MCP import bindings should evaluate");

        assert_eq!(
            evaluated_bindings,
            json!({
                "project_id": 1,
                "type": "local",
                "task_id": 42
            })
        );
        assert_eq!(
            field_contexts,
            vec![
                "MCP import `summary_prompt` parameter `project_id`",
                "MCP import `summary_prompt` binding `type`",
                "MCP import `summary_prompt` binding `task_id`",
            ]
        );
    }

    fn test_source_span() -> SourceSpan {
        SourceSpan {
            start: SourcePosition { line: 1, column: 1 },
            end: SourcePosition { line: 1, column: 1 },
        }
    }
}
