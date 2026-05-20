use crate::dsl::{Declaration, McpPromptImportDeclaration, McpResourceImportDeclaration, ToolDeclaration, ToolSource, Workflow};
use crate::mcp::schema::to_json_value;
use crate::mcp::{McpClient, McpError, McpServerConfig};
use crate::semantic::support::expression::EvaluationContext;
use rust_mcp_schema::{ToolInputSchema, ToolOutputSchema};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

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
        let output_schema = output_schema.and_then(|output_schema| serde_json::from_value(output_schema).ok());

        Some(Self {
            name,
            description,
            input_schema,
            output_schema,
        })
    }
}

impl PartialEq for McpToolLock {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.description == other.description
            && to_json_value(&self.input_schema) == to_json_value(&other.input_schema)
            && self.output_schema.as_ref().map(to_json_value) == other.output_schema.as_ref().map(to_json_value)
    }
}

impl McpLock {
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn discover_from_workflow(workflow: &Workflow) -> Result<Self, McpError> {
        let mut lock = Self::empty();

        for server_config in McpServerConfig::from_workflow(workflow)? {
            log::debug!("discovering MCP tools from literal server config: {}", server_config.name);
            let server_lock = McpClient::new(server_config.clone()).list_tools()?;
            lock.servers.insert(server_config.name, server_lock);
        }

        Ok(lock)
    }

    pub fn discover_from_workflow_with_lock_context(
        workflow: &Workflow,
        lock_context: Option<&McpLockResolutionContext>,
    ) -> Result<Self, McpError> {
        let Some(lock_context) = lock_context else {
            return Self::discover_from_workflow(workflow);
        };

        let evaluation_context = lock_context.to_evaluation_context();

        Self::discover_from_workflow_with_context(workflow, &evaluation_context)
    }

