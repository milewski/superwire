use crate::schema::to_json_value;
use crate::{HttpMcpClientFactory, McpClientFactory, McpClientRequestScope, McpError, McpServerConfig};
use rust_mcp_schema::{ToolInputSchema, ToolOutputSchema};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use superwire_semantic::support::expression::EvaluationContext;
use superwire_types::ast::{Declaration, ToolSource, Workflow};

mod apply;
mod name_resolution;
mod project;
mod validate;

pub use name_resolution::McpServerToolLookup;

pub const PROJECT_MCP_LOCK_FILE_NAME: &str = "superwire.lock";

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct McpLock {
    pub servers: BTreeMap<String, McpServerLock>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct McpLockResolutionContext {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub input: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub secrets: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dynamic: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub agent_outputs: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub agent_contexts: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ProjectMcpLock {
    pub version: u32,
    pub workflows: BTreeMap<String, ProjectWorkflowMcpLockEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ProjectWorkflowMcpLockEntry {
    #[serde(default)]
    pub hash: String,
    #[serde(flatten)]
    pub lock: McpLock,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct McpServerLock {
    pub tools: BTreeMap<String, McpToolLock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prompts: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub prompt_arguments: BTreeMap<String, Vec<McpPromptArgumentLock>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolLock {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: ToolInputSchema,
    pub output_schema: Option<ToolOutputSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpPromptArgumentLock {
    pub name: String,
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl McpToolLock {
    #[must_use]
    pub fn from_json_schema_values(
        name: String,
        description: Option<String>,
        input_schema: Value,
        output_schema: Option<Value>,
    ) -> Option<Self> {
        let input_schema = serde_json::from_value(input_schema).ok()?;
        let output_schema = output_schema.map(serde_json::from_value).transpose().ok()?;

        Some(Self {
            name,
            description,
            input_schema,
            output_schema,
        })
    }

    fn serialized_schema_matches<SchemaValue: Serialize>(left: &SchemaValue, right: &SchemaValue) -> bool {
        match (to_json_value(left), to_json_value(right)) {
            (Ok(left_value), Ok(right_value)) => left_value == right_value,
            (Ok(_) | Err(_), Err(_)) | (Err(_), Ok(_)) => false,
        }
    }
}

impl PartialEq for McpToolLock {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.description == other.description
            && Self::serialized_schema_matches(&self.input_schema, &other.input_schema)
            && match (&self.output_schema, &other.output_schema) {
                (Some(left_schema), Some(right_schema)) => Self::serialized_schema_matches(left_schema, right_schema),
                (None, None) => true,
                (Some(_), None) | (None, Some(_)) => false,
            }
    }
}

impl McpLock {
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn discover_from_workflow(workflow: &Workflow) -> Result<Self, McpError> {
        Self::discover_from_workflow_with_client_factory(workflow, &HttpMcpClientFactory)
    }

    pub fn discover_from_workflow_with_client_factory(
        workflow: &Workflow,
        client_factory: &dyn McpClientFactory,
    ) -> Result<Self, McpError> {
        let mut lock = Self::empty();

        for server_config in McpServerConfig::from_workflow(workflow)? {
            log::debug!("discovering MCP tools from literal server config: {}", server_config.name);
            let server_lock = client_factory.client_for_config(server_config.clone())?.list_tools()?;
            lock.servers.insert(server_config.name, server_lock);
        }

        Ok(lock)
    }

    pub fn discover_from_workflow_with_lock_context(
        workflow: &Workflow,
        lock_context: Option<&McpLockResolutionContext>,
    ) -> Result<Self, McpError> {
        Self::discover_from_workflow_with_lock_context_and_client_factory(workflow, lock_context, &HttpMcpClientFactory)
    }

    pub fn discover_from_workflow_with_lock_context_and_client_factory(
        workflow: &Workflow,
        lock_context: Option<&McpLockResolutionContext>,
        client_factory: &dyn McpClientFactory,
    ) -> Result<Self, McpError> {
        let Some(lock_context) = lock_context else {
            return Self::discover_from_workflow_with_client_factory(workflow, client_factory);
        };

        let evaluation_context = lock_context.to_evaluation_context();

        Self::discover_from_workflow_with_context_and_client_factory(workflow, &evaluation_context, client_factory)
    }

    pub fn discover_from_workflow_with_context(workflow: &Workflow, evaluation_context: &EvaluationContext) -> Result<Self, McpError> {
        Self::discover_from_workflow_with_context_and_client_factory(workflow, evaluation_context, &HttpMcpClientFactory)
    }

    pub fn discover_from_workflow_with_context_and_client_factory(
        workflow: &Workflow,
        evaluation_context: &EvaluationContext,
        client_factory: &dyn McpClientFactory,
    ) -> Result<Self, McpError> {
        let mut lock = Self::empty();
        let mut evaluation_context = evaluation_context.clone();
        evaluation_context.evaluate_available_workflow_dynamic_bindings(workflow);
        let request_scope = McpClientRequestScope::from_workflow(client_factory, workflow, &evaluation_context)?;

        for declaration in workflow.declarations() {
            let Declaration::McpServer(mcp_server_declaration) = declaration else {
                continue;
            };
            let server_config = McpServerConfig::resolve_from_declaration_with_endpoint_validator(
                mcp_server_declaration,
                &evaluation_context,
                |server_name, endpoint| request_scope.validate_endpoint(server_name, endpoint),
            )?;
            log::debug!("discovering MCP tools from runtime server config: {}", server_config.name);
            let server_lock = request_scope.client_for_config(server_config.clone())?.list_tools()?;

            lock.servers.insert(server_config.name, server_lock);
        }

        Ok(lock)
    }

    pub fn read_from_path(lock_path: &Path) -> Result<Self, McpError> {
        let lock_text = std::fs::read_to_string(lock_path).map_err(|source| McpError::ReadLock {
            path: lock_path.display().to_string(),
            source,
        })?;

        serde_json::from_str(&lock_text).map_err(|source| McpError::ParseLock {
            path: lock_path.display().to_string(),
            source,
        })
    }

    pub fn write_to_path(&self, lock_path: &Path) -> Result<(), McpError> {
        std::fs::write(lock_path, self.file_text(lock_path)?).map_err(|source| McpError::WriteLock {
            path: lock_path.display().to_string(),
            source,
        })
    }

    pub fn file_text(&self, lock_path: &Path) -> Result<String, McpError> {
        let lock_text = serde_json::to_string_pretty(self).map_err(|source| McpError::SerializeLock {
            path: lock_path.display().to_string(),
            source,
        })?;

        Ok(format!("{lock_text}\n"))
    }

    #[must_use]
    pub fn find_tool(&self, source: &ToolSource) -> Option<&McpToolLock> {
        self.find_tool_with_name(source).map(|(_tool_name, mcp_tool_lock)| mcp_tool_lock)
    }

    #[must_use]
    pub fn find_tool_with_name(&self, source: &ToolSource) -> Option<(String, &McpToolLock)> {
        let ToolSource::Mcp(mcp_tool_source) = source;

        if let Some(server_name) = &mcp_tool_source.server_name {
            let server_lock = self.servers.get(server_name)?;

            return server_lock.find_tool_with_name(&mcp_tool_source.tool_name);
        }

        for server_lock in self.servers.values() {
            if let Some((tool_name, mcp_tool_lock)) = server_lock.find_tool_with_name(&mcp_tool_source.tool_name) {
                return Some((tool_name, mcp_tool_lock));
            }
        }

        None
    }
}

impl McpLockResolutionContext {
    #[must_use]
    pub fn to_evaluation_context(&self) -> EvaluationContext {
        EvaluationContext {
            input_values: self.input.clone().into_iter().collect(),
            secret_values: self.secrets.clone().into_iter().collect(),
            agent_outputs: self.agent_outputs.clone().into_iter().collect(),
            agent_contexts: self.agent_contexts.clone().into_iter().collect(),
            local_bindings: self.dynamic.clone().into_iter().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{McpLock, McpPromptArgumentLock, McpServerLock, McpToolLock};
    use serde_json::json;
    use std::collections::BTreeMap;
    use superwire_types::ast::{
        Declaration, Expression, McpBatchImportDeclaration, McpImportKind, McpImportSource, McpPromptBatchImportItem,
        McpPromptImportDeclaration, McpResourceImportDeclaration, McpToolSource, ObjectField, SourceSpan, ToolDeclaration, ToolSource,
        Workflow,
    };

    #[test]
    fn normalizes_mcp_item_names_for_lookup() {
        struct NormalizationCase {
            item_name: &'static str,
            normalized_name: &'static str,
        }

        let normalization_cases = [
            NormalizationCase {
                item_name: "dynamic-summary-prompt",
                normalized_name: "dynamic_summary_prompt",
            },
            NormalizationCase {
                item_name: "DynamicSummaryPrompt",
                normalized_name: "dynamic_summary_prompt",
            },
            NormalizationCase {
                item_name: "dynamic summary prompt",
                normalized_name: "dynamic_summary_prompt",
            },
            NormalizationCase {
                item_name: "dynamic__summary---prompt",
                normalized_name: "dynamic_summary_prompt",
            },
            NormalizationCase {
                item_name: "FetchTaskData2",
                normalized_name: "fetch_task_data2",
            },
            NormalizationCase {
                item_name: "  _task  ",
                normalized_name: "task",
            },
        ];

        for normalization_case in normalization_cases {
            assert_eq!(
                McpServerLock::normalize_item_name(normalization_case.item_name),
                normalization_case.normalized_name,
                "normalization failed for {}",
                normalization_case.item_name
            );
        }
    }

    #[test]
    fn applies_lock_name_resolution_to_workflow_imports() {
        let mut workflow = workflow_with_declarations(vec![
            Declaration::Tool(tool_import_declaration("fetch_task_data", "fetch_task_data")),
            Declaration::McpResource(resource_import_declaration("project_readme", "project_readme")),
            Declaration::McpPrompt(prompt_import_declaration(
                "summarize_task_prompt",
                "summarize_task_prompt",
                Vec::new(),
            )),
        ]);
        let mcp_lock = import_resolution_lock();

        mcp_lock.apply_to_workflow(&mut workflow);

        let declarations = workflow.declarations();
        let Declaration::Tool(tool_declaration) = &declarations[0] else {
            panic!("first declaration should be a tool import");
        };
        let Declaration::McpResource(resource_declaration) = &declarations[1] else {
            panic!("second declaration should be a resource import");
        };
        let Declaration::McpPrompt(prompt_declaration) = &declarations[2] else {
            panic!("third declaration should be a prompt import");
        };

        assert_eq!(
            tool_declaration.source.as_ref().and_then(ToolSource::mcp_tool_name),
            Some("FetchTaskData")
        );
        assert_eq!(resource_declaration.source.item_name, "project-readme");
        assert_eq!(prompt_declaration.source.item_name, "summarize-task-prompt");
    }

    #[test]
    fn cached_tool_lookup_preserves_exact_before_normalized_resolution() {
        let mut tools = BTreeMap::new();
        let mut normalized_tool_lock = fetch_task_data_tool_lock();
        let mut exact_tool_lock = fetch_task_data_tool_lock();
        normalized_tool_lock.name = "FetchTaskData".to_string();
        exact_tool_lock.name = "fetch_task_data".to_string();
        tools.insert("FetchTaskData".to_string(), normalized_tool_lock);
        tools.insert("fetch_task_data".to_string(), exact_tool_lock);

        let server_lock = McpServerLock {
            tools,
            ..McpServerLock::default()
        };
        let tool_lookup = server_lock.tool_lookup();

        let Some((exact_tool_name, _mcp_tool_lock)) = tool_lookup.find_tool_with_name(&server_lock, "fetch_task_data") else {
            panic!("exact tool lookup should resolve");
        };
        let Some((normalized_tool_name, _mcp_tool_lock)) = tool_lookup.find_tool_with_name(&server_lock, "fetch task data") else {
            panic!("normalized tool lookup should resolve");
        };

        assert_eq!(exact_tool_name, "fetch_task_data");
        assert_eq!(normalized_tool_name, "FetchTaskData");
    }

    #[test]
    fn unscoped_tool_lookup_preserves_server_order_before_later_exact_matches() {
        let mut first_server_tools = BTreeMap::new();
        let mut second_server_tools = BTreeMap::new();
        let mut normalized_tool_lock = fetch_task_data_tool_lock();
        let mut exact_tool_lock = fetch_task_data_tool_lock();
        normalized_tool_lock.name = "FetchTaskData".to_string();
        exact_tool_lock.name = "fetch_task_data".to_string();
        first_server_tools.insert("FetchTaskData".to_string(), normalized_tool_lock);
        second_server_tools.insert("fetch_task_data".to_string(), exact_tool_lock);

        let mcp_lock = McpLock {
            servers: BTreeMap::from([
                (
                    "alpha".to_string(),
                    McpServerLock {
                        tools: first_server_tools,
                        ..McpServerLock::default()
                    },
                ),
                (
                    "beta".to_string(),
                    McpServerLock {
                        tools: second_server_tools,
                        ..McpServerLock::default()
                    },
                ),
            ]),
        };
        let tool_source = ToolSource::Mcp(McpToolSource {
            server_name: None,
            tool_name: "fetch_task_data".to_string(),
            span: SourceSpan::generated(),
        });

        let Some((resolved_tool_name, _mcp_tool_lock)) = mcp_lock.find_tool_with_name(&tool_source) else {
            panic!("unscoped tool lookup should resolve");
        };

        assert_eq!(resolved_tool_name, "FetchTaskData");
    }

    #[test]
    fn applies_lock_schema_to_imported_tool() {
        let mut workflow = workflow_with_declarations(vec![Declaration::Tool(tool_import_declaration(
            "fetch_task_data",
            "fetch_task_data",
        ))]);
        let mcp_lock = import_resolution_lock();

        mcp_lock.apply_to_workflow(&mut workflow);

        let Declaration::Tool(tool_declaration) = &workflow.declarations()[0] else {
            panic!("declaration should be a tool import");
        };

        assert_eq!(tool_declaration.description.as_deref(), Some("Fetch task data"));
        assert_eq!(tool_declaration.input_fields[0].name, "task_id");
        assert_eq!(tool_declaration.output_fields[0].name, "title");
    }

    #[test]
    fn retains_discovered_schema_for_fixed_binding_validation() {
        let mut tool_declaration = tool_import_declaration("fetch_task_data", "fetch_task_data");
        tool_declaration.fixed_binding_fields.push(ObjectField {
            name: "task_id".to_string(),
            value: Expression::NumberLiteral("42".to_string()),
            span: SourceSpan::generated(),
        });
        let mut workflow = workflow_with_declarations(vec![Declaration::Tool(tool_declaration)]);
        let mcp_lock = import_resolution_lock();

        mcp_lock.apply_to_workflow(&mut workflow);

        let Declaration::Tool(tool_declaration) = &workflow.declarations()[0] else {
            panic!("declaration should be a tool import");
        };
        let mcp_schema = tool_declaration
            .mcp_schema
            .as_ref()
            .expect("discovered MCP schema should be retained");

        assert!(tool_declaration.input_fields.is_empty());
        assert_eq!(mcp_schema.input.pointer("/properties/task_id/type"), Some(&json!("number")));
        assert!(mcp_schema.uses_discovered_input);
        assert!(mcp_schema.uses_discovered_output);
    }

    #[test]
    fn ignores_prompt_binding_validation_for_missing_server_lock() {
        let workflow = workflow_with_declarations(vec![Declaration::McpPrompt(prompt_import_declaration(
            "summarize_task_prompt",
            "summarize_task_prompt",
            Vec::new(),
        ))]);
        let mcp_lock = McpLock::empty();
        let binding_messages = mcp_lock.validate_prompt_import_bindings(&workflow);

        assert!(binding_messages.is_empty());
    }

    #[test]
    fn validates_mixed_prompt_batch_with_shared_and_item_bindings() {
        let workflow = mixed_prompt_batch_workflow(vec!["project_id", "task_id"], vec!["type"]);
        let mcp_lock = prompt_argument_lock();
        let binding_messages = mcp_lock.validate_prompt_import_bindings(&workflow);

        assert!(binding_messages.is_empty());
    }

    #[test]
    fn rejects_mixed_prompt_batch_when_item_binding_is_missing() {
        let workflow = mixed_prompt_batch_workflow(vec!["project_id", "task_id"], Vec::new());
        let mcp_lock = prompt_argument_lock();
        let binding_messages = mcp_lock.validate_prompt_import_bindings(&workflow);

        assert_eq!(
            binding_messages,
            vec!["MCP prompt `dynamic_summary_prompt` requires binding `type` from server prompt `dynamic_summary_prompt`"]
        );
    }

    fn workflow_with_declarations(declarations: Vec<Declaration>) -> Workflow {
        Workflow {
            declarations,
            source_text: None,
        }
    }

    fn tool_import_declaration(local_name: &str, tool_name: &str) -> ToolDeclaration {
        ToolDeclaration {
            name: local_name.to_string(),
            description: None,
            max_calls: None,
            source: Some(ToolSource::Mcp(McpToolSource {
                server_name: Some("local".to_string()),
                tool_name: tool_name.to_string(),
                span: SourceSpan::generated(),
            })),
            imported: true,
            input_fields: Vec::new(),
            binding_fields: Vec::new(),
            fixed_binding_fields: Vec::new(),
            output_fields: Vec::new(),
            mcp_schema: None,
            schema_issues: Vec::new(),
            span: SourceSpan::generated(),
        }
    }

    fn resource_import_declaration(local_name: &str, resource_name: &str) -> McpResourceImportDeclaration {
        McpResourceImportDeclaration {
            name: local_name.to_string(),
            source: mcp_import_source(McpImportKind::Resource, resource_name),
            parameters: Vec::new(),
            span: SourceSpan::generated(),
        }
    }

    fn prompt_import_declaration(local_name: &str, prompt_name: &str, parameters: Vec<ObjectField>) -> McpPromptImportDeclaration {
        McpPromptImportDeclaration {
            name: local_name.to_string(),
            source: mcp_import_source(McpImportKind::Prompt, prompt_name),
            parameters,
            span: SourceSpan::generated(),
        }
    }

    fn mcp_import_source(kind: McpImportKind, item_name: &str) -> McpImportSource {
        McpImportSource {
            server_name: "local".to_string(),
            kind,
            item_name: item_name.to_string(),
            span: SourceSpan::generated(),
        }
    }

    fn mixed_prompt_batch_workflow(shared_binding_names: Vec<&str>, prompt_binding_names: Vec<&str>) -> Workflow {
        workflow_with_declarations(vec![Declaration::McpBatch(McpBatchImportDeclaration {
            server_name: "local".to_string(),
            fixed_binding_fields: binding_fields(shared_binding_names),
            input_fields: Vec::new(),
            max_calls: None,
            output_fields: Vec::new(),
            tool_items: Vec::new(),
            resource_items: Vec::new(),
            prompt_items: vec![McpPromptBatchImportItem::new(
                "dynamic_summary_prompt".to_string(),
                None,
                binding_fields(prompt_binding_names),
                SourceSpan::generated(),
            )],
            tools: Vec::new(),
            resources: Vec::new(),
            prompts: Vec::new(),
            span: SourceSpan::generated(),
        })])
    }

    fn binding_fields(binding_names: Vec<&str>) -> Vec<ObjectField> {
        binding_names
            .into_iter()
            .map(|binding_name| ObjectField {
                name: binding_name.to_string(),
                value: Expression::StringLiteral("value".to_string()),
                span: SourceSpan::generated(),
            })
            .collect()
    }

    fn prompt_argument_lock() -> McpLock {
        let mut prompt_arguments = BTreeMap::new();
        prompt_arguments.insert(
            "dynamic-summary-prompt".to_string(),
            vec![
                McpPromptArgumentLock {
                    name: "project_id".to_string(),
                    required: true,
                    description: None,
                },
                McpPromptArgumentLock {
                    name: "task_id".to_string(),
                    required: true,
                    description: None,
                },
                McpPromptArgumentLock {
                    name: "type".to_string(),
                    required: true,
                    description: None,
                },
            ],
        );

        let mut servers = BTreeMap::new();
        servers.insert(
            "local".to_string(),
            McpServerLock {
                prompts: vec!["dynamic-summary-prompt".to_string()],
                prompt_arguments,
                ..McpServerLock::default()
            },
        );

        McpLock { servers }
    }

    fn import_resolution_lock() -> McpLock {
        let mut tools = BTreeMap::new();
        tools.insert("FetchTaskData".to_string(), fetch_task_data_tool_lock());

        let mut servers = BTreeMap::new();
        servers.insert(
            "local".to_string(),
            McpServerLock {
                tools,
                resources: vec!["project-readme".to_string()],
                prompts: vec!["summarize-task-prompt".to_string()],
                ..McpServerLock::default()
            },
        );

        McpLock { servers }
    }

    fn fetch_task_data_tool_lock() -> McpToolLock {
        McpToolLock::from_json_schema_values(
            "FetchTaskData".to_string(),
            Some("Fetch task data".to_string()),
            json!({
                "type": "object",
                "properties": {
                    "task_id": {
                        "type": "number"
                    }
                },
                "required": ["task_id"],
                "additionalProperties": false
            }),
            Some(json!({
                "type": "object",
                "properties": {
                    "title": {
                        "type": "string"
                    }
                },
                "required": ["title"],
                "additionalProperties": false
            })),
        )
        .expect("MCP tool lock should deserialize from JSON schema values")
    }
}
