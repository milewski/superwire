use crate::runtime::{DynamicProvider, ScriptedProviderFactory, WorkflowProviderFactory, WorkflowRuntime, WorkflowRuntimeError};
use async_trait::async_trait;
use engine_ai_agent::{Context, Message, Provider, ProviderError, ProviderResponse, StopReason, ToolCall, ToolDefinition};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn executes_existing_minimum_workflow_with_scripted_provider() {
    let workflow_source = include_str!("../../workflows/minimum.ai");
    let mut outputs_by_agent_name = HashMap::<String, Value>::new();

    outputs_by_agent_name.insert("greeting".to_owned(), Value::String("Hello from scripted runtime".to_owned()));

    let workflow_runtime = WorkflowRuntime::new(ScriptedProviderFactory::new(outputs_by_agent_name));

    let execution_result = workflow_runtime
        .execute_source(workflow_source, json!({}), json!({}))
        .await
        .expect("minimum workflow should execute successfully");

    assert_eq!(
        execution_result.output,
        json!({
            "greeting": "Hello from scripted runtime"
        })
    );
}

#[tokio::test]
async fn resolves_dependencies_and_interpolates_prompt_references() {
    let workflow_source = r#"
            provider scripted {
                driver: "scripted"
                models: ["mock-model"]
            }

            input {
                subject: string
            }

            agent first {
                model: scripted("mock-model")
                prompt: "First prompt: {{ input.subject }}"
                output: {
                    summary: string
                }
            }

            agent second {
                model: scripted("mock-model")
                prompt: "Second prompt: {{ agent.first.summary }}"
                output: string
            }

            output {
                first_summary: agent.first.summary
                second_text: agent.second
            }
        "#;

    let mut outputs_by_agent_name = HashMap::<String, Value>::new();

    outputs_by_agent_name.insert(
        "first".to_owned(),
        json!({
            "summary": "summary-from-first"
        }),
    );
    outputs_by_agent_name.insert("second".to_owned(), Value::String("answer-from-second".to_owned()));

    let recorded_prompts = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let workflow_runtime = WorkflowRuntime::new(PromptRecordingProviderFactory::new(
        outputs_by_agent_name,
        Arc::clone(&recorded_prompts),
    ));

    let execution_result = workflow_runtime
        .execute_source(
            workflow_source,
            json!({
                "subject": "engine-ai"
            }),
            json!({}),
        )
        .await
        .expect("workflow should execute successfully");

    assert_eq!(
        execution_result.output,
        json!({
            "first_summary": "summary-from-first",
            "second_text": "answer-from-second"
        })
    );

    let recorded_prompts = recorded_prompts
        .lock()
        .expect("prompt recorder mutex should not be poisoned")
        .clone();

    assert_eq!(recorded_prompts.len(), 2);
    assert_eq!(recorded_prompts[0], ("first".to_owned(), "First prompt: engine-ai".to_owned()));
    assert_eq!(
        recorded_prompts[1],
        ("second".to_owned(), "Second prompt: summary-from-first".to_owned())
    );
}

#[tokio::test]
async fn reports_agent_output_type_mismatch() {
    let workflow_source = r#"
            provider scripted {
                driver: "scripted"
                models: ["mock-model"]
            }

            agent first {
                model: scripted("mock-model")
                prompt: "irrelevant"
                output: {
                    score: number
                }
            }

            output {
                result: agent.first
            }
        "#;

    let mut outputs_by_agent_name = HashMap::<String, Value>::new();

    outputs_by_agent_name.insert("first".to_owned(), Value::String("this should have been an object".to_owned()));

    let workflow_runtime = WorkflowRuntime::new(ScriptedProviderFactory::new(outputs_by_agent_name));

    let execution_error = workflow_runtime
        .execute_source(workflow_source, json!({}), json!({}))
        .await
        .expect_err("runtime should reject mismatched output type");

    assert!(matches!(
        execution_error,
        WorkflowRuntimeError::AgentOutputTypeMismatch {
            agent_name,
            message: _
        } if agent_name == "first"
    ));
}

#[derive(Debug, Clone)]
struct PromptRecordingProviderFactory {
    outputs_by_agent_name: HashMap<String, Value>,
    recorded_prompts: Arc<Mutex<Vec<(String, String)>>>,
}

impl PromptRecordingProviderFactory {
    fn new(outputs_by_agent_name: HashMap<String, Value>, recorded_prompts: Arc<Mutex<Vec<(String, String)>>>) -> Self {
        Self {
            outputs_by_agent_name,
            recorded_prompts,
        }
    }
}

impl WorkflowProviderFactory for PromptRecordingProviderFactory {
    fn build_provider(
        &self,
        agent_name: &str,
        _provider_name: &str,
        _provider_settings: &Map<String, Value>,
        _model_name: &str,
    ) -> Result<DynamicProvider, WorkflowRuntimeError> {
        let output_value =
            self.outputs_by_agent_name
                .get(agent_name)
                .cloned()
                .ok_or_else(|| WorkflowRuntimeError::ProviderFactoryFailed {
                    message: format!("missing scripted output for '{agent_name}'"),
                })?;

        Ok(DynamicProvider::new(PromptRecordingProvider {
            agent_name: agent_name.to_owned(),
            output_value,
            recorded_prompts: Arc::clone(&self.recorded_prompts),
        }))
    }
}

#[derive(Debug, Clone)]
struct PromptRecordingProvider {
    agent_name: String,
    output_value: Value,
    recorded_prompts: Arc<Mutex<Vec<(String, String)>>>,
}

#[async_trait]
impl Provider for PromptRecordingProvider {
    async fn generate(
        &self,
        context: &Context,
        _tools: &[ToolDefinition],
        _config: &engine_ai_agent::AgentConfig,
    ) -> Result<ProviderResponse, ProviderError> {
        let prompt_text = context
            .messages
            .iter()
            .rev()
            .find_map(|message| match message {
                Message::User { content } => Some(content.clone()),
                _ => None,
            })
            .unwrap_or_default();

        self.recorded_prompts
            .lock()
            .expect("prompt recorder mutex should not be poisoned")
            .push((self.agent_name.clone(), prompt_text));

        let finalize_tool_call = ToolCall {
            id: format!("{}-finalize", self.agent_name),
            name: "finalize".to_owned(),
            arguments: json!({
                "output": {
                    "type": "success",
                    "answer": self.output_value.clone()
                }
            }),
        };

        Ok(ProviderResponse {
            tool_calls: vec![finalize_tool_call],
            text: None,
            stop_reason: StopReason::ToolCalls,
            usage: None,
        })
    }
}