    pub fn discover_from_workflow_with_context(workflow: &Workflow, evaluation_context: &EvaluationContext) -> Result<Self, McpError> {
        let mut lock = Self::empty();

        for declaration in workflow.declarations() {
            let Declaration::McpServer(mcp_server_declaration) = declaration else {
                continue;
            };
            let server_config = McpServerConfig::resolve_from_declaration(mcp_server_declaration, evaluation_context)?;
            log::debug!("discovering MCP tools from runtime server config: {}", server_config.name);
            let server_lock = McpClient::new(server_config.clone()).list_tools()?;

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
        let lock_text = serde_json::to_string_pretty(self).map_err(|source| McpError::SerializeLock {
            path: lock_path.display().to_string(),
            source,
        })?;

        std::fs::write(lock_path, format!("{lock_text}\n")).map_err(|source| McpError::WriteLock {
            path: lock_path.display().to_string(),
            source,
        })
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

    pub fn apply_to_workflow(&self, workflow: &mut Workflow) {
        for declaration in &mut workflow.declarations {
            match declaration {
                Declaration::Tool(tool_declaration) => {
                    self.apply_to_tool_declaration(tool_declaration);
                }
                Declaration::McpToolBatch(tool_batch_import_declaration) => {
                    for tool_declaration in &mut tool_batch_import_declaration.tools {
                        self.apply_to_tool_declaration(tool_declaration);
                    }
                }
                Declaration::McpBatch(batch_import_declaration) => {
                    for tool_declaration in &mut batch_import_declaration.tools {
                        self.apply_to_tool_declaration(tool_declaration);
                    }

                    for resource_import_declaration in &mut batch_import_declaration.resources {
                        self.apply_to_resource_import_declaration(resource_import_declaration);
                    }

                    for prompt_import_declaration in &mut batch_import_declaration.prompts {
                        self.apply_to_prompt_import_declaration(prompt_import_declaration);
                    }
                }
                Declaration::McpResourceBatch(resource_batch_import_declaration) => {
                    for resource_import_declaration in &mut resource_batch_import_declaration.resources {
                        self.apply_to_resource_import_declaration(resource_import_declaration);
                    }
                }
                Declaration::McpPromptBatch(prompt_batch_import_declaration) => {
                    for prompt_import_declaration in &mut prompt_batch_import_declaration.prompts {
                        self.apply_to_prompt_import_declaration(prompt_import_declaration);
                    }
                }
                Declaration::McpResource(resource_import_declaration) => {
                    self.apply_to_resource_import_declaration(resource_import_declaration);
                }
                Declaration::McpPrompt(prompt_import_declaration) => {
                    self.apply_to_prompt_import_declaration(prompt_import_declaration);
                }
                Declaration::Provider(_)
                | Declaration::Model(_)
                | Declaration::McpServer(_)
                | Declaration::Secrets(_)
                | Declaration::Input(_)
                | Declaration::Schema(_)
                | Declaration::Dynamic(_)
                | Declaration::Agent(_)
                | Declaration::Output(_) => {}
            }
        }
    }

    #[must_use]
    pub fn validate_prompt_import_bindings(&self, workflow: &Workflow) -> Vec<String> {
        let mut messages = Vec::new();

        for declaration in workflow.declarations() {
            match declaration {
                Declaration::McpPrompt(prompt_import_declaration) => {
                    messages.extend(self.prompt_import_binding_messages(prompt_import_declaration));
                }
                Declaration::McpPromptBatch(prompt_batch_import_declaration) => {
                    for prompt_item in &prompt_batch_import_declaration.items {
                        let prompt_import_declaration = prompt_item.to_prompt_import_declaration(
                            &prompt_batch_import_declaration.server_name,
                            &prompt_batch_import_declaration.parameters,
                        );

                        messages.extend(self.prompt_import_binding_messages(&prompt_import_declaration));
                    }
                }
                Declaration::McpBatch(batch_import_declaration) => {
                    for prompt_item in &batch_import_declaration.prompt_items {
                        let prompt_import_declaration = prompt_item.to_prompt_import_declaration(
                            &batch_import_declaration.server_name,
                            &batch_import_declaration.fixed_binding_fields,
                        );

                        messages.extend(self.prompt_import_binding_messages(&prompt_import_declaration));
                    }
                }
                Declaration::McpToolBatch(_)
                | Declaration::McpResourceBatch(_)
                | Declaration::McpResource(_)
                | Declaration::Tool(_)
                | Declaration::Provider(_)
                | Declaration::Model(_)
                | Declaration::McpServer(_)
                | Declaration::Secrets(_)
                | Declaration::Input(_)
                | Declaration::Schema(_)
                | Declaration::Dynamic(_)
                | Declaration::Agent(_)
                | Declaration::Output(_) => {}
            }
        }

        messages
    }

    #[must_use]
    fn prompt_import_binding_messages(&self, prompt_import_declaration: &McpPromptImportDeclaration) -> Vec<String> {
        let Some(server_lock) = self.servers.get(&prompt_import_declaration.source.server_name) else {
            return Vec::new();
        };
        let Some(prompt_arguments) = server_lock.prompt_arguments_for_name(&prompt_import_declaration.source.item_name) else {
            return Vec::new();
        };

        prompt_import_declaration.required_binding_messages(prompt_arguments)
    }

    fn apply_to_tool_declaration(&self, tool_declaration: &mut ToolDeclaration) {
        let Some((resolved_tool_name, mcp_tool)) = self.find_tool_for_tool_declaration(tool_declaration) else {
            return;
        };

        if let Some(ToolSource::Mcp(mcp_tool_source)) = &mut tool_declaration.source {
            mcp_tool_source.tool_name = resolved_tool_name;
        }

        tool_declaration.apply_mcp_schema(mcp_tool);
    }

    fn apply_to_resource_import_declaration(&self, resource_import_declaration: &mut McpResourceImportDeclaration) {
        if let Some(resolved_resource_name) = self.find_resource_name(
            &resource_import_declaration.source.server_name,
            &resource_import_declaration.source.item_name,
        ) {
            resource_import_declaration.source.item_name = resolved_resource_name;
        }
    }

    fn apply_to_prompt_import_declaration(&self, prompt_import_declaration: &mut McpPromptImportDeclaration) {
        if let Some(resolved_prompt_name) = self.find_prompt_name(
            &prompt_import_declaration.source.server_name,
            &prompt_import_declaration.source.item_name,
        ) {
            prompt_import_declaration.source.item_name = resolved_prompt_name;
        }
    }

    #[must_use]
    fn find_resource_name(&self, server_name: &str, requested_resource_name: &str) -> Option<String> {
        self.servers.get(server_name)?.find_resource_with_name(requested_resource_name)
    }

    #[must_use]
    fn find_prompt_name(&self, server_name: &str, requested_prompt_name: &str) -> Option<String> {
        self.servers.get(server_name)?.find_prompt_with_name(requested_prompt_name)
    }

    #[must_use]
    fn find_tool_for_tool_declaration(&self, tool_declaration: &ToolDeclaration) -> Option<(String, &McpToolLock)> {
        let Some(tool_source) = &tool_declaration.source else {
            return None;
        };

        let ToolSource::Mcp(mcp_tool_source) = tool_source;

        if mcp_tool_source.server_name.is_none() {
            if let Some(server_lock) = self.servers.get(&mcp_tool_source.tool_name) {
                if let Some((resolved_tool_name, mcp_tool)) = server_lock.find_tool_with_name(&tool_declaration.name) {
                    return Some((resolved_tool_name, mcp_tool));
                }
            }
        }

        self.find_tool_with_name(tool_source)
    }
}

impl McpServerLock {
    #[must_use]
    pub fn find_tool_with_name(&self, requested_tool_name: &str) -> Option<(String, &McpToolLock)> {
        if let Some(mcp_tool_lock) = self.tools.get(requested_tool_name) {
            return Some((requested_tool_name.to_string(), mcp_tool_lock));
        }

        let normalized_requested_name = Self::normalize_item_name(requested_tool_name);

        for (tool_name, mcp_tool_lock) in &self.tools {
            if Self::normalize_item_name(tool_name) == normalized_requested_name {
                return Some((tool_name.clone(), mcp_tool_lock));
            }
        }

        None
    }

    #[must_use]
    pub fn find_resource_with_name(&self, requested_resource_name: &str) -> Option<String> {
        Self::find_listed_item_with_name(&self.resources, requested_resource_name)
    }

    #[must_use]
    pub fn find_prompt_with_name(&self, requested_prompt_name: &str) -> Option<String> {
        Self::find_listed_item_with_name(&self.prompts, requested_prompt_name)
    }

    #[must_use]
    pub fn prompt_arguments_for_name(&self, requested_prompt_name: &str) -> Option<&[McpPromptArgumentLock]> {
        let prompt_name = self.find_prompt_with_name(requested_prompt_name)?;

        self.prompt_arguments.get(&prompt_name).map(std::vec::Vec::as_slice)
    }

    #[must_use]
    fn find_listed_item_with_name(listed_item_names: &[String], requested_item_name: &str) -> Option<String> {
        if listed_item_names
            .iter()
            .any(|listed_item_name| listed_item_name == requested_item_name)
        {
            return Some(requested_item_name.to_string());
        }

        let normalized_requested_name = Self::normalize_item_name(requested_item_name);

        listed_item_names
            .iter()
            .find(|listed_item_name| Self::normalize_item_name(listed_item_name) == normalized_requested_name)
            .cloned()
    }

    #[must_use]
    pub fn normalize_item_name(item_name: &str) -> String {
        let mut normalized_name = String::new();
        let mut previous_was_underscore = false;

        for (index, character) in item_name.chars().enumerate() {
            if character.is_ascii_uppercase() {
                if index > 0 && !previous_was_underscore {
                    normalized_name.push('_');
                }

                normalized_name.push(character.to_ascii_lowercase());
                previous_was_underscore = false;

                continue;
            }

            if character.is_ascii_lowercase() || character.is_ascii_digit() {
                normalized_name.push(character);
                previous_was_underscore = false;

                continue;
            }

            if !previous_was_underscore {
                normalized_name.push('_');
                previous_was_underscore = true;
            }
        }

        normalized_name.trim_matches('_').to_string()
    }

    #[must_use]
    pub fn normalize_tool_name(tool_name: &str) -> String {
        Self::normalize_item_name(tool_name)
    }
}

impl McpPromptImportDeclaration {
    #[must_use]
    fn has_parameter_binding(&self, parameter_name: &str) -> bool {
        self.parameters.iter().any(|parameter| parameter.name == parameter_name)
    }

    #[must_use]
    fn required_binding_messages(&self, prompt_arguments: &[McpPromptArgumentLock]) -> Vec<String> {
        let mut messages = Vec::new();

        for prompt_argument in prompt_arguments.iter().filter(|prompt_argument| prompt_argument.required) {
            if self.has_parameter_binding(&prompt_argument.name) {
                continue;
            }

            messages.push(format!(
                "MCP prompt `{}` requires binding `{}` from server prompt `{}`",
                self.name, prompt_argument.name, self.source.item_name
            ));
        }

        messages
    }
}

impl ProjectMcpLock {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            version: 1,
            workflows: BTreeMap::new(),
        }
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
        let lock_text = serde_json::to_string_pretty(self).map_err(|source| McpError::SerializeLock {
            path: lock_path.display().to_string(),
            source,
        })?;

        std::fs::write(lock_path, format!("{lock_text}\n")).map_err(|source| McpError::WriteLock {
            path: lock_path.display().to_string(),
            source,
        })
    }

