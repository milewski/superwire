use crate::error::WorkflowError;
use engine_ai_agent::{
    validate_json_against_schema_with_context, AgentConfig, Context, Provider, ProviderError, ProviderResponse, RuntimeTool, StopReason,
    ToolDefinition, ToolError, ToolResult,
};
use futures::future::join_all;
use schemars::Schema;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

type ToolRegistry<'a> = HashMap<String, &'a Arc<dyn RuntimeTool>>;

#[derive(Debug, Clone)]
pub(crate) struct DynamicExecutor {
    finalize_schema: Schema,
    max_iterations: usize,
}

enum ToolCallExecution<'a> {
    Complete(&'a engine_ai_agent::ToolCall),
    Continue {
        ignored_finalize_calls: Vec<&'a engine_ai_agent::ToolCall>,
        tool_calls: Vec<&'a engine_ai_agent::ToolCall>,
    },
}

impl DynamicExecutor {
    pub(crate) fn new(output_schema: Schema) -> Result<Self, WorkflowError> {
        Ok(Self {
            finalize_schema: build_finalize_schema(output_schema)?,
            max_iterations: 5,
        })
    }

    pub(crate) async fn execute(
        &self,
        context: &mut Context,
        provider: &(dyn Provider + Send + Sync),
        tools: &[Arc<dyn RuntimeTool>],
        config: &AgentConfig,
    ) -> Result<Value, WorkflowError> {
        let (tool_definitions, tool_registry) = self.prepare_tools(tools)?;
        let mut iteration = 0;

        loop {
            if iteration >= self.max_iterations {
                return Err(WorkflowError::execution(format!(
                    "maximum iterations ({}) reached without calling finalize",
                    self.max_iterations
                )));
            }

            let response = self.generate_with_retry(context, provider, &tool_definitions, config).await?;

            if let Some(token_usage) = response.usage {
                context.add_token_usage(token_usage);
            }

            if let Some(text) = &response.text {
                let trimmed_text = text.trim();

                if !trimmed_text.is_empty() {
                    context.add_assistant_message(trimmed_text);
                }
            }

            if context.is_stuck(config.stuck_threshold) {
                return Err(WorkflowError::execution("agent entered a repeated loop"));
            }

            if response.stop_reason == StopReason::MaxTokens {
                return Err(WorkflowError::execution("provider reached its maximum token limit"));
            }

            if response.stop_reason == StopReason::EndOfSequence {
                context.add_user_message("You must finish by calling the 'finalize' tool.");
            }

            if response.tool_calls.is_empty() {
                iteration += 1;
                continue;
            }

            for tool_call in &response.tool_calls {
                context.add_tool_call(tool_call.clone());
            }

            match self.classify_tool_calls(&response) {
                ToolCallExecution::Complete(finalize_tool_call) => {
                    if let Some(output) = self.process_finalize_tool_call(context, finalize_tool_call)? {
                        return Ok(output);
                    }
                }
                ToolCallExecution::Continue {
                    ignored_finalize_calls,
                    tool_calls,
                } => {
                    for finalize_tool_call in ignored_finalize_calls {
                        context.add_tool_result(ToolResult::Failure {
                            tool_call_id: finalize_tool_call.id.clone(),
                            content: Value::String("The 'finalize' tool must be called by itself to complete execution.".to_string()),
                        });
                    }

                    let mut execution_futures = Vec::new();

                    for tool_call in tool_calls {
                        match tool_registry.get(&tool_call.name) {
                            Some(tool) => {
                                execution_futures.push(async move { (tool_call, tool.execute(tool_call.arguments.clone()).await) });
                            }
                            None => {
                                context.add_tool_result(ToolResult::Failure {
                                    tool_call_id: tool_call.id.clone(),
                                    content: Value::String(format!("unknown tool '{}'", tool_call.name)),
                                });
                            }
                        }
                    }

                    for (tool_call, execution_result) in join_all(execution_futures).await {
                        let tool_result = match execution_result {
                            Ok(content) => ToolResult::Success {
                                tool_call_id: tool_call.id.clone(),
                                content,
                            },
                            Err(error) => ToolResult::Failure {
                                tool_call_id: tool_call.id.clone(),
                                content: Value::String(error.to_string()),
                            },
                        };

                        context.add_tool_result(tool_result);
                    }
                }
            }

            iteration += 1;
        }
    }

