use crate::context::Context;
use crate::error::ValidationError;
use crate::message::ToolResult;
use crate::tool::{DoneTool, Tool, ToolError};
use crate::traits::{Executable, Provider, ProviderResponse, StopReason, Validator};
use async_trait::async_trait;
use std::sync::Arc;

/// Executor that loops until a "done" tool is called with valid output
pub struct LoopExecutor<P, T, V, O>
where
    P: Provider,
    T: Tool,
    V: Validator<Output = O>,
{
    done_tool: Arc<DoneTool<V, O>>,
    max_iterations: usize,
    phantom: std::marker::PhantomData<(P, T)>,
}

impl<P, T, V, O> LoopExecutor<P, T, V, O>
where
    P: Provider<Input = String, Tool = T> + Send + Sync,
    T: Tool + Send + Sync,
    V: Validator<Output = O> + Send + Sync,
    O: Send + Sync + serde::Serialize + serde::de::DeserializeOwned + schemars::JsonSchema,
{
    pub fn new(validator: Arc<V>) -> Self {
        Self {
            done_tool: Arc::new(DoneTool::new(validator)),
            max_iterations: 5,
            phantom: std::marker::PhantomData,
        }
    }

    #[must_use]
    pub fn with_max_iterations(mut self, max_iterations: usize) -> Self {
        self.max_iterations = max_iterations;
        self
    }

    async fn try_process_done_tool(
        &self,
        context: &mut Context<String, T>,
        response: &ProviderResponse,
    ) -> Result<Option<String>, String> {
        let is_done_only = response.tool_calls.len() == 1 && response.tool_calls[0].name == "done";

        if !is_done_only {
            return Ok(None);
        }

        let tool_call = &response.tool_calls[0];

        let input_result: Result<O, _> = serde_json::from_value(tool_call.arguments.clone());

        match input_result {
            Ok(input) => match self.done_tool.execute(input).await {
                Ok(value) => {
                    context.add_tool_result(ToolResult {
                        tool_call_id: tool_call.id.clone(),
                        content: value.clone(),
                        is_error: false,
                    });

                    let content = serde_json::to_string(&value).unwrap_or_else(|_| value.to_string());
                    Ok(Some(content))
                }
                Err(tool_error) => {
                    context.add_validation_error(ValidationError::new(tool_error.error.clone()));
                    context.add_tool_result(ToolResult {
                        tool_call_id: tool_call.id.clone(),
                        content: serde_json::Value::String(tool_error.to_agent_message()),
                        is_error: true,
                    });
                    context.increment_attempt();

                    Ok(None)
                }
            },
            Err(error) => {
                let tool_error = ToolError::new(format!("Failed to deserialize done tool arguments: {error}"))
                    .with_suggestion("Check that the arguments match the expected schema".to_string())
                    .with_context("error".to_string(), serde_json::Value::String(error.to_string()));

                context.add_validation_error(ValidationError::new(tool_error.error.clone()));
                context.add_tool_result(ToolResult {
                    tool_call_id: tool_call.id.clone(),
                    content: serde_json::Value::String(tool_error.to_agent_message()),
                    is_error: true,
                });
                context.increment_attempt();

                Ok(None)
            }
        }
    }

    async fn execute_regular_tools(&self, context: &mut Context<String, T>, response: &ProviderResponse) {
        for tool_call in response.tool_calls.iter().filter(|tc| tc.name != "done") {
            let tool = context.tools.iter().find(|t| t.name() == tool_call.name);

            let result = match tool {
                Some(tool) => match serde_json::from_value(tool_call.arguments.clone()) {
                    Ok(input) => tool.execute(input).await,
                    Err(error) => Err(ToolError::new(format!(
                        "Failed to deserialize arguments for tool '{}': {}",
                        tool_call.name, error
                    ))
                    .with_suggestion("Check that the arguments match the tool's expected schema".to_string())
                    .with_context(
                        "tool".to_string(),
                        serde_json::Value::String(tool_call.name.to_string()),
                    )),
                },
                None => Err(ToolError::new(format!("Tool '{}' not found", tool_call.name))
                    .with_suggestion("Check the tool name and ensure it's registered".to_string())),
            };

            match result {
                Ok(content) => {
                    context.add_tool_result(ToolResult {
                        tool_call_id: tool_call.id.clone(),
                        content,
                        is_error: false,
                    });
                }
                Err(error) => {
                    context.add_validation_error(ValidationError::new(error.error.clone()));
                    context.add_tool_result(ToolResult {
                        tool_call_id: tool_call.id.clone(),
                        content: serde_json::Value::String(error.to_agent_message()),
                        is_error: true,
                    });
                }
            }
        }
    }
}

