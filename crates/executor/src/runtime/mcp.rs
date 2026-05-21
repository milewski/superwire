use super::{ExecutorError, WorkflowExecutor};
use crate::event::{ExecutorEvent, McpCallEventDetails};
use crate::model::ToolCallTracker;
use serde_json::{Map, Value};
use std::time::Instant;
use superwire_core::dsl::{McpCall, McpCallOperation, ObjectField};
use superwire_core::mcp::McpServerConfig;
use superwire_core::semantic::support::expression::EvaluationContext;
use tokio::sync::mpsc;

impl WorkflowExecutor {
    pub(in crate::runtime) fn execute_mcp_call(
        &self,
        mcp_call: &McpCall,
        evaluation_context: &EvaluationContext,
        event_sender: Option<&mpsc::Sender<ExecutorEvent>>,
    ) -> Result<Value, ExecutorError> {
        let target_name = mcp_call.target_name().ok_or_else(|| ExecutorError::Other {
            message: format!("{} call requires a target name", mcp_call.operation.as_str()),
        })?;
        let expected_root = mcp_call.operation.expected_root();

        if mcp_call.callee.root_keyword() != Some(expected_root) {
            return Err(ExecutorError::Other {
                message: format!(
                    "{} call must target `{}.<name>`",
                    mcp_call.operation.as_str(),
                    expected_root.as_str()
                ),
            });
        }

        match mcp_call.operation {
            McpCallOperation::Read => self.execute_resource_read(target_name, mcp_call, evaluation_context, event_sender),
            McpCallOperation::Render => self.execute_prompt_render(target_name, mcp_call, evaluation_context, event_sender),
        }
    }

    fn execute_resource_read(
        &self,
        resource_name: &str,
        mcp_call: &McpCall,
        evaluation_context: &EvaluationContext,
        event_sender: Option<&mpsc::Sender<ExecutorEvent>>,
    ) -> Result<Value, ExecutorError> {
        let resource_import = self
            .workflow
            .find_resource_import(resource_name)
            .ok_or_else(|| ExecutorError::Other {
                message: format!("resource `{resource_name}` is not imported"),
            })?;
        let server_config = self.resolve_mcp_import_server(&resource_import.source.server_name, evaluation_context)?;
        let arguments = self.resolve_mcp_call_parameters(
            &resource_import.parameters,
            &mcp_call.parameter_fields,
            evaluation_context,
            resource_name,
        )?;
        let call_details = McpCallEventDetails::new(
            mcp_call.operation.as_str().to_string(),
            resource_name.to_string(),
            resource_import.source.server_name.clone(),
            resource_import.source.item_name.clone(),
            arguments.clone(),
            None,
        );

        if let Some(sender) = event_sender {
            let _ = sender.try_send(ExecutorEvent::mcp_call_started(call_details.clone()));
        }

        let started_at = Instant::now();
        let result = match self
            .mcp_pool
            .get(&server_config)?
            .read_resource(&resource_import.source.item_name, arguments)
        {
            Ok(result) => result,
            Err(error) => {
                if let Some(sender) = event_sender {
                    let _ = sender.try_send(ExecutorEvent::mcp_call_failed(
                        call_details,
                        Value::String(error.to_string()),
                        started_at.elapsed(),
                    ));
                }

                return Err(ExecutorError::Other {
                    message: format!("MCP resource `{resource_name}` failed: {error}"),
                });
            }
        };
        let rendered_result = Value::String(render_mcp_resource_result(&result));

        if let Some(sender) = event_sender {
            let _ = sender.try_send(ExecutorEvent::mcp_call_completed(
                call_details,
                rendered_result.clone(),
                result,
                started_at.elapsed(),
            ));
        }

        Ok(rendered_result)
    }