    pub fn insert_workflow_lock(&mut self, lock_root: &Path, workflow_path: &Path, workflow_lock: McpLock) {
        self.insert_workflow_lock_with_source(lock_root, workflow_path, workflow_lock, "");
    }

    pub fn insert_workflow_lock_with_source(
        &mut self,
        lock_root: &Path,
        workflow_path: &Path,
        workflow_lock: McpLock,
        workflow_source: &str,
    ) {
        let workflow_key = Self::workflow_key(lock_root, workflow_path);
        let workflow_hash = Self::workflow_hash(workflow_source);

        self.workflows.insert(
            workflow_key,
            ProjectWorkflowMcpLockEntry {
                hash: workflow_hash,
                lock: workflow_lock,
            },
        );
    }

    #[must_use]
    pub fn workflow_lock(&self, lock_root: &Path, workflow_path: &Path) -> Option<&McpLock> {
        let workflow_key = Self::workflow_key(lock_root, workflow_path);

        self.workflows.get(&workflow_key).map(ProjectWorkflowMcpLockEntry::lock)
    }

    #[must_use]
    pub fn discover_lock_path_for_workflow(workflow_path: &Path) -> Option<PathBuf> {
        let mut current_directory = if workflow_path.is_dir() {
            workflow_path.to_path_buf()
        } else {
            workflow_path.parent()?.to_path_buf()
        };

        loop {
            let candidate_path = current_directory.join(PROJECT_MCP_LOCK_FILE_NAME);

            if candidate_path.exists() {
                return Some(candidate_path);
            }

            if !current_directory.pop() {
                return None;
            }
        }
    }