    fn prepare_tools<'a>(&self, tools: &'a [Arc<dyn RuntimeTool>]) -> Result<(Vec<ToolDefinition>, ToolRegistry<'a>), WorkflowError> {
        let mut tool_definitions = Vec::with_capacity(tools.len() + 1);
        let mut tool_registry = HashMap::with_capacity(tools.len());

        for tool in tools {
            let definition = tool.definition().map_err(tool_error_to_execution)?;

            if definition.name == "finalize" {
                return Err(WorkflowError::execution("tool name 'finalize' is reserved"));
            }

            if tool_registry.contains_key(&definition.name) {
                return Err(WorkflowError::execution(format!("duplicate runtime tool '{}'", definition.name)));
            }

            tool_registry.insert(definition.name.clone(), tool);
            tool_definitions.push(definition);
        }

        tool_definitions.push(ToolDefinition {
            name: "finalize".to_string(),
            description: finalize_tool_description().to_string(),
            parameters_schema: self.finalize_schema.clone(),
        });

        Ok((tool_definitions, tool_registry))
    }

    fn classify_tool_calls<'a>(&self, response: &'a ProviderResponse) -> ToolCallExecution<'a> {
        let mut finalize_tool_calls = Vec::new();
        let mut other_tool_calls = Vec::new();

        for tool_call in &response.tool_calls {
            if tool_call.name == "finalize" {
                finalize_tool_calls.push(tool_call);
            } else {
                other_tool_calls.push(tool_call);
            }
        }

        if other_tool_calls.is_empty() {
            if let Some(finalize_tool_call) = finalize_tool_calls.last() {
                return ToolCallExecution::Complete(finalize_tool_call);
            }
        }

        ToolCallExecution::Continue {
            ignored_finalize_calls: finalize_tool_calls,
            tool_calls: other_tool_calls,
        }
    }

    fn process_finalize_tool_call(
        &self,
        context: &mut Context,
        tool_call: &engine_ai_agent::ToolCall,
    ) -> Result<Option<Value>, WorkflowError> {
        if let Err(error) = validate_json_against_schema_with_context(
            &tool_call.arguments,
            &self.finalize_schema,
            "Finalize tool arguments do not match schema",
        ) {
            context.add_tool_result(ToolResult::Failure {
                tool_call_id: tool_call.id.clone(),
                content: Value::String(error.to_string()),
            });

            return Ok(None);
        }

        let output_object = tool_call
            .arguments
            .get("output")
            .and_then(Value::as_object)
            .expect("validated finalize payload should contain an output object");
        let output_type = output_object
            .get("type")
            .and_then(Value::as_str)
            .expect("validated finalize payload should contain an output type");

        match output_type {
            "success" => {
                let answer = output_object
                    .get("answer")
                    .cloned()
                    .expect("validated success payload should include an answer");

                context.add_tool_result(ToolResult::Success {
                    tool_call_id: tool_call.id.clone(),
                    content: answer.clone(),
                });

                Ok(Some(answer))
            }
            "failure" => {
                let reason = output_object
                    .get("reason")
                    .and_then(Value::as_str)
                    .expect("validated failure payload should include a reason");

                context.add_tool_result(ToolResult::Failure {
                    tool_call_id: tool_call.id.clone(),
                    content: Value::String(reason.to_string()),
                });

                Err(WorkflowError::execution(format!("agent reported finalize failure: {reason}")))
            }
            _ => unreachable!("finalize schema validation should prevent unsupported finalize states"),
        }
    }

    async fn generate_with_retry(
        &self,
        context: &Context,
        provider: &(dyn Provider + Send + Sync),
        tools: &[ToolDefinition],
        config: &AgentConfig,
    ) -> Result<ProviderResponse, WorkflowError> {
        let mut attempt = 0;

        loop {
            match provider.generate(context, tools, config).await {
                Ok(response) => return Ok(response),
                Err(error) if error.is_retriable() && attempt < config.provider_max_retries => {
                    sleep(retry_delay_for_provider_error(&error, attempt, config.provider_retry_base_delay_ms)).await;
                    attempt += 1;
                }
                Err(error) => return Err(WorkflowError::execution(format!("provider error: {error}"))),
            }
        }
    }
}

