use super::context::Context;
use super::error::ValidationError;
use super::message::{ToolCall, ToolResult};
use super::traits::{Executable, Provider, StopReason, Tool};
use async_trait::async_trait;

/// Executor that loops until a "done" tool is called
pub struct LoopExecutor<P, T>
where
    P: Provider,
    T: Tool,
{
    done_tool_name: String,
    phantom: std::marker::PhantomData<(P, T)>,
}

impl<P, T> LoopExecutor<P, T>
where
    P: Provider,
    T: Tool,
{
    #[must_use]
    pub fn new(done_tool_name: String) -> Self {
        Self {
            done_tool_name,
            phantom: std::marker::PhantomData,
        }
    }
}

#[async_trait]
impl<P, T> Executable for LoopExecutor<P, T>
where
    P: Provider<Input = String, Tool = T> + Send + Sync,
    T: Tool + Send + Sync,
{
    type Input = String;
    type Output = String;
    type Provider = P;
    type Tool = T;

    async fn execute(
        &self,
        context: &Context<Self::Input, Self::Tool>,
        provider: &Self::Provider,
    ) -> Result<Self::Output, String> {
        let mut local_context = context.clone();
        let mut iteration = 0;
        const MAX_ITERATIONS: usize = 100;

        loop {
            if iteration >= MAX_ITERATIONS {
                return Err(format!(
                    "Maximum iterations ({MAX_ITERATIONS}) reached without calling done tool"
                ));
            }

            iteration += 1;

            let response = provider
                .generate(&local_context)
                .await
                .map_err(|error| format!("Provider error at iteration {iteration}: {error}"))?;

            if let Some(text) = response.text {
                local_context.add_assistant_message(text);
            }

            if response.tool_calls.is_empty() {
                if response.stop_reason == StopReason::EndOfSequence {
                    local_context.add_system_message(format!(
                        "You must call the '{}' tool to complete the task. Do not end the conversation without calling this tool.",
                        self.done_tool_name
                    ));
                }
                continue;
            }

            for tool_call in &response.tool_calls {
                local_context.add_tool_call(tool_call.clone());
            }

            for tool_call in &response.tool_calls {
                if tool_call.name == self.done_tool_name {
                    let result = self.execute_tool(tool_call).await?;

                    local_context.add_tool_result(ToolResult {
                        tool_call_id: tool_call.id.clone(),
                        content: result.clone(),
                        is_error: false,
                    });

                    return Ok(result);
                }
            }

            use futures::future::FutureExt;

            let tool_futures: Vec<_> = response
                .tool_calls
                .iter()
                .map(|tool_call| self.execute_tool(tool_call).boxed())
                .collect();

            let results = futures::future::join_all(tool_futures).await;

            for (tool_call, result) in response.tool_calls.iter().zip(results.iter()) {
                match result {
                    Ok(content) => {
                        local_context.add_tool_result(ToolResult {
                            tool_call_id: tool_call.id.clone(),
                            content: content.clone(),
                            is_error: false,
                        });
                    }
                    Err(error) => {
                        local_context
                            .add_validation_error(ValidationError::new(format!("Tool execution error: {error}")));
                        local_context.add_tool_result(ToolResult {
                            tool_call_id: tool_call.id.clone(),
                            content: error.clone(),
                            is_error: true,
                        });
                    }
                }
            }
        }
    }
}

impl<P, T> LoopExecutor<P, T>
where
    P: Provider,
    T: Tool,
{
    async fn execute_tool(&self, _tool_call: &ToolCall) -> Result<String, String> {
        Err("execute_tool must be implemented".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::super::context::Context;
    use super::super::message::ToolCall;
    use super::super::traits::{Provider, ProviderResponse, StopReason, Tool};
    use super::*;
    use crate::agent::Message;
    use serde_json::json;

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

    impl Tool for MockTool {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            "Mock tool"
        }

        fn parameters_schema(&self) -> schemars::Schema {
            schemars::Schema::default()
        }
    }

    struct TestExecutor {
        tool_calls_sequence: Vec<Vec<ToolCall>>,
        tool_results: std::collections::HashMap<String, String>,
        current_call_index: std::sync::Arc<std::sync::Mutex<usize>>,
    }

    impl TestExecutor {
        fn new(
            tool_calls_sequence: Vec<Vec<ToolCall>>,
            tool_results: std::collections::HashMap<String, String>,
        ) -> Self {
            Self {
                tool_calls_sequence,
                tool_results,
                current_call_index: std::sync::Arc::new(std::sync::Mutex::new(0)),
            }
        }
    }

    #[async_trait]
    impl Executable for TestExecutor {
        type Input = String;
        type Output = String;
        type Provider = MockProvider;
        type Tool = MockTool;

        async fn execute(
            &self,
            context: &Context<Self::Input, Self::Tool>,
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
                        let result = self
                            .tool_results
                            .get(&tool_call.id)
                            .cloned()
                            .unwrap_or_else(|| "success".to_string());
                        local_context.add_message(Message::tool_result(ToolResult {
                            tool_call_id: tool_call.id.clone(),
                            content: result.clone(),
                            is_error: false,
                        }));
                        return Ok(result);
                    }

                    let result = self
                        .tool_results
                        .get(&tool_call.id)
                        .cloned()
                        .unwrap_or_else(|| format!("Result for {}", tool_call.name));

                    local_context.add_message(Message::tool_result(ToolResult {
                        tool_call_id: tool_call.id.clone(),
                        content: result,
                        is_error: false,
                    }));
                }
            }
        }
    }

    #[tokio::test]
    async fn test_loop_until_done_immediate() {
        let provider = MockProvider::new(vec!["response1".to_string()]);

        let tool_calls = vec![vec![ToolCall {
            id: "call_1".to_string(),
            name: "done".to_string(),
            arguments: json!({"status": "success"}),
        }]];

        let mut tool_results = std::collections::HashMap::new();
        tool_results.insert("call_1".to_string(), "Final result".to_string());

        let executor = TestExecutor::new(tool_calls, tool_results);

        let context = Context::<String, MockTool>::new("test input".to_string());
        let result = executor.execute(&context, &provider).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Final result");
    }

    #[tokio::test]
    async fn test_loop_until_done_multiple_iterations() {
        let provider = MockProvider::new(vec![
            "response1".to_string(),
            "response2".to_string(),
            "response3".to_string(),
        ]);

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
        tool_results.insert("call_3".to_string(), "Task completed".to_string());

        let executor = TestExecutor::new(tool_calls, tool_results);

        let context = Context::<String, MockTool>::new("test input".to_string());
        let result = executor.execute(&context, &provider).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Task completed");
    }

    #[tokio::test]
    async fn test_loop_until_done_max_iterations() {
        let responses: Vec<String> = (0..101).map(|i| format!("response{}", i)).collect();
        let provider = MockProvider::new(responses);

        let tool_calls: Vec<Vec<ToolCall>> = (0..101)
            .map(|i| {
                vec![ToolCall {
                    id: format!("call_{}", i),
                    name: "other_tool".to_string(),
                    arguments: json!({}),
                }]
            })
            .collect();

        let tool_results = std::collections::HashMap::new();

        let executor = TestExecutor::new(tool_calls, tool_results);

        let context = Context::<String, MockTool>::new("test input".to_string());
        let result = executor.execute(&context, &provider).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Maximum iterations"));
    }
}