    fn workflow_key(lock_root: &Path, workflow_path: &Path) -> String {
        let normalized_workflow_path = workflow_path.canonicalize().unwrap_or_else(|_error| workflow_path.to_path_buf());
        let lock_root_path = if lock_root.as_os_str().is_empty() {
            Path::new(".")
        } else {
            lock_root
        };
        let normalized_lock_root = lock_root_path.canonicalize().unwrap_or_else(|_error| lock_root_path.to_path_buf());
        let relative_workflow_path = normalized_workflow_path
            .strip_prefix(&normalized_lock_root)
            .unwrap_or(normalized_workflow_path.as_path());

        relative_workflow_path.to_string_lossy().replace('\\', "/")
    }

    fn workflow_hash(workflow_source: &str) -> String {
        format!("{:x}", Sha256::digest(workflow_source.as_bytes()))
    }
}

impl ProjectWorkflowMcpLockEntry {
    #[must_use]
    pub fn lock(&self) -> &McpLock {
        &self.lock
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

impl ToolDeclaration {
    fn apply_mcp_schema(&mut self, mcp_tool: &McpToolLock) {
        if self.description.is_none() {
            self.description.clone_from(&mcp_tool.description);
        }

        if self.input_fields.is_empty() {
            let fixed_binding_names = self
                .fixed_binding_fields
                .iter()
                .map(|fixed_binding_field| fixed_binding_field.name.as_str())
                .collect::<Vec<_>>();

            self.input_fields = mcp_tool.input_fields_except(&fixed_binding_names);
        }

        if self.output_fields.is_empty() {
            self.output_fields = mcp_tool.output_fields();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{McpLock, McpPromptArgumentLock, McpServerLock};
    use crate::parse_inline_workflow;
    use std::collections::BTreeMap;

    #[test]
    fn validates_mixed_prompt_batch_with_shared_and_item_bindings() {
        let workflow = parse_inline_workflow! {
            input {
                project_id: number
                task_id: number
            }

            from mcp.local {
                bindings {
                    project_id: input.project_id
                    task_id: input.task_id
                }

                prompt dynamic_summary_prompt {
                    bindings {
                        type: "task"
                    }
                }
            }
        };
        let mcp_lock = prompt_argument_lock();
        let binding_messages = mcp_lock.validate_prompt_import_bindings(&workflow);

        assert!(binding_messages.is_empty());
    }

    #[test]
    fn rejects_mixed_prompt_batch_when_item_binding_is_missing() {
        let workflow = parse_inline_workflow! {
            input {
                project_id: number
                task_id: number
            }

            from mcp.local {
                bindings {
                    project_id: input.project_id
                    task_id: input.task_id
                }

                prompt dynamic_summary_prompt
            }
        };
        let mcp_lock = prompt_argument_lock();
        let binding_messages = mcp_lock.validate_prompt_import_bindings(&workflow);

        assert_eq!(
            binding_messages,
            vec!["MCP prompt `dynamic_summary_prompt` requires binding `type` from server prompt `dynamic_summary_prompt`"]
        );
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
}