fn tool_error_to_execution(error: ToolError) -> WorkflowError {
    WorkflowError::execution(error.to_string())
}

fn retry_delay_for_provider_error(error: &ProviderError, attempt: usize, base_delay_ms: u64) -> Duration {
    if let ProviderError::RateLimited {
        message: _,
        retry_after_seconds: Some(retry_after_seconds),
    } = error
    {
        return Duration::from_secs(*retry_after_seconds);
    }

    let growth_factor = 2_u64.saturating_pow(match u32::try_from(attempt) {
        Ok(attempt_count) => attempt_count,
        Err(_) => u32::MAX,
    });
    Duration::from_millis(base_delay_ms.saturating_mul(growth_factor).min(30_000))
}

fn build_finalize_schema(output_schema: Schema) -> Result<Schema, WorkflowError> {
    serde_json::from_value(json!({
        "type": "object",
        "required": ["output"],
        "additionalProperties": false,
        "properties": {
            "output": {
                "oneOf": [
                    {
                        "type": "object",
                        "required": ["type", "answer"],
                        "additionalProperties": false,
                        "properties": {
                            "type": {
                                "const": "success"
                            },
                            "answer": serde_json::to_value(output_schema)
                                .map_err(|error| WorkflowError::schema(format!("failed to serialize output schema: {error}")))?
                        }
                    },
                    {
                        "type": "object",
                        "required": ["type", "reason"],
                        "additionalProperties": false,
                        "properties": {
                            "type": {
                                "const": "failure"
                            },
                            "reason": {
                                "type": "string"
                            }
                        }
                    }
                ]
            }
        }
    }))
    .map_err(|error| WorkflowError::schema(format!("failed to build finalize schema: {error}")))
}

fn finalize_tool_description() -> &'static str {
    r#"
        Call this tool only when you are done.
        Arguments MUST be exactly one of:
            { "output": { "type": "success", "answer": <final_json_object> } }
            { "output": { "type": "failure", "reason": "<why you could not complete>" } }

        Required success keys: output.type and output.answer.
        Required failure keys: output.type and output.reason.
    "#
}

#[cfg(test)]
mod tests {
    use super::DynamicExecutor;
    use crate::compiler::build_type_schema;
    use engine_ai_agent::{AgentConfig, Context, Provider, ProviderResponse, StopReason};
    use serde_json::json;

    #[derive(Debug)]
    struct StaticProvider {
        response: ProviderResponse,
    }

    #[async_trait::async_trait]
    impl Provider for StaticProvider {
        async fn generate(
            &self,
            _context: &Context,
            _tools: &[engine_ai_agent::ToolDefinition],
            _config: &AgentConfig,
        ) -> Result<ProviderResponse, engine_ai_agent::ProviderError> {
            Ok(self.response.clone())
        }
    }

    #[tokio::test]
    async fn accepts_valid_runtime_finalize_payloads() {
        let schema = build_type_schema(
            &crate::ast::TypeExpression::Primitive(crate::ast::PrimitiveType::String),
            &std::collections::BTreeMap::new(),
        )
        .expect("schema should build");
        let executor = DynamicExecutor::new(schema).expect("executor should build");
        let provider = StaticProvider {
            response: ProviderResponse {
                tool_calls: vec![engine_ai_agent::ToolCall {
                    id: "call-1".to_string(),
                    name: "finalize".to_string(),
                    arguments: json!({
                        "output": {
                            "type": "success",
                            "answer": "done"
                        }
                    }),
                }],
                text: None,
                stop_reason: StopReason::ToolCalls,
                usage: None,
            },
        };
        let mut context = Context::new();
        let output = executor
            .execute(&mut context, &provider, &[], &AgentConfig::default())
            .await
            .expect("execution should succeed");

        assert_eq!(output, json!("done"));
    }
}
