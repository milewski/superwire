use super::{ExecutorError, ToolCallExecutionContext, WorkflowExecutor};
use crate::model::{ExecutorEventSenderExt, ToolCallTracker};
use serde_json::Value;
use std::time::Instant;
use superwire_dsl::{McpCall, McpCallOperation, McpImportBindingEvaluationKind, McpImportBindings, ObjectField};
use superwire_mcp::{render_mcp_prompt_result, render_mcp_resource_result, McpServerConfig};
use superwire_protocol::event::{ExecutorEvent, McpCallEventDetails, McpOperation};
use superwire_semantic::support::expression::EvaluationContext;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy)]
pub(in crate::runtime) struct McpRenderContext<'a> {
    evaluation_context: &'a EvaluationContext,
    event_sender: Option<&'a mpsc::Sender<ExecutorEvent>>,
}

impl<'a> From<ToolCallExecutionContext<'a>> for McpRenderContext<'a> {
    fn from(tool_call_execution_context: ToolCallExecutionContext<'a>) -> Self {
        Self {
            evaluation_context: tool_call_execution_context.evaluation_context,
            event_sender: tool_call_execution_context.event_sender,
        }
    }
}

impl WorkflowExecutor {
    pub(in crate::runtime) fn execute_mcp_call(
        &self,
        mcp_call: &McpCall,
        mcp_render_context: McpRenderContext<'_>,
    ) -> Result<Value, ExecutorError> {
        let target_name = mcp_call.target_name().ok_or_else(|| ExecutorError::Other {
            message: format!("{} call requires a target name", mcp_call.operation.as_str()),
        })?;
        let expected_root = mcp_call.operation.expected_root();

        if !mcp_call.has_valid_callee() {
            return Err(ExecutorError::Other {
                message: format!(
                    "{} call must target `{}.<name>`",
                    mcp_call.operation.as_str(),
                    expected_root.as_str()
                ),
            });
        }

        match mcp_call.operation {
            McpCallOperation::Read => self.execute_resource_read(target_name, mcp_call, mcp_render_context),
            McpCallOperation::Render => self.execute_prompt_render(target_name, mcp_call, mcp_render_context),
        }
    }

    fn execute_resource_read(
        &self,
        resource_name: &str,
        mcp_call: &McpCall,
        mcp_render_context: McpRenderContext<'_>,
    ) -> Result<Value, ExecutorError> {
        let resource_import = self.lookups.resource_import(resource_name).ok_or_else(|| ExecutorError::Other {
            message: format!("resource `{resource_name}` is not imported"),
        })?;
        let server_config = self.resolve_mcp_import_server(&resource_import.source.server_name, mcp_render_context.evaluation_context)?;
        let arguments = self.resolve_mcp_call_parameters(
            &resource_import.parameters,
            &mcp_call.parameter_fields,
            mcp_render_context.evaluation_context,
            resource_name,
        )?;
        let call_details = McpCallEventDetails::from_arguments(
            McpOperation::from(mcp_call.operation),
            resource_name.to_string(),
            resource_import.source.server_name.clone(),
            resource_import.source.item_name.clone(),
            &arguments,
        );

        if let Some(sender) = mcp_render_context.event_sender {
            sender.try_send_observed(ExecutorEvent::mcp_call_started(call_details.clone()));
        }

        let started_at = Instant::now();
        let result = match self
            .mcp_pool
            .get(&server_config)?
            .read_resource(&resource_import.source.item_name, arguments)
        {
            Ok(result) => result,
            Err(error) => {
                if let Some(sender) = mcp_render_context.event_sender {
                    sender.try_send_observed(ExecutorEvent::mcp_call_failed(call_details, started_at.elapsed()));
                }

                return Err(ExecutorError::mcp_with_source(
                    None,
                    Some(resource_import.source.server_name.clone()),
                    Some(resource_name.to_string()),
                    format!("MCP resource `{resource_name}` request failed"),
                    error,
                ));
            }
        };
        let rendered_result = Value::String(render_mcp_resource_result(&result));

        if let Some(sender) = mcp_render_context.event_sender {
            sender.try_send_observed(ExecutorEvent::mcp_call_completed(
                call_details,
                &rendered_result,
                started_at.elapsed(),
            ));
        }

        Ok(rendered_result)
    }

