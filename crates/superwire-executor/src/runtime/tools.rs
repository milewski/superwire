use super::{ExecutorError, ToolCallExecutionContext, WorkflowExecutor};
use crate::event::{ExecutorEvent, McpCallEventDetails};
use crate::model::{ModelSchema, ModelToolDefinition, ModelToolSource, ToolCallLimitScope};
use serde_json::{Map, Value};
use std::time::Instant;
use superwire_dsl::{AgentExpressionPropertyName, Declaration, Expression, ObjectField, ReferenceKeyword, ToolCall, ToolSource, Workflow};
use superwire_mcp::{normalize_mcp_tool_result, McpServerConfig};
use superwire_semantic::support::expression::{evaluate_expression, EvaluationContext};
use superwire_semantic::support::types::WorkflowType;
use superwire_semantic::{PlannedAgent, TypedToolIr};
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy)]
pub(in crate::runtime) struct StartupMcpToolValidationContext<'a> {
    pub(in crate::runtime) evaluation_context: &'a EvaluationContext,
    pub(in crate::runtime) event_sender: Option<&'a mpsc::Sender<ExecutorEvent>>,
}

impl WorkflowExecutor {
    pub(super) fn planned_agent_available_mcp_calls(
        &self,
        planned_agent: &PlannedAgent,
        evaluation_context: &EvaluationContext,
    ) -> Result<Vec<Value>, ExecutorError> {
        let tool_definitions = self.resolve_agent_use_definitions(planned_agent, evaluation_context)?;
        let mut calls = Vec::new();

        for tool_definition in tool_definitions {
            match tool_definition.source {
                ModelToolSource::Mcp {
                    server_name, tool_name, ..
                } => calls.push(serde_json::json!({
                    "operation": "call",
                    "target_name": tool_definition.name,
                    "server_name": server_name.unwrap_or_else(|| "default".to_string()),
                    "item_name": tool_name,
                })),
                ModelToolSource::McpPrompt {
                    server_name, prompt_name, ..
                } => calls.push(serde_json::json!({
                    "operation": "render",
                    "target_name": tool_definition.name,
                    "server_name": server_name,
                    "item_name": prompt_name,
                })),
                ModelToolSource::McpResource {
                    server_name,
                    resource_name,
                    ..
                } => calls.push(serde_json::json!({
                    "operation": "read",
                    "target_name": tool_definition.name,
                    "server_name": server_name,
                    "item_name": resource_name,
                })),
                ModelToolSource::Finalize | ModelToolSource::Local => {}
            }
        }

        Ok(calls)
    }

    fn planned_mcp_tool_call(&self, tool_name: &str, evaluation_context: &EvaluationContext) -> Result<Option<Value>, ExecutorError> {
        let Some(typed_tool) = self.execution_plan.tools.get(tool_name) else {
            return Ok(None);
        };
        let ModelToolSource::Mcp {
            server_name,
            tool_name: mcp_tool_name,
            ..
        } = self.model_tool_source(&typed_tool.declaration, evaluation_context)?
        else {
            return Ok(None);
        };

        Ok(Some(serde_json::json!({
            "operation": "call",
            "target_name": tool_name,
            "server_name": server_name.unwrap_or_else(|| "default".to_string()),
            "item_name": mcp_tool_name,
        })))
    }

    fn planned_mcp_import_call(&self, mcp_call: &superwire_dsl::McpCall) -> Option<Value> {
        let target_name = mcp_call.target_name()?;

        match mcp_call.operation {
            superwire_dsl::McpCallOperation::Read => {
                let resource_import = self.lookups.resource_import(target_name)?;

                Some(serde_json::json!({
                    "operation": "read",
                    "target_name": target_name,
                    "server_name": resource_import.source.server_name,
                    "item_name": resource_import.source.item_name,
                }))
            }
            superwire_dsl::McpCallOperation::Render => {
                let prompt_import = self.lookups.prompt_import(target_name)?;

                Some(serde_json::json!({
                    "operation": "render",
                    "target_name": target_name,
                    "server_name": prompt_import.source.server_name,
                    "item_name": prompt_import.source.item_name,
                }))
            }
        }
    }