#[async_trait]
impl<P, T, V, O> Executable for LoopExecutor<P, T, V, O>
where
    P: Provider<Input = String, Tool = T> + Send + Sync,
    T: Tool + Send + Sync,
    V: Validator<Output = O> + Send + Sync,
    O: Send + Sync + serde::Serialize + serde::de::DeserializeOwned + schemars::JsonSchema,
{
    type Prompt = String;
    type Output = String;
    type Provider = P;
    type Tool = T;

    async fn execute(
        &self,
        context: &Context<Self::Prompt, Self::Tool>,
        provider: &Self::Provider,
    ) -> Result<Self::Output, String> {
        let mut local_context = context.clone();
        let mut iteration = 0;

        loop {
            if iteration >= self.max_iterations {
                return Err(format!(
                    "Maximum iterations ({}) reached without calling done tool",
                    self.max_iterations
                ));
            }

            let response = provider
                .generate(&local_context)
                .await
                .map_err(|error| format!("Provider error at iteration {iteration}: {error}"))?;

            if let Some(text) = &response.text {
                local_context.add_assistant_message(text.clone());
            }

            if response.tool_calls.is_empty() {
                if response.stop_reason == StopReason::EndOfSequence {
                    local_context.add_system_message(
                        "You must call the 'done' tool to complete the task. Do not end the conversation without calling this tool.".to_string()
                    );
                }

                iteration += 1;

                continue;
            }

            for tool_call in &response.tool_calls {
                local_context.add_tool_call(tool_call.clone());
            }

            if let Some(result) = self.try_process_done_tool(&mut local_context, &response).await? {
                return Ok(result);
            }

            self.execute_regular_tools(&mut local_context, &response).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Context;
    use crate::message::{Message, ToolCall};
    use crate::traits::{Provider, ProviderResponse, StopReason, Tool, Validator};
    use crate::ValidationError;
    use serde::{Deserialize, Serialize};
    use serde_json::json;
    use std::sync::Arc;

    struct MockProvider {
        responses: Vec<String>,
        current_index: std::sync::Arc<std::sync::Mutex<usize>>,
    }

    impl MockProvider {
        fn new(responses: Vec<String>) -> Self {
            Self {
                responses,
                current_index: std::sync::Arc::new(std::sync::Mutex::new(0)),
            }
        }
    }

    #[async_trait]
    impl Provider for MockProvider {
        type Input = String;
        type Tool = MockTool;

        async fn generate(&self, _context: &Context<Self::Input, Self::Tool>) -> Result<ProviderResponse, String> {
            let response = {
                let mut index = self.current_index.lock().unwrap();
                if *index >= self.responses.len() {
                    return Err("No more responses".to_string());
                }
                let response = self.responses[*index].clone();
                *index += 1;
                response
            };
            Ok(ProviderResponse {
                tool_calls: vec![],
                text: Some(response),
                stop_reason: StopReason::EndOfSequence,
            })
        }
    }

    #[derive(Clone)]
    struct MockTool {
        name: String,
    }

    #[async_trait]
    impl Tool for MockTool {
        type Input = serde_json::Value;

        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            "Mock tool"
        }

        async fn execute(&self, _input: Self::Input) -> Result<serde_json::Value, ToolError> {
            Ok(serde_json::Value::String(format!("Result for {}", self.name)))
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
    struct TestOutput {
        status: String,
    }

    struct MockValidator {
        should_pass: bool,
    }

    #[async_trait]
    impl Validator for MockValidator {
        type Output = TestOutput;

        async fn validate(&self, _output: &Self::Output) -> Result<(), ValidationError> {
            if self.should_pass {
                Ok(())
            } else {
                Err(ValidationError::new("Validation failed".to_string()))
            }
        }
    }

    struct TestExecutor<V>
    where
        V: Validator<Output = TestOutput>,
    {
        tool_calls_sequence: Vec<Vec<ToolCall>>,
        tool_results: std::collections::HashMap<String, String>,
        current_call_index: std::sync::Arc<std::sync::Mutex<usize>>,
        loop_executor: LoopExecutor<MockProvider, MockTool, V, TestOutput>,
    }

    impl<V> TestExecutor<V>
    where
        V: Validator<Output = TestOutput> + Send + Sync,
    {
        fn new(
            tool_calls_sequence: Vec<Vec<ToolCall>>,
            tool_results: std::collections::HashMap<String, String>,
            validator: Arc<V>,
        ) -> Self {
            Self {
                tool_calls_sequence,
                tool_results,
                current_call_index: std::sync::Arc::new(std::sync::Mutex::new(0)),
                loop_executor: LoopExecutor::new(validator),
            }
        }
    }

    #[async_trait]
    impl<V> Executable for TestExecutor<V>
    where
        V: Validator<Output = TestOutput> + Send + Sync,
    {
        type Prompt = String;
        type Output = String;
        type Provider = MockProvider;
        type Tool = MockTool;

        async fn execute(
            &self,
            context: &Context<Self::Prompt, Self::Tool>,
            provider: &Self::Provider,
        ) -> Result<Self::Output, String> {
            let mut local_context = context.clone();
            let mut iteration = 0;
            const MAX_ITERATIONS: usize = 100;

            loop {
                if iteration >= MAX_ITERATIONS {
                    return Err("Maximum iterations reached".to_string());
                }

                let tool_calls = {
                    let mut call_index = self.current_call_index.lock().unwrap();
                    if *call_index >= self.tool_calls_sequence.len() {
                        return Err("No more tool calls".to_string());
                    }

                    let calls = self.tool_calls_sequence[*call_index].clone();
                    *call_index += 1;
                    calls
                };

                iteration += 1;

                let _response = provider.generate(&local_context).await?;

                for tool_call in tool_calls {
                    local_context.add_message(Message::tool_call(tool_call.clone()));

                    if tool_call.name == "done" {
                        let input_result: Result<TestOutput, _> = serde_json::from_value(tool_call.arguments.clone());

                        match input_result {
                            Ok(input) => match self.loop_executor.done_tool.execute(input).await {
                                Ok(value) => {
                                    let content = serde_json::to_string(&value).unwrap_or_else(|_| value.to_string());

                                    local_context.add_message(Message::tool_result(ToolResult {
                                        tool_call_id: tool_call.id.clone(),
                                        content: value.clone(),
                                        is_error: false,
                                    }));
                                    return Ok(content);
                                }
                                Err(tool_error) => {
                                    local_context.add_validation_error(ValidationError::new(tool_error.error.clone()));
                                    local_context.add_message(Message::tool_result(ToolResult {
                                        tool_call_id: tool_call.id.clone(),
                                        content: serde_json::Value::String(tool_error.to_agent_message()),
                                        is_error: true,
                                    }));
                                    local_context.increment_attempt();
                                }
                            },
                            Err(error) => {
                                let tool_error = ToolError::new(format!("Failed to parse arguments: {error}"))
                                    .with_suggestion("Check that the arguments match the expected schema".to_string());
                                local_context.add_validation_error(ValidationError::new(tool_error.error.clone()));
                                local_context.add_message(Message::tool_result(ToolResult {
                                    tool_call_id: tool_call.id.clone(),
                                    content: serde_json::Value::String(tool_error.to_agent_message()),
                                    is_error: true,
                                }));
                                local_context.increment_attempt();
                            }
                        }
                        continue;
                    }

                    let result = self
                        .tool_results
                        .get(&tool_call.id)
                        .cloned()
                        .unwrap_or_else(|| format!("Result for {}", tool_call.name));

                    local_context.add_message(Message::tool_result(ToolResult {
                        tool_call_id: tool_call.id.clone(),
                        content: serde_json::Value::String(result),
                        is_error: false,
                    }));
                }
            }
        }
    }

    #[tokio::test]
    async fn test_loop_until_done_immediate() {
        let provider = MockProvider::new(vec!["response1".to_string()]);
        let validator = Arc::new(MockValidator { should_pass: true });

        let tool_calls = vec![vec![ToolCall {
            id: "call_1".to_string(),
            name: "done".to_string(),
            arguments: json!({"status": "success"}),
        }]];

        let tool_results = std::collections::HashMap::new();

        let executor = TestExecutor::new(tool_calls, tool_results, validator);

        let context = Context::<String, MockTool>::new("test input".to_string());
        let result = executor.execute(&context, &provider).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_loop_until_done_multiple_iterations() {
        let provider = MockProvider::new(vec![
            "response1".to_string(),
            "response2".to_string(),
            "response3".to_string(),
        ]);
        let validator = Arc::new(MockValidator { should_pass: true });

        let tool_calls = vec![
            vec![ToolCall {
                id: "call_1".to_string(),
                name: "search".to_string(),
                arguments: json!({"query": "test"}),
            }],
            vec![ToolCall {
                id: "call_2".to_string(),
                name: "calculate".to_string(),
                arguments: json!({"expression": "2+2"}),
            }],
            vec![ToolCall {
                id: "call_3".to_string(),
                name: "done".to_string(),
                arguments: json!({"status": "success"}),
            }],
        ];

        let mut tool_results = std::collections::HashMap::new();
        tool_results.insert("call_1".to_string(), "Search results".to_string());
        tool_results.insert("call_2".to_string(), "4".to_string());

        let executor = TestExecutor::new(tool_calls, tool_results, validator);

        let context = Context::<String, MockTool>::new("test input".to_string());
        let result = executor.execute(&context, &provider).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_loop_with_validation_failure() {
        let provider = MockProvider::new(vec!["response1".to_string(), "response2".to_string()]);
        let validator = Arc::new(MockValidator { should_pass: false });

        let tool_calls = vec![vec![ToolCall {
            id: "call_1".to_string(),
            name: "done".to_string(),
            arguments: json!({"status": "incomplete"}),
        }]];

        let tool_results = std::collections::HashMap::new();

        let executor = TestExecutor::new(tool_calls, tool_results, validator);

        let context = Context::<String, MockTool>::new("test input".to_string());
        let result = executor.execute(&context, &provider).await;

        assert!(result.is_err());
    }
}