    fn execute_prompt_render(
        &self,
        prompt_name: &str,
        mcp_call: &McpCall,
        mcp_render_context: McpRenderContext<'_>,
    ) -> Result<Value, ExecutorError> {
        let prompt_import = self.lookups.prompt_import(prompt_name).ok_or_else(|| ExecutorError::Other {
            message: format!("prompt `{prompt_name}` is not imported"),
        })?;
        let server_config = self.resolve_mcp_import_server(&prompt_import.source.server_name, mcp_render_context.evaluation_context)?;
        let arguments = self.resolve_mcp_call_parameters(
            &prompt_import.parameters,
            &mcp_call.parameter_fields,
            mcp_render_context.evaluation_context,
            prompt_name,
        )?;
        let call_details = McpCallEventDetails::from_arguments(
            McpOperation::from(mcp_call.operation),
            prompt_name.to_string(),
            prompt_import.source.server_name.clone(),
            prompt_import.source.item_name.clone(),
            &arguments,
        );

        if let Some(sender) = mcp_render_context.event_sender {
            sender.try_send_observed(ExecutorEvent::mcp_call_started(call_details.clone()));
        }

        let started_at = Instant::now();
        let result = match self
            .mcp_pool
            .get(&server_config)?
            .get_prompt(&prompt_import.source.item_name, arguments)
        {
            Ok(result) => result,
            Err(error) => {
                if let Some(sender) = mcp_render_context.event_sender {
                    sender.try_send_observed(ExecutorEvent::mcp_call_failed(call_details, started_at.elapsed()));
                }

                return Err(ExecutorError::mcp_with_source(
                    None,
                    Some(prompt_import.source.server_name.clone()),
                    Some(prompt_name.to_string()),
                    format!("MCP prompt `{prompt_name}` request failed"),
                    error,
                ));
            }
        };
        let rendered_result = Value::String(render_mcp_prompt_result(&result));

        if let Some(sender) = mcp_render_context.event_sender {
            sender.try_send_observed(ExecutorEvent::mcp_call_completed(
                call_details,
                &rendered_result,
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
        let parameter_tool_call_tracker = ToolCallTracker::default();
        let tool_call_execution_context = ToolCallExecutionContext::new(evaluation_context, None, &parameter_tool_call_tracker);

        McpImportBindings::new(import_parameters, call_parameters).evaluate_json(
            import_name,
            McpImportBindingEvaluationKind::CallParameter,
            |parameter, field_context| self.evaluate_runtime_expression(&parameter.value, tool_call_execution_context, &field_context),
        )
    }

    pub(in crate::runtime) fn resolve_mcp_import_bindings(
        &self,
        import_parameters: &[ObjectField],
        override_binding_fields: &[ObjectField],
        evaluation_context: &EvaluationContext,
        import_name: &str,
    ) -> Result<Value, ExecutorError> {
        let binding_tool_call_tracker = ToolCallTracker::default();
        let tool_call_execution_context = ToolCallExecutionContext::new(evaluation_context, None, &binding_tool_call_tracker);

        McpImportBindings::new(import_parameters, override_binding_fields).evaluate_json_with_local_kind(
            import_name,
            McpImportBindingEvaluationKind::ImportParameter,
            McpImportBindingEvaluationKind::ImportBinding,
            |binding_field, field_context| {
                self.evaluate_runtime_expression(&binding_field.value, tool_call_execution_context, &field_context)
            },
        )
    }

    pub(in crate::runtime) fn resolve_mcp_import_parameters(
        &self,
        parameters: &[ObjectField],
        evaluation_context: &EvaluationContext,
        import_name: &str,
    ) -> Result<Value, ExecutorError> {
        let parameter_tool_call_tracker = ToolCallTracker::default();
        let tool_call_execution_context = ToolCallExecutionContext::new(evaluation_context, None, &parameter_tool_call_tracker);

        McpImportBindings::new(&[], parameters).evaluate_json(
            import_name,
            McpImportBindingEvaluationKind::ImportParameter,
            |parameter, field_context| self.evaluate_runtime_expression(&parameter.value, tool_call_execution_context, &field_context),
        )
    }

    pub(in crate::runtime) fn resolve_mcp_import_server(
        &self,
        server_name: &str,
        evaluation_context: &EvaluationContext,
    ) -> Result<McpServerConfig, ExecutorError> {
        let mcp_server_declaration = self.lookups.mcp_server(server_name).ok_or_else(|| ExecutorError::Other {
            message: format!("MCP import references unknown MCP server `{server_name}`"),
        })?;

        McpServerConfig::resolve_from_declaration_with_endpoint_validator(
            mcp_server_declaration,
            evaluation_context,
            |resolved_name, endpoint| self.mcp_pool.validate_endpoint(resolved_name, endpoint),
        )
        .map_err(|error| ExecutorError::mcp_with_source(None, Some(server_name.to_string()), None, error.public_message(), error))
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
                .map_err(|error| {
                    ExecutorError::mcp_with_source(
                        None,
                        Some(prompt_import.source.server_name.clone()),
                        Some(prompt_import.name.clone()),
                        format!("MCP prompt `{}` failed: {error}", prompt_import.name),
                        error,
                    )
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
                .map_err(|error| {
                    ExecutorError::mcp_with_source(
                        None,
                        Some(resource_import.source.server_name.clone()),
                        Some(resource_import.name.clone()),
                        format!("MCP resource `{}` failed: {error}", resource_import.name),
                        error,
                    )
                })?;

            context_sections.push(format!(
                "MCP resource `{}`:\n{}",
                resource_import.name,
                render_mcp_resource_result(&result)
            ));
        }

        Ok(context_sections.join("\n\n"))
    }
}

pub(in crate::runtime) use superwire_mcp::normalize_mcp_prompt_value as normalize_prompt;
