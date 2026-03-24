use crate::ast::{CompactArgument, Expression, FunctionExpression, InferenceProperty, ReferenceExpression, ReferenceRoot};
use crate::compiler::{build_type_schema, CompiledAgent, CompiledProvider, CompiledWorkflow};
use crate::error::WorkflowError;
use crate::runtime::dynamic_executor::DynamicExecutor;
use crate::runtime::interpolation::render_prompt;
use crate::runtime::provider_factory::{DefaultProviderFactory, ProviderFactory};
use crate::runtime::tool_binding::BoundRuntimeTool;
use crate::runtime::value::{evaluate_plain_expression, render_inline_string, serialize_context_value};
use crate::runtime::{StoredContext, WorkflowAgentResult, WorkflowState};
use engine_ai_agent::{validate_json_against_schema_with_context, AgentConfig, Context, RuntimeTool};
use futures::future::{join_all, BoxFuture};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Debug)]
pub struct WorkflowRunner<Factory = DefaultProviderFactory> {
    provider_factory: Factory,
    secrets: BTreeMap<String, Value>,
    tools: BTreeMap<String, Arc<dyn RuntimeTool>>,
}

impl Default for WorkflowRunner<DefaultProviderFactory> {
    fn default() -> Self {
        Self {
            provider_factory: DefaultProviderFactory,
            secrets: BTreeMap::new(),
            tools: BTreeMap::new(),
        }
    }
}