    fn execute_prompt_render(
        &self,
        prompt_name: &str,
        mcp_call: &McpCall,
        evaluation_context: &EvaluationContext,
        event_sender: Option<&mpsc::Sender<ExecutorEvent>>,
    ) -> Result<Value, ExecutorError> {
        let prompt_import = self.workflow.find_prompt_import(prompt_name).ok_or_else(|| ExecutorError::Other {
            message: format!("prompt `{prompt_name}` is not imported"),
        })?;
        let server_config = self.resolve_mcp_import_server(&prompt_import.source.server_name, evaluation_context)?;
        let arguments = self.resolve_mcp_call_parameters(
            &prompt_import.parameters,
            &mcp_call.parameter_fields,
            evaluation_context,
            prompt_name,
        )?;
        let call_details = McpCallEventDetails::new(
            mcp_call.operation.as_str().to_string(),
            prompt_name.to_string(),
            prompt_import.source.server_name.clone(),
            prompt_import.source.item_name.clone(),
            arguments.clone(),
            None,
        );

        if let Some(sender) = event_sender {
            let _ = sender.try_send(ExecutorEvent::mcp_call_started(call_details.clone()));
        }

        let started_at = Instant::now();
        let result = match self
            .mcp_pool
            .get(&server_config)?
            .get_prompt(&prompt_import.source.item_name, arguments)
        {
            Ok(result) => result,
            Err(error) => {
                if let Some(sender) = event_sender {
                    let _ = sender.try_send(ExecutorEvent::mcp_call_failed(
                        call_details,
                        Value::String(error.to_string()),
                        started_at.elapsed(),
                    ));
                }

                return Err(ExecutorError::Other {
                    message: format!("MCP prompt `{prompt_name}` failed: {error}"),
                });
            }
        };
        let rendered_result = Value::String(render_mcp_prompt_result(&result));

        if let Some(sender) = event_sender {
            let _ = sender.try_send(ExecutorEvent::mcp_call_completed(
                call_details,
                rendered_result.clone(),
                result,
                started_at.elapsed(),
            ));
        }

        Ok(rendered_result)
    }

    fn resolve_mcp_call_parameters(
        &self,
        import_parameters: &[ObjectField],
        call_parameters: &[ObjectField],
        evaluation_context: &EvaluationContext,
        import_name: &str,
    ) -> Result<Value, ExecutorError> {
        let mut resolved_parameters = Map::new();

        for parameter in import_parameters.iter().chain(call_parameters.iter()) {
            let parameter_value = self.evaluate_runtime_expression(
                &parameter.value,
                evaluation_context,
                &format!("MCP call `{import_name}` parameter `{}`", parameter.name),
                None,
                &ToolCallTracker::default(),
            )?;
            resolved_parameters.insert(parameter.name.clone(), parameter_value);
        }

        Ok(Value::Object(resolved_parameters))
    }

    pub(in crate::runtime) fn resolve_mcp_import_context(&self, evaluation_context: &EvaluationContext) -> Result<String, ExecutorError> {
        let mut context_sections = Vec::new();

        for prompt_import in self.workflow.prompt_imports() {
            let server_config = self.resolve_mcp_import_server(&prompt_import.source.server_name, evaluation_context)?;
            let arguments = self.resolve_mcp_import_parameters(&prompt_import.parameters, evaluation_context, &prompt_import.name)?;
            let result = self
                .mcp_pool
                .get(&server_config)?
                .get_prompt(&prompt_import.source.item_name, arguments)
                .map_err(|error| ExecutorError::Other {
                    message: format!("MCP prompt `{}` failed: {error}", prompt_import.name),
                })?;

            context_sections.push(format!(
                "MCP prompt `{}`:\n{}",
                prompt_import.name,
                render_mcp_prompt_result(&result)
            ));
        }

        for resource_import in self.workflow.resource_imports() {
            let server_config = self.resolve_mcp_import_server(&resource_import.source.server_name, evaluation_context)?;
            let arguments = self.resolve_mcp_import_parameters(&resource_import.parameters, evaluation_context, &resource_import.name)?;
            let result = self
                .mcp_pool
                .get(&server_config)?
                .read_resource(&resource_import.source.item_name, arguments)
                .map_err(|error| ExecutorError::Other {
                    message: format!("MCP resource `{}` failed: {error}", resource_import.name),
                })?;

            context_sections.push(format!(
                "MCP resource `{}`:\n{}",
                resource_import.name,
                render_mcp_resource_result(&result)
            ));
        }

        Ok(context_sections.join("\n\n"))
    }

