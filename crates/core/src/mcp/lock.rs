use crate::dsl::{Declaration, ToolDeclaration, ToolSource, Workflow};
use crate::mcp::schema::to_json_value;
use crate::mcp::{McpClient, McpError, McpServerConfig};
use crate::semantic::support::expression::EvaluationContext;
use rust_mcp_schema::{ToolInputSchema, ToolOutputSchema};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct McpLock {
    pub servers: BTreeMap<String, McpServerLock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution_context: Option<McpLockResolutionContext>,
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
pub struct McpServerLock {
    pub tools: BTreeMap<String, McpToolLock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prompts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolLock {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: ToolInputSchema,
    pub output_schema: Option<ToolOutputSchema>,
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
        let mut lock = Self::discover_from_workflow_with_context(workflow, &evaluation_context)?;
        lock.resolution_context = Some(lock_context.clone());

        Ok(lock)
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
        let ToolSource::Mcp(mcp_tool_source) = source;

        if let Some(server_name) = &mcp_tool_source.server_name {
            return self
                .servers
                .get(server_name)
                .and_then(|server_lock| server_lock.tools.get(&mcp_tool_source.tool_name));
        }

        self.servers
            .values()
            .find_map(|server_lock| server_lock.tools.get(&mcp_tool_source.tool_name))
    }

    pub fn apply_to_workflow(&self, workflow: &mut Workflow) {
        for declaration in &mut workflow.declarations {
            let Declaration::Tool(tool_declaration) = declaration else {
                continue;
            };
            let Some(mcp_tool) = self.find_tool_for_tool_declaration(tool_declaration) else {
                continue;
            };

            tool_declaration.apply_mcp_schema(mcp_tool);
        }
    }

    #[must_use]
    fn find_tool_for_tool_declaration(&self, tool_declaration: &ToolDeclaration) -> Option<&McpToolLock> {
        let Some(tool_source) = &tool_declaration.source else {
            return None;
        };

        let ToolSource::Mcp(mcp_tool_source) = tool_source;

        if mcp_tool_source.server_name.is_none() {
            if let Some(server_lock) = self.servers.get(&mcp_tool_source.tool_name) {
                if let Some(mcp_tool) = server_lock.tools.get(&tool_declaration.name) {
                    return Some(mcp_tool);
                }
            }
        }

        self.find_tool(tool_source)
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