impl<Factory> WorkflowRunner<Factory>
where
    Factory: ProviderFactory,
{
    #[must_use]
    pub fn new(provider_factory: Factory) -> Self {
        Self {
            provider_factory,
            secrets: BTreeMap::new(),
            tools: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn with_runtime_tool(mut self, tool_name: impl Into<String>, tool: Arc<dyn RuntimeTool>) -> Self {
        self.tools.insert(tool_name.into(), tool);
        self
    }

    #[must_use]
    pub fn with_secret_json(mut self, secret_name: impl Into<String>, secret_value: Value) -> Self {
        self.secrets.insert(secret_name.into(), secret_value);
        self
    }

    pub fn with_serialized_secret(mut self, secret_name: impl Into<String>, secret_value: impl Serialize) -> Result<Self, WorkflowError> {
        let serialized_secret = serde_json::to_value(secret_value)
            .map_err(|error| WorkflowError::execution(format!("failed to serialize secret value: {error}")))?;
        self.secrets.insert(secret_name.into(), serialized_secret);

        Ok(self)
    }

    pub async fn run<OutputType, InputType>(&self, workflow: &CompiledWorkflow, inputs: InputType) -> Result<OutputType, WorkflowError>
    where
        OutputType: DeserializeOwned,
        InputType: Serialize,
    {
        let input_value = serde_json::to_value(inputs)
            .map_err(|error| WorkflowError::execution(format!("failed to serialize workflow inputs: {error}")))?;
        self.validate_inputs_and_secrets(workflow, &input_value)?;

        let mut workflow_state = WorkflowState {
            agent_results: BTreeMap::new(),
            inputs: input_value,
            secrets: self.secrets.clone(),
        };

        for stage in workflow.dependency_graph.parallel_stages() {
            let state_snapshot = workflow_state.clone();
            let stage_futures = stage.iter().map(|agent_name| {
                let state_snapshot = state_snapshot.clone();

                async move {
                    let agent = find_agent(workflow, agent_name)?;
                    let agent_result = self.execute_agent(workflow, &state_snapshot, agent).await?;
                    Ok::<(String, WorkflowAgentResult), WorkflowError>((agent_name.clone(), agent_result))
                }
            });

            for stage_result in join_all(stage_futures).await {
                let (agent_name, agent_result) = stage_result?;
                workflow_state.agent_results.insert(agent_name, agent_result);
            }
        }

        let final_output = self.evaluate_output_object(workflow, &workflow_state, &BTreeMap::new()).await?;

        serde_json::from_value(final_output)
            .map_err(|error| WorkflowError::execution(format!("failed to deserialize workflow output into requested type: {error}")))
    }

    async fn execute_agent(
        &self,
        workflow: &CompiledWorkflow,
        state: &WorkflowState,
        agent: &CompiledAgent,
    ) -> Result<WorkflowAgentResult, WorkflowError> {
        if let Some(for_loop) = &agent.for_loop {
            let loop_source_value = evaluate_plain_expression(&for_loop.source, workflow, state, &BTreeMap::new())?;
            let loop_items = loop_source_value
                .as_array()
                .cloned()
                .ok_or_else(|| WorkflowError::execution(format!("agent '{}' loop source did not evaluate to an array", agent.name)))?;

            let iteration_futures = loop_items.into_iter().map(|loop_item| async move {
                let local_values = BTreeMap::from([(for_loop.item_name.clone(), loop_item)]);
                self.execute_agent_iteration(workflow, state, agent, &local_values).await
            });
            let mut outputs = Vec::new();
            let mut contexts = Vec::new();

            for iteration_result in join_all(iteration_futures).await {
                let (output, context) = iteration_result?;
                outputs.push(output);
                contexts.push(context);
            }

            Ok(WorkflowAgentResult {
                context: StoredContext::Many(contexts),
                model: agent.model.clone(),
                output: Value::Array(outputs),
            })
        } else {
            let (output, context) = self.execute_agent_iteration(workflow, state, agent, &BTreeMap::new()).await?;

            Ok(WorkflowAgentResult {
                context: StoredContext::Single(context),
                model: agent.model.clone(),
                output,
            })
        }
    }

    async fn execute_agent_iteration(
        &self,
        workflow: &CompiledWorkflow,
        state: &WorkflowState,
        agent: &CompiledAgent,
        local_values: &BTreeMap<String, Value>,
    ) -> Result<(Value, Context), WorkflowError> {
        let provider = self.build_provider(workflow, state, agent)?;
        let runtime_tools = self.build_runtime_tools(workflow, state, agent, local_values)?;
        let mut context = self.build_initial_context(workflow, state, agent, local_values).await?;
        let prompt = render_prompt(&agent.prompt, workflow, state, local_values)?;
        let config = build_agent_config(&agent.inference)?;
        let output_schema = build_type_schema(&agent.output_type, &workflow.schemas)?;
        let executor = DynamicExecutor::new(output_schema)?;

        context.add_user_message(prompt);

        let output = executor.execute(&mut context, provider.as_ref(), &runtime_tools, &config).await?;

        Ok((output, context))
    }

    async fn build_initial_context(
        &self,
        workflow: &CompiledWorkflow,
        state: &WorkflowState,
        agent: &CompiledAgent,
        local_values: &BTreeMap<String, Value>,
    ) -> Result<Context, WorkflowError> {
        let Some(context_expression) = &agent.context else {
            return Ok(Context::new());
        };

        match context_expression {
            Expression::Function(FunctionExpression::Context(reference)) => clone_context(reference, state),
            Expression::Function(FunctionExpression::Compact(_)) => {
                let compacted_value = self
                    .evaluate_output_expression(context_expression, workflow, state, local_values)
                    .await?;
                let compacted_text = compacted_value
                    .as_str()
                    .ok_or_else(|| WorkflowError::execution("compact(...) used as agent context must resolve to a single string"))?;
                let mut compacted_context = Context::new();
                compacted_context.add_system_message(compacted_text.to_string());

                Ok(compacted_context)
            }
            _ => Err(WorkflowError::execution(
                "agent context must be a context(...) or compact(...) function",
            )),
        }
    }

    fn build_provider(
        &self,
        workflow: &CompiledWorkflow,
        state: &WorkflowState,
        agent: &CompiledAgent,
    ) -> Result<Arc<dyn engine_ai_agent::Provider + Send + Sync>, WorkflowError> {
        let provider = workflow
            .providers
            .get(&agent.model.provider_name)
            .ok_or_else(|| WorkflowError::execution(format!("provider '{}' is not available", agent.model.provider_name)))?;
        let api_key = resolve_provider_api_key(provider, &state.secrets)?;

        self.provider_factory
            .build_provider(&agent.name, provider, &agent.model.model_name, api_key.as_deref())
    }

    fn build_runtime_tools(
        &self,
        workflow: &CompiledWorkflow,
        state: &WorkflowState,
        agent: &CompiledAgent,
        local_values: &BTreeMap<String, Value>,
    ) -> Result<Vec<Arc<dyn RuntimeTool>>, WorkflowError> {
        let mut runtime_tools = Vec::new();

        for tool_usage in &agent.tools {
            let wrapped_tool = self
                .tools
                .get(&tool_usage.name)
                .ok_or_else(|| WorkflowError::execution(format!("workflow runtime tool '{}' was not provided", tool_usage.name)))?;
            let mut bound_arguments = BTreeMap::new();

            for binding in &tool_usage.arguments {
                bound_arguments.insert(
                    binding.name.clone(),
                    evaluate_plain_expression(&binding.value, workflow, state, local_values)?,
                );
            }

            runtime_tools.push(Arc::new(BoundRuntimeTool::new(
                tool_usage.name.clone(),
                Arc::clone(wrapped_tool),
                bound_arguments,
            )) as Arc<dyn RuntimeTool>);
        }

        Ok(runtime_tools)
    }

    fn evaluate_output_expression<'a>(
        &'a self,
        expression: &'a Expression,
        workflow: &'a CompiledWorkflow,
        state: &'a WorkflowState,
        local_values: &'a BTreeMap<String, Value>,
    ) -> BoxFuture<'a, Result<Value, WorkflowError>> {
        Box::pin(async move {
            match expression {
                Expression::Array(items) => {
                    let mut values = Vec::new();

                    for item in items {
                        values.push(self.evaluate_output_expression(item, workflow, state, local_values).await?);
                    }

                    Ok(Value::Array(values))
                }
                Expression::Function(FunctionExpression::Compact(compact_expression)) => {
                    self.evaluate_compact_expression(workflow, state, compact_expression, local_values)
                        .await
                }
                Expression::Object(fields) => {
                    let mut object_value = Map::new();

                    for field in fields {
                        object_value.insert(
                            field.name.clone(),
                            self.evaluate_output_expression(&field.value, workflow, state, local_values).await?,
                        );
                    }

                    Ok(Value::Object(object_value))
                }
                _ => evaluate_plain_expression(expression, workflow, state, local_values),
            }
        })
    }

    async fn evaluate_output_object(
        &self,
        workflow: &CompiledWorkflow,
        state: &WorkflowState,
        local_values: &BTreeMap<String, Value>,
    ) -> Result<Value, WorkflowError> {
        let mut object_value = Map::new();

        for output_field in &workflow.output_fields {
            object_value.insert(
                output_field.name.clone(),
                self.evaluate_output_expression(&output_field.value, workflow, state, local_values)
                    .await?,
            );
        }

        Ok(Value::Object(object_value))
    }

    async fn evaluate_compact_expression(
        &self,
        workflow: &CompiledWorkflow,
        state: &WorkflowState,
        compact_expression: &crate::ast::CompactExpression,
        local_values: &BTreeMap<String, Value>,
    ) -> Result<Value, WorkflowError> {
        let compact_parts = parse_compact_parts(compact_expression)?;
        let ReferenceRoot::Agent(source_agent_name) = &compact_parts.source_reference.root else {
            unreachable!("validator should restrict compact sources to agent references")
        };
        let source_agent_result = state
            .agent_results
            .get(source_agent_name)
            .ok_or_else(|| WorkflowError::execution(format!("missing compact source agent '{source_agent_name}'")))?;

        match &source_agent_result.context {
            StoredContext::Many(contexts) => {
                let mut summaries = Vec::new();

                for source_context in contexts {
                    summaries.push(
                        self.compact_single_context(
                            workflow,
                            state,
                            source_agent_result,
                            source_context.clone(),
                            &compact_parts,
                            local_values,
                        )
                        .await?,
                    );
                }

                Ok(Value::Array(summaries.into_iter().map(Value::String).collect()))
            }
            StoredContext::Single(source_context) => self
                .compact_single_context(
                    workflow,
                    state,
                    source_agent_result,
                    source_context.clone(),
                    &compact_parts,
                    local_values,
                )
                .await
                .map(Value::String),
        }
    }

    async fn compact_single_context(
        &self,
        workflow: &CompiledWorkflow,
        state: &WorkflowState,
        source_agent_result: &WorkflowAgentResult,
        mut source_context: Context,
        compact_parts: &CompactParts,
        local_values: &BTreeMap<String, Value>,
    ) -> Result<String, WorkflowError> {
        let model_selector = compact_parts.model.clone().unwrap_or_else(|| source_agent_result.model.clone());
        let provider = self.build_provider_from_selector(workflow, state, &model_selector, "compact")?;
        let output_schema = build_type_schema(
            &crate::ast::TypeExpression::Primitive(crate::ast::PrimitiveType::String),
            &workflow.schemas,
        )?;
        let executor = DynamicExecutor::new(output_schema)?;
        let prompt = if let Some(prompt) = &compact_parts.prompt {
            render_inline_string(prompt, workflow, state, local_values)?
        } else {
            default_compaction_prompt()
        };
        let config = build_agent_config(compact_parts.inference.as_deref().unwrap_or(&[]))?;

        source_context.add_user_message(prompt);

        let compacted_value = executor.execute(&mut source_context, provider.as_ref(), &[], &config).await?;

        compacted_value
            .as_str()
            .map(ToOwned::to_owned)
            .ok_or_else(|| WorkflowError::execution("compact(...) must produce a string"))
    }

    fn build_provider_from_selector(
        &self,
        workflow: &CompiledWorkflow,
        state: &WorkflowState,
        model_selector: &crate::ast::ModelSelector,
        agent_name: &str,
    ) -> Result<Arc<dyn engine_ai_agent::Provider + Send + Sync>, WorkflowError> {
        let provider = workflow
            .providers
            .get(&model_selector.provider_name)
            .ok_or_else(|| WorkflowError::execution(format!("provider '{}' is not available", model_selector.provider_name)))?;
        let api_key = resolve_provider_api_key(provider, &state.secrets)?;

        self.provider_factory
            .build_provider(agent_name, provider, &model_selector.model_name, api_key.as_deref())
    }

    fn validate_inputs_and_secrets(&self, workflow: &CompiledWorkflow, inputs: &Value) -> Result<(), WorkflowError> {
        if let Some(input_schema) = &workflow.input_schema {
            validate_json_against_schema_with_context(inputs, input_schema, "Workflow input does not match schema")
                .map_err(|error| WorkflowError::validation(error.to_string()))?;
        }

        if let Some(secret_schema) = &workflow.secret_schema {
            let secret_value = Value::Object(self.secrets.clone().into_iter().collect());

            validate_json_against_schema_with_context(&secret_value, secret_schema, "Workflow secrets do not match schema")
                .map_err(|error| WorkflowError::validation(error.to_string()))?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
struct CompactParts {
    inference: Option<Vec<InferenceProperty>>,
    model: Option<crate::ast::ModelSelector>,
    prompt: Option<crate::ast::StringTemplate>,
    source_reference: ReferenceExpression,
}

fn parse_compact_parts(compact_expression: &crate::ast::CompactExpression) -> Result<CompactParts, WorkflowError> {
    let mut inference = None;
    let mut model = None;
    let mut prompt = None;
    let mut source_reference = None;

    for argument in &compact_expression.arguments {
        match argument {
            CompactArgument::Agent(reference) => source_reference = Some(reference.clone()),
            CompactArgument::Inference(inference_properties) => inference = Some(inference_properties.clone()),
            CompactArgument::Model(model_selector) => model = Some(model_selector.clone()),
            CompactArgument::Prompt(prompt_template) => prompt = Some(prompt_template.clone()),
        }
    }

    Ok(CompactParts {
        inference,
        model,
        prompt,
        source_reference: source_reference.ok_or_else(|| WorkflowError::execution("compact(...) requires an agent reference"))?,
    })
}

fn find_agent<'a>(workflow: &'a CompiledWorkflow, agent_name: &str) -> Result<&'a CompiledAgent, WorkflowError> {
    workflow
        .agents
        .iter()
        .find(|agent| agent.name == agent_name)
        .ok_or_else(|| WorkflowError::execution(format!("compiled agent '{agent_name}' was not found")))
}

fn resolve_provider_api_key(provider: &CompiledProvider, secrets: &BTreeMap<String, Value>) -> Result<Option<String>, WorkflowError> {
    let Some(secret_name) = &provider.api_key_secret_name else {
        return Ok(None);
    };
    let secret_value = secrets
        .get(secret_name)
        .ok_or_else(|| WorkflowError::execution(format!("missing secret value '{secret_name}'")))?;
    let secret_text = secret_value
        .as_str()
        .ok_or_else(|| WorkflowError::execution(format!("secret '{secret_name}' must be a string value")))?;

    Ok(Some(secret_text.to_string()))
}

fn clone_context(reference: &ReferenceExpression, state: &WorkflowState) -> Result<Context, WorkflowError> {
    let serialized_context = serialize_context_value(reference, state)?;

    match serialized_context {
        Value::Object(_) => serde_json::from_value(serialized_context)
            .map_err(|error| WorkflowError::execution(format!("failed to deserialize stored context: {error}"))),
        Value::Array(_) => Err(WorkflowError::execution(
            "context(...) used as agent context must point to a single agent context",
        )),
        _ => Err(WorkflowError::execution("context(...) produced an invalid runtime value")),
    }
}

fn build_agent_config(inference_properties: &[InferenceProperty]) -> Result<AgentConfig, WorkflowError> {
    let mut config = AgentConfig::default();

    for inference_property in inference_properties {
        match inference_property {
            InferenceProperty::FrequencyPenalty(value) => config.frequency_penalty = Some(parse_f32(value)?),
            InferenceProperty::MaxTokens(value) => config.max_tokens = Some(*value),
            InferenceProperty::PresencePenalty(value) => config.presence_penalty = Some(parse_f32(value)?),
            InferenceProperty::RepeatPenalty(value) => config.repeat_penalty = Some(parse_f32(value)?),
            InferenceProperty::Seed(value) => config.seed = Some(*value),
            InferenceProperty::StopSequences(values) => config.stop_sequences = Some(values.clone()),
            InferenceProperty::Temperature(value) => config.temperature = Some(parse_f32(value)?),
            InferenceProperty::TopK(value) => config.top_k = Some(*value),
            InferenceProperty::TopP(value) => config.top_p = Some(parse_f32(value)?),
        }
    }

    Ok(config)
}

fn parse_f32(value: &str) -> Result<f32, WorkflowError> {
    value
        .replace('_', "")
        .parse::<f32>()
        .map_err(|error| WorkflowError::execution(format!("failed to parse decimal inference value '{value}': {error}")))
}

fn default_compaction_prompt() -> String {
    "Compact the available conversation context into a concise, high-signal summary that preserves the facts, conclusions, and important structured details needed for the next step.".to_string()
}

#[cfg(test)]
mod tests {
    use super::WorkflowRunner;
    use crate::compiler::compile_workflow;
    use crate::parser::parse_workflow;
    use crate::runtime::ProviderFactory;
    use engine_ai_agent::{AgentConfig, Context, Provider, ProviderError, ProviderResponse, StopReason, ToolDefinition};
    use serde::Deserialize;
    use serde_json::json;
    use std::collections::{BTreeMap, VecDeque};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[derive(Debug)]
    struct MockProvider {
        active_requests: Arc<Mutex<usize>>,
        max_parallel_requests: Arc<Mutex<usize>>,
        queued_responses: Mutex<VecDeque<ProviderResponse>>,
        response_delay: Duration,
    }

    #[async_trait::async_trait]
    impl Provider for MockProvider {
        async fn generate(
            &self,
            _context: &Context,
            _tools: &[ToolDefinition],
            _config: &AgentConfig,
        ) -> Result<ProviderResponse, ProviderError> {
            {
                let mut active_requests = self.active_requests.lock().expect("active request lock should not be poisoned");
                *active_requests += 1;

                let mut max_parallel_requests = self.max_parallel_requests.lock().expect("max parallel lock should not be poisoned");

                if *active_requests > *max_parallel_requests {
                    *max_parallel_requests = *active_requests;
                }
            }

            tokio::time::sleep(self.response_delay).await;

            let response = self
                .queued_responses
                .lock()
                .expect("queued response lock should not be poisoned")
                .pop_front()
                .expect("queued response should be available");

            let mut active_requests = self.active_requests.lock().expect("active request lock should not be poisoned");
            *active_requests -= 1;

            Ok(response)
        }
    }

    #[derive(Debug, Clone)]
    struct MockProviderFactory {
        active_requests: Arc<Mutex<usize>>,
        max_parallel_requests: Arc<Mutex<usize>>,
        response_delay: Duration,
        responses_by_agent: BTreeMap<String, Vec<ProviderResponse>>,
    }

    impl ProviderFactory for MockProviderFactory {
        fn build_provider(
            &self,
            agent_name: &str,
            _provider: &crate::compiler::CompiledProvider,
            _model_name: &str,
            _api_key: Option<&str>,
        ) -> Result<Arc<dyn Provider + Send + Sync>, crate::error::WorkflowError> {
            let responses = self
                .responses_by_agent
                .get(agent_name)
                .cloned()
                .ok_or_else(|| crate::error::WorkflowError::execution(format!("missing mock provider for '{agent_name}'")))?;

            Ok(Arc::new(MockProvider {
                active_requests: Arc::clone(&self.active_requests),
                max_parallel_requests: Arc::clone(&self.max_parallel_requests),
                queued_responses: Mutex::new(VecDeque::from(responses)),
                response_delay: self.response_delay,
            }))
        }
    }

    fn finalize_response(answer: serde_json::Value) -> ProviderResponse {
        ProviderResponse {
            tool_calls: vec![engine_ai_agent::ToolCall {
                id: "call-1".to_string(),
                name: "finalize".to_string(),
                arguments: json!({
                    "output": {
                        "type": "success",
                        "answer": answer,
                    }
                }),
            }],
            text: None,
            stop_reason: StopReason::ToolCalls,
            usage: None,
        }
    }

    #[derive(Debug, Deserialize)]
    struct Output {
        a: String,
        b: String,
    }

    #[tokio::test]
    async fn runs_independent_agents_in_parallel() {
        let source = r#"
            provider openai {
                driver: "openai"
                models: ["gpt-4.1-mini"]
            }

            agent a {
                model: openai("gpt-4.1-mini")
                prompt: "A"
                output: string
            }

            agent b {
                model: openai("gpt-4.1-mini")
                prompt: "B"
                output: string
            }

            output {
                a: agent.a
                b: agent.b
            }
        "#;
        let workflow =
            compile_workflow(parse_workflow(source).expect("workflow should parse"), ".".into()).expect("workflow should compile");
        let max_parallel_requests = Arc::new(Mutex::new(0));
        let runner = WorkflowRunner::new(MockProviderFactory {
            active_requests: Arc::new(Mutex::new(0)),
            max_parallel_requests: Arc::clone(&max_parallel_requests),
            response_delay: Duration::from_millis(25),
            responses_by_agent: BTreeMap::from([
                ("a".to_string(), vec![finalize_response(json!("first"))]),
                ("b".to_string(), vec![finalize_response(json!("second"))]),
            ]),
        });
        let output: Output = runner.run(&workflow, json!({})).await.expect("workflow should execute");

        assert_eq!(output.a, "first");
        assert_eq!(output.b, "second");
        assert!(*max_parallel_requests.lock().expect("parallel lock should not be poisoned") >= 2);
    }

    #[tokio::test]
    async fn rejects_invalid_workflow_inputs_before_execution() {
        let source = r#"
            provider openai {
                driver: "openai"
                models: ["gpt-4.1-mini"]
            }

            input {
                topic: string
            }

            agent summary {
                model: openai("gpt-4.1-mini")
                prompt: "{{ input.topic }}"
                output: string
            }

            output {
                summary: agent.summary
            }
        "#;
        let workflow =
            compile_workflow(parse_workflow(source).expect("workflow should parse"), ".".into()).expect("workflow should compile");
        let runner = WorkflowRunner::new(MockProviderFactory {
            active_requests: Arc::new(Mutex::new(0)),
            max_parallel_requests: Arc::new(Mutex::new(0)),
            response_delay: Duration::from_millis(0),
            responses_by_agent: BTreeMap::new(),
        });
        let error = runner
            .run::<serde_json::Value, _>(&workflow, json!({}))
            .await
            .expect_err("runner should reject invalid inputs");

        assert!(error.to_string().contains("Workflow input does not match schema"));
    }

    #[tokio::test]
    async fn workflow_macro_executes_with_a_custom_runner() {
        #[derive(Debug, Deserialize)]
        struct GreetingOutput {
            greeting: String,
        }

        let workflow_path = format!("{}/workflows/v2/minimum.ai", env!("CARGO_MANIFEST_DIR"));
        let runner = WorkflowRunner::new(MockProviderFactory {
            active_requests: Arc::new(Mutex::new(0)),
            max_parallel_requests: Arc::new(Mutex::new(0)),
            response_delay: Duration::from_millis(0),
            responses_by_agent: BTreeMap::from([("greeting".to_string(), vec![finalize_response(json!("hello"))])]),
        });
        let output = crate::workflow!(runner = runner, crate::input!() => workflow_path => GreetingOutput).await;

        assert_eq!(output.greeting, "hello");
    }
}