    pub(in crate::runtime) fn resolve_mcp_import_server(
        &self,
        server_name: &str,
        evaluation_context: &EvaluationContext,
    ) -> Result<McpServerConfig, ExecutorError> {
        let mcp_server_declaration = self.workflow.find_mcp_server(server_name).ok_or_else(|| ExecutorError::Other {
            message: format!("MCP import references unknown MCP server `{server_name}`"),
        })?;

        McpServerConfig::resolve_from_declaration(mcp_server_declaration, evaluation_context).map_err(|error| ExecutorError::Other {
            message: error.to_string(),
        })
    }

    pub(in crate::runtime) fn resolve_mcp_import_parameters(
        &self,
        parameters: &[ObjectField],
        evaluation_context: &EvaluationContext,
        import_name: &str,
    ) -> Result<Value, ExecutorError> {
        let mut resolved_parameters = Map::new();

        for parameter in parameters {
            let parameter_value = self.evaluate_runtime_expression(
                &parameter.value,
                evaluation_context,
                &format!("MCP import `{import_name}` parameter `{}`", parameter.name),
                None,
                &ToolCallTracker::default(),
            )?;
            resolved_parameters.insert(parameter.name.clone(), parameter_value);
        }

        Ok(Value::Object(resolved_parameters))
    }

    pub(in crate::runtime) fn merge_mcp_import_binding_overrides(
        &self,
        bindings: Value,
        override_binding_fields: &[ObjectField],
        evaluation_context: &EvaluationContext,
        import_name: &str,
    ) -> Result<Value, ExecutorError> {
        let mut binding_object = bindings.as_object().cloned().unwrap_or_default();

        for override_binding_field in override_binding_fields {
            let binding_value = self.evaluate_runtime_expression(
                &override_binding_field.value,
                evaluation_context,
                &format!("MCP import `{import_name}` binding `{}`", override_binding_field.name),
                None,
                &ToolCallTracker::default(),
            )?;
            binding_object.insert(override_binding_field.name.clone(), binding_value);
        }

        Ok(Value::Object(binding_object))
    }
}

pub(in crate::runtime) fn normalize_prompt(prompt_value: Value) -> String {
    if let Some(prompt) = prompt_value.as_str() {
        return prompt.to_string();
    }

    serde_json::to_string(&prompt_value).unwrap_or_else(|_| prompt_value.to_string())
}

fn render_mcp_prompt_result(result: &Value) -> String {
    let Some(messages) = result.get("messages").and_then(Value::as_array) else {
        return normalize_prompt(result.clone());
    };
    let mut rendered_messages = Vec::new();

    for message in messages {
        let role = message.get("role").and_then(Value::as_str).unwrap_or("message");
        let content = message.get("content").map_or_else(String::new, render_mcp_content_value);
        rendered_messages.push(format!("{role}: {content}"));
    }

    rendered_messages.join("\n")
}

fn render_mcp_resource_result(result: &Value) -> String {
    let Some(contents) = result.get("contents").and_then(Value::as_array) else {
        return normalize_prompt(result.clone());
    };
    let mut rendered_contents = Vec::new();

    for content in contents {
        rendered_contents.push(render_mcp_content_value(content));
    }

    rendered_contents.join("\n")
}

fn render_mcp_content_value(content: &Value) -> String {
    if let Some(text) = content.as_str() {
        return text.to_string();
    }

    if let Some(text) = content.get("text").and_then(Value::as_str) {
        return text.to_string();
    }

    if let Some(blob) = content.get("blob").and_then(Value::as_str) {
        return blob.to_string();
    }

    normalize_prompt(content.clone())
}