    pub(super) fn validate_startup_mcp_tool_calls(
        &self,
        startup_validation_context: StartupMcpToolValidationContext<'_>,
    ) -> Result<(), ExecutorError> {
        for tool_call in self.workflow.startup_tool_calls() {
            self.validate_startup_mcp_tool_call(tool_call, startup_validation_context)?;
        }

        for output_field in &self.execution_plan.output_declaration.fields {
            for tool_call in output_field.value.tool_calls() {
                self.validate_startup_mcp_tool_call(tool_call, startup_validation_context)?;
            }
        }

        Ok(())
    }

    fn validate_startup_mcp_tool_call(
        &self,
        tool_call: &ToolCall,
        startup_validation_context: StartupMcpToolValidationContext<'_>,
    ) -> Result<(), ExecutorError> {
        let Some(tool_name) = tool_call.callee.tool_name() else {
            return Ok(());
        };
        let Some(typed_tool) = self.execution_plan.tools.get(tool_name) else {
            return Ok(());
        };
        let ModelToolSource::Mcp { .. } = self.model_tool_source(&typed_tool.declaration, startup_validation_context.evaluation_context)?
        else {
            return Ok(());
        };
        let Ok(bindings) = typed_tool.resolve_bindings(&tool_call.binding_fields, startup_validation_context.evaluation_context) else {
            return Ok(());
        };
        let Some(arguments) =
            self.startup_tool_call_arguments(tool_call, typed_tool, startup_validation_context.evaluation_context, &bindings)?
        else {
            return Ok(());
        };
        let validation_started_at = Instant::now();
        let input_schema = ModelSchema::model_tool_input(typed_tool.input_type.clone(), bindings.clone());

        if let Some(sender) = startup_validation_context.event_sender {
            let _ = sender.try_send(ExecutorEvent::mcp_tool_validation_started(
                String::new(),
                tool_name.to_string(),
                Value::Object(arguments.clone()),
                input_schema.json_value(),
            ));
        }

        if let Some(sender) = startup_validation_context.event_sender {
            let _ = sender.try_send(ExecutorEvent::mcp_tool_validation_completed(
                String::new(),
                tool_name.to_string(),
                validation_started_at.elapsed(),
            ));
        }

        Ok(())
    }

