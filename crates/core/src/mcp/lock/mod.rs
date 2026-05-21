use crate::dsl::{Declaration, ToolSource, Workflow};
use crate::mcp::schema::to_json_value;
use crate::mcp::{McpClient, McpError, McpServerConfig};
use crate::semantic::support::expression::EvaluationContext;
use rust_mcp_schema::{ToolInputSchema, ToolOutputSchema};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

mod apply;
mod name_resolution;
mod project;
mod validate;

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