    fn startup_tool_call_arguments(
        &self,
        tool_call: &ToolCall,
        typed_tool: &TypedToolIr,
        evaluation_context: &EvaluationContext,
        bindings: &Value,
    ) -> Result<Option<Map<String, Value>>, ExecutorError> {
        let Some(tool_name) = tool_call.callee.tool_name() else {
            return Ok(None);
        };
        let mut arguments = Map::new();
        let mut input_arguments = Map::new();

        for input_field in &tool_call.input_fields {
            let Ok(input_value) = evaluate_expression(
                &input_field.value,
                evaluation_context,
                &format!("input field `{}` for tool `{tool_name}`", input_field.name),
            ) else {
                return Ok(None);
            };
            arguments.insert(input_field.name.clone(), input_value.clone());
            input_arguments.insert(input_field.name.clone(), input_value);
        }

        typed_tool
            .input_type
            .validate_value_allowing_missing_nullable_fields(&Value::Object(input_arguments))
            .map_err(|message| ExecutorError::Other {
                message: format!("deterministic tool call `{tool_name}` input is invalid: {message}"),
            })?;

        if let Some(binding_object) = bindings.as_object() {
            for (binding_name, binding_value) in binding_object {
                arguments.insert(binding_name.clone(), binding_value.clone());
            }
        }

        Ok(Some(arguments))
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn execute_deterministic_tool_call(
        &self,
        tool_call: &ToolCall,
        tool_call_execution_context: ToolCallExecutionContext<'_>,
    ) -> Result<Value, ExecutorError> {
        let tool_name = tool_call.callee.tool_name().ok_or_else(|| ExecutorError::Other {
            message: "deterministic tool call must use `tool.<name>` reference".to_string(),
        })?;
        log::debug!("executing deterministic tool call `{tool_name}`");
        let typed_tool = self.execution_plan.tools.get(tool_name).ok_or_else(|| ExecutorError::Other {
            message: format!("deterministic tool call references unknown tool `{tool_name}`"),
        })?;

        tool_call_execution_context
            .tool_call_tracker
            .register_call(tool_name, typed_tool.declaration.max_calls, &ToolCallLimitScope::Workflow)
            .map_err(|message| ExecutorError::Other { message })?;

        let bindings = typed_tool.resolve_bindings(&tool_call.binding_fields, tool_call_execution_context.evaluation_context)?;
        let source = self.model_tool_source(&typed_tool.declaration, tool_call_execution_context.evaluation_context)?;
        let mut input_arguments = Map::new();

        for input_field in &tool_call.input_fields {
            let input_value = self.evaluate_runtime_expression(
                &input_field.value,
                tool_call_execution_context,
                &format!("input field `{}` for tool `{}`", input_field.name, tool_name),
            )?;
            input_arguments.insert(input_field.name.clone(), input_value);
        }

        if let Err(message) = typed_tool
            .input_type
            .validate_value_allowing_missing_nullable_fields(&Value::Object(input_arguments.clone()))
        {
            return Err(ExecutorError::Other {
                message: format!("deterministic tool call `{tool_name}` input is invalid: {message}"),
            });
        }

        let mut arguments = input_arguments;

        if let Some(binding_object) = bindings.as_object() {
            for (binding_name, binding_value) in binding_object {
                arguments.insert(binding_name.clone(), binding_value.clone());
            }
        }

        match source {
            ModelToolSource::Mcp {
                server_name,
                tool_name: mcp_tool_name,
                endpoint,
                headers,
            } => {
                let input_schema = ModelSchema::model_tool_input(typed_tool.input_type.clone(), bindings.clone());

                let server_config = McpServerConfig {
                    name: server_name.unwrap_or_else(|| "default".to_string()),
                    endpoint,
                    headers,
                };
                let call_details = McpCallEventDetails::new(
                    "call".to_string(),
                    tool_name.to_string(),
                    server_config.name.clone(),
                    mcp_tool_name.clone(),
                    Value::Object(arguments.clone()),
                    Some(input_schema.json_value()),
                );

                if let Some(sender) = tool_call_execution_context.event_sender {
                    let _ = sender.try_send(ExecutorEvent::mcp_call_started(call_details.clone()));
                }

                let started_at = Instant::now();
                log::info!("calling MCP tool `{mcp_tool_name}` for deterministic tool `{tool_name}`");
                let result = match self
                    .mcp_pool
                    .get(&server_config)?
                    .call_tool(&mcp_tool_name, Value::Object(arguments))
                {
                    Ok(result) => result,
                    Err(error) => {
                        if let Some(sender) = tool_call_execution_context.event_sender {
                            let _ = sender.try_send(ExecutorEvent::mcp_call_failed(
                                call_details,
                                Value::String(error.to_string()),
                                started_at.elapsed(),
                            ));
                        }

                        return Err(ExecutorError::Other {
                            message: format!("deterministic tool call `{tool_name}` failed: {error}"),
                        });
                    }
                };
                let normalized_result = normalize_mcp_tool_result(result.clone());
                let projected_result = typed_tool.output_type.project_json_value(&normalized_result);

                log::debug!("completed deterministic MCP tool `{tool_name}`");

                if let Some(sender) = tool_call_execution_context.event_sender {
                    let _ = sender.try_send(ExecutorEvent::mcp_call_completed(
                        call_details,
                        projected_result.clone(),
                        result,
                        started_at.elapsed(),
                    ));
                }

                Ok(projected_result)
            }
            ModelToolSource::Local => Err(ExecutorError::Other {
                message: format!("deterministic tool call `{tool_name}` is not backed by MCP"),
            }),
            ModelToolSource::McpPrompt { .. } | ModelToolSource::McpResource { .. } => Err(ExecutorError::Other {
                message: format!("deterministic tool call `{tool_name}` cannot target MCP prompts or resources"),
            }),
            ModelToolSource::Finalize => Err(ExecutorError::Other {
                message: format!("deterministic tool call `{tool_name}` cannot use internal finalize tool"),
            }),
        }
    }

    pub(super) fn resolve_agent_use_definitions(
        &self,
        planned_agent: &PlannedAgent,
        evaluation_context: &EvaluationContext,
    ) -> Result<Vec<ModelToolDefinition>, ExecutorError> {
        let Some(uses_expression) = planned_agent.declaration.expression_property(AgentExpressionPropertyName::Uses) else {
            return Ok(Vec::new());
        };
        let Expression::ArrayLiteral(use_expressions) = uses_expression else {
            return Err(ExecutorError::Other {
                message: format!("uses for agent `{}` must be an array", planned_agent.name),
            });
        };
        let mut tool_definitions = Vec::new();

        for use_expression in use_expressions {
            tool_definitions.push(self.resolve_agent_use_definition(use_expression, planned_agent, evaluation_context)?);
        }

        Ok(tool_definitions)
    }

    fn resolve_agent_use_definition(
        &self,
        use_expression: &Expression,
        planned_agent: &PlannedAgent,
        evaluation_context: &EvaluationContext,
    ) -> Result<ModelToolDefinition, ExecutorError> {
        let reference = match use_expression {
            Expression::Reference(reference) => reference,
            Expression::ToolCall(tool_call) => &tool_call.callee,
            _ => {
                return Err(ExecutorError::Other {
                    message: format!(
                        "uses for agent `{}` must contain tool, prompt, or resource references",
                        planned_agent.name
                    ),
                });
            }
        };

        match reference.root_keyword() {
            Some(ReferenceKeyword::Tool) => self.resolve_agent_tool_definition(use_expression, planned_agent, evaluation_context),
            Some(ReferenceKeyword::Prompt | ReferenceKeyword::Resource) => {
                let reference_keyword = reference.root_keyword().expect("reference keyword should exist");
                self.resolve_agent_mcp_import_tool_definition(use_expression, planned_agent, evaluation_context, reference_keyword)
            }
            _ => Err(ExecutorError::Other {
                message: format!(
                    "uses for agent `{}` must use tool, prompt, or resource references",
                    planned_agent.name
                ),
            }),
        }
    }

    fn resolve_agent_tool_definition(
        &self,
        tool_expression: &Expression,
        planned_agent: &PlannedAgent,
        evaluation_context: &EvaluationContext,
    ) -> Result<ModelToolDefinition, ExecutorError> {
        let (tool_reference, override_binding_fields, override_max_calls) = match tool_expression {
            Expression::Reference(reference) => (reference, &[] as &[ObjectField], None),
            Expression::ToolCall(tool_call) => (&tool_call.callee, tool_call.binding_fields.as_slice(), tool_call.max_calls),
            _ => {
                return Err(ExecutorError::Other {
                    message: format!("tools for agent `{}` must contain tool references", planned_agent.name),
                });
            }
        };
        let tool_name = tool_reference.tool_name().ok_or_else(|| ExecutorError::Other {
            message: format!("tools for agent `{}` must use `tool.<name>` references", planned_agent.name),
        })?;
        let typed_tool = self.execution_plan.tools.get(tool_name).ok_or_else(|| ExecutorError::Other {
            message: format!("agent `{}` references unknown tool `{tool_name}`", planned_agent.name),
        })?;
        let bindings = typed_tool.resolve_bindings(override_binding_fields, evaluation_context)?;
        log::debug!(
            "resolved tool `{}` for agent `{}`: binding_keys={}",
            typed_tool.name,
            planned_agent.name,
            bindings.as_object().map_or(0, serde_json::Map::len)
        );

        Ok(ModelToolDefinition {
            name: typed_tool.name.clone(),
            description: typed_tool.declaration.description.clone(),
            source: self.model_tool_source(&typed_tool.declaration, evaluation_context)?,
            input_schema: ModelSchema::model_tool_input(typed_tool.input_type.clone(), bindings.clone()),
            output_schema: ModelSchema::workflow(typed_tool.output_type.clone()),
            bindings,
            max_calls: override_max_calls.or(typed_tool.declaration.max_calls),
            max_calls_scope: if override_max_calls.is_some() {
                ToolCallLimitScope::Agent {
                    agent_name: planned_agent.name.clone(),
                }
            } else {
                ToolCallLimitScope::Workflow
            },
        })
    }

    #[allow(clippy::too_many_lines)]
    fn resolve_agent_mcp_import_tool_definition(
        &self,
        import_expression: &Expression,
        planned_agent: &PlannedAgent,
        evaluation_context: &EvaluationContext,
        reference_keyword: ReferenceKeyword,
    ) -> Result<ModelToolDefinition, ExecutorError> {
        let (reference, override_binding_fields, override_max_calls) = match import_expression {
            Expression::Reference(reference) => (reference, &[] as &[ObjectField], None),
            Expression::ToolCall(tool_call) => (&tool_call.callee, tool_call.binding_fields.as_slice(), tool_call.max_calls),
            _ => {
                return Err(ExecutorError::Other {
                    message: format!(
                        "{} for agent `{}` must contain {} references",
                        reference_keyword.as_str(),
                        planned_agent.name,
                        reference_keyword.as_str()
                    ),
                });
            }
        };
        let import_name = reference.import_name(reference_keyword).ok_or_else(|| ExecutorError::Other {
            message: format!(
                "{} for agent `{}` must use `{}.<name>` references",
                reference_keyword.as_str(),
                planned_agent.name,
                reference_keyword.as_str()
            ),
        })?;
        let (server_name, source_item_name, import_parameters) = match reference_keyword {
            ReferenceKeyword::Prompt => {
                let prompt_import = self.lookups.prompt_import(import_name).ok_or_else(|| ExecutorError::Other {
                    message: format!("agent `{}` references unknown prompt `{import_name}`", planned_agent.name),
                })?;

                (
                    prompt_import.source.server_name.clone(),
                    prompt_import.source.item_name.clone(),
                    prompt_import.parameters.as_slice(),
                )
            }
            ReferenceKeyword::Resource => {
                let resource_import = self.lookups.resource_import(import_name).ok_or_else(|| ExecutorError::Other {
                    message: format!("agent `{}` references unknown resource `{import_name}`", planned_agent.name),
                })?;

                (
                    resource_import.source.server_name.clone(),
                    resource_import.source.item_name.clone(),
                    resource_import.parameters.as_slice(),
                )
            }
            ReferenceKeyword::Tool
            | ReferenceKeyword::Agent
            | ReferenceKeyword::Dynamic
            | ReferenceKeyword::Input
            | ReferenceKeyword::Model
            | ReferenceKeyword::Secrets => {
                return Err(ExecutorError::Other {
                    message: format!("unsupported MCP import reference `{}`", reference_keyword.as_str()),
                });
            }
        };
        let bindings = self.resolve_mcp_import_bindings(import_parameters, override_binding_fields, evaluation_context, import_name)?;
        let server_config = self.resolve_mcp_import_server(&server_name, evaluation_context)?;
        let source = match reference_keyword {
            ReferenceKeyword::Prompt => ModelToolSource::McpPrompt {
                server_name,
                prompt_name: source_item_name,
                endpoint: server_config.endpoint,
                headers: server_config.headers,
            },
            ReferenceKeyword::Resource => ModelToolSource::McpResource {
                server_name,
                resource_name: source_item_name,
                endpoint: server_config.endpoint,
                headers: server_config.headers,
            },
            ReferenceKeyword::Tool
            | ReferenceKeyword::Agent
            | ReferenceKeyword::Dynamic
            | ReferenceKeyword::Input
            | ReferenceKeyword::Model
            | ReferenceKeyword::Secrets => {
                unreachable!("unsupported MCP import reference should return earlier")
            }
        };
        let tool_name = match reference_keyword {
            ReferenceKeyword::Prompt => format!("render_{import_name}"),
            ReferenceKeyword::Resource => format!("read_{import_name}"),
            ReferenceKeyword::Tool
            | ReferenceKeyword::Agent
            | ReferenceKeyword::Dynamic
            | ReferenceKeyword::Input
            | ReferenceKeyword::Model
            | ReferenceKeyword::Secrets => {
                unreachable!("unsupported MCP import reference should return earlier")
            }
        };

        Ok(ModelToolDefinition {
            name: tool_name,
            description: Some(format!(
                "{} MCP {} `{import_name}`",
                reference_keyword.as_str(),
                reference_keyword.as_str()
            )),
            source,
            input_schema: ModelSchema::OpenObject,
            output_schema: ModelSchema::workflow(WorkflowType::String),
            bindings,
            max_calls: override_max_calls,
            max_calls_scope: ToolCallLimitScope::Agent {
                agent_name: planned_agent.name.clone(),
            },
        })
    }

    fn model_tool_source(
        &self,
        tool_declaration: &superwire_dsl::ToolDeclaration,
        evaluation_context: &EvaluationContext,
    ) -> Result<ModelToolSource, ExecutorError> {
        let Some(ToolSource::Mcp(mcp_tool_source)) = &tool_declaration.source else {
            return Ok(ModelToolSource::Local);
        };
        let is_server_only_source = mcp_tool_source.server_name.is_none() && self.lookups.mcp_server(&mcp_tool_source.tool_name).is_some();
        let resolved_server_name = if is_server_only_source {
            Some(mcp_tool_source.tool_name.as_str())
        } else {
            mcp_tool_source.server_name.as_deref()
        };
        let mcp_server_declaration = if let Some(server_name) = resolved_server_name {
            self.lookups.mcp_server(server_name).ok_or_else(|| ExecutorError::Other {
                message: format!("tool `{}` references unknown MCP server `{server_name}`", tool_declaration.name),
            })?
        } else {
            self.lookups.default_mcp_server().ok_or_else(|| ExecutorError::Other {
                message: format!("tool `{}` uses MCP but no `mcp` server is declared", tool_declaration.name),
            })?
        };
        let mcp_server_config = McpServerConfig::resolve_from_declaration(mcp_server_declaration, evaluation_context).map_err(|error| {
            ExecutorError::Other {
                message: error.to_string(),
            }
        })?;

        Ok(ModelToolSource::Mcp {
            server_name: resolved_server_name.map(str::to_string),
            tool_name: if is_server_only_source {
                tool_declaration.name.clone()
            } else {
                mcp_tool_source.tool_name.clone()
            },
            endpoint: mcp_server_config.endpoint,
            headers: mcp_server_config.headers,
        })
    }
}

trait WorkflowStartupToolCallsExt {
    fn startup_tool_calls(&self) -> Vec<&ToolCall>;
}

impl WorkflowStartupToolCallsExt for Workflow {
    fn startup_tool_calls(&self) -> Vec<&ToolCall> {
        let mut tool_calls = Vec::new();

        for declaration in self.declarations() {
            let Declaration::Dynamic(dynamic_block) = declaration else {
                continue;
            };

            for dynamic_field in &dynamic_block.fields {
                tool_calls.extend(dynamic_field.value.tool_calls());
            }
        }

        tool_calls
    }
}

pub(super) trait ExpressionMcpExecutionPlanExt {
    fn planned_mcp_calls(&self, executor: &WorkflowExecutor, evaluation_context: &EvaluationContext) -> Result<Vec<Value>, ExecutorError>;
}

impl ExpressionMcpExecutionPlanExt for Expression {
    fn planned_mcp_calls(&self, executor: &WorkflowExecutor, evaluation_context: &EvaluationContext) -> Result<Vec<Value>, ExecutorError> {
        let mut planned_calls = Vec::new();
        self.collect_planned_mcp_calls(executor, evaluation_context, &mut planned_calls)?;

        Ok(planned_calls)
    }
}

trait ExpressionMcpExecutionPlanCollectorExt {
    fn collect_planned_mcp_calls(
        &self,
        executor: &WorkflowExecutor,
        evaluation_context: &EvaluationContext,
        planned_calls: &mut Vec<Value>,
    ) -> Result<(), ExecutorError>;
}

impl ExpressionMcpExecutionPlanCollectorExt for Expression {
    fn collect_planned_mcp_calls(
        &self,
        executor: &WorkflowExecutor,
        evaluation_context: &EvaluationContext,
        planned_calls: &mut Vec<Value>,
    ) -> Result<(), ExecutorError> {
        match self {
            Self::ToolCall(tool_call) => {
                if let Some(tool_name) = tool_call.callee.tool_name() {
                    if let Some(planned_call) = executor.planned_mcp_tool_call(tool_name, evaluation_context)? {
                        planned_calls.push(planned_call);
                    }
                }

                for input_field in &tool_call.input_fields {
                    input_field
                        .value
                        .collect_planned_mcp_calls(executor, evaluation_context, planned_calls)?;
                }

                for binding_field in &tool_call.binding_fields {
                    binding_field
                        .value
                        .collect_planned_mcp_calls(executor, evaluation_context, planned_calls)?;
                }
            }
            Self::McpCall(mcp_call) => {
                if let Some(planned_call) = executor.planned_mcp_import_call(mcp_call) {
                    planned_calls.push(planned_call);
                }

                for parameter_field in &mcp_call.parameter_fields {
                    parameter_field
                        .value
                        .collect_planned_mcp_calls(executor, evaluation_context, planned_calls)?;
                }
            }
            Self::StringTemplate(string_template) => {
                for string_template_part in &string_template.parts {
                    if let superwire_dsl::StringTemplatePart::Interpolation(interpolation_expression) = string_template_part {
                        interpolation_expression.collect_planned_mcp_calls(executor, evaluation_context, planned_calls)?;
                    }
                }
            }
            Self::FunctionCall(function_call) => {
                for call_argument in &function_call.arguments {
                    call_argument
                        .expression()
                        .collect_planned_mcp_calls(executor, evaluation_context, planned_calls)?;
                }
            }
            Self::Asset(asset) => {
                asset
                    .source
                    .collect_planned_mcp_calls(executor, evaluation_context, planned_calls)?;

                for option in &asset.options {
                    option
                        .value
                        .collect_planned_mcp_calls(executor, evaluation_context, planned_calls)?;
                }
            }
            Self::NullFallback(null_fallback) => {
                null_fallback
                    .value
                    .collect_planned_mcp_calls(executor, evaluation_context, planned_calls)?;
                null_fallback
                    .fallback
                    .collect_planned_mcp_calls(executor, evaluation_context, planned_calls)?;
            }
            Self::Match(match_expression) => {
                match_expression
                    .value
                    .collect_planned_mcp_calls(executor, evaluation_context, planned_calls)?;

                for match_branch in &match_expression.branches {
                    if let superwire_dsl::MatchBranch::Fallback { value, .. } = match_branch {
                        value.collect_planned_mcp_calls(executor, evaluation_context, planned_calls)?;
                    }
                }
            }
            Self::ArrayLiteral(item_expressions) => {
                for item_expression in item_expressions {
                    item_expression.collect_planned_mcp_calls(executor, evaluation_context, planned_calls)?;
                }
            }
            Self::ObjectLiteral(object_fields) => {
                for object_field in object_fields {
                    object_field
                        .value
                        .collect_planned_mcp_calls(executor, evaluation_context, planned_calls)?;
                }
            }
            Self::NumberLiteral(_)
            | Self::BooleanLiteral(_)
            | Self::NullLiteral
            | Self::StringLiteral(_)
            | Self::Reference(_)
            | Self::VariantProjection(_) => {}
        }

        Ok(())
    }
}

trait TypedToolRuntimeExt {
    fn resolve_bindings(
        &self,
        override_binding_fields: &[ObjectField],
        evaluation_context: &EvaluationContext,
    ) -> Result<Value, ExecutorError>;
}

impl TypedToolRuntimeExt for TypedToolIr {
    fn resolve_bindings(
        &self,
        override_binding_fields: &[ObjectField],
        evaluation_context: &EvaluationContext,
    ) -> Result<Value, ExecutorError> {
        let mut binding_values = Map::new();

        for fixed_binding_field in &self.declaration.fixed_binding_fields {
            let binding_value = evaluate_expression(
                &fixed_binding_field.value,
                evaluation_context,
                &format!("fixed binding `{}` for tool `{}`", fixed_binding_field.name, self.name),
            )?;
            binding_values.insert(fixed_binding_field.name.clone(), binding_value);
        }

        let mut typed_binding_values = Map::new();

        for override_binding_field in override_binding_fields {
            let binding_value = evaluate_expression(
                &override_binding_field.value,
                evaluation_context,
                &format!("binding `{}` for tool `{}`", override_binding_field.name, self.name),
            )?;
            binding_values.insert(override_binding_field.name.clone(), binding_value.clone());
            typed_binding_values.insert(override_binding_field.name.clone(), binding_value);
        }

        self.binding_type
            .validate_value_allowing_missing_nullable_fields(&Value::Object(typed_binding_values))
            .map_err(|message| ExecutorError::Other {
                message: format!("tool `{}` binding values are invalid: {message}", self.name),
            })?;

        Ok(Value::Object(binding_values))
    }
}
