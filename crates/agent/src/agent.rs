use crate::context::Context;
use crate::error::AgentError;
use crate::traits::{Executable, Provider, Tool};

/// Configuration for the agent
#[derive(Default)]
pub struct AgentConfig {
    pub max_tokens: Option<usize>,
}

impl AgentConfig {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }
}

/// The main agent that executes once without retry logic
pub struct Agent<E, P, T>
where
    E: Executable<Provider = P, Tool = T>,
    P: Provider,
    T: Tool,
{
    executor: E,
    provider: P,
    tools: Vec<T>,
    config: AgentConfig,
}

impl<E, P, T> Agent<E, P, T>
where
    E: Executable<Provider = P, Tool = T>,
    P: Provider,
    T: Tool,
{
    pub fn new(executor: E, provider: P) -> Self {
        Self {
            executor,
            provider,
            tools: Vec::new(),
            config: AgentConfig { max_tokens: None },
        }
    }

    #[must_use]
    pub fn with_tools(mut self, tools: Vec<T>) -> Self {
        self.tools = tools;
        self
    }

    #[must_use]
    pub fn with_config(mut self, config: AgentConfig) -> Self {
        self.config = config;
        self
    }

    pub async fn run(&self, prompt: E::Prompt) -> Result<E::Output, AgentError> {
        let context = Context::<E::Prompt, E::Tool>::new(prompt);

        if let Some(max_tokens) = self.config.max_tokens {
            if context.total_tokens >= max_tokens {
                return Err(AgentError::MaxTokensExceeded {
                    max_tokens,
                    used_tokens: context.total_tokens,
                });
            }
        }

        self.executor
            .execute(&context, &self.provider)
            .await
            .map_err(|message| AgentError::ExecutionFailed { message })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Context;
    use crate::error::AgentError;
    use crate::traits::{Executable, Provider, ProviderResponse, StopReason, Tool};

    struct MockProvider;

    #[async_trait::async_trait]
    impl Provider for MockProvider {
        type Input = String;
        type Tool = MockTool;

        async fn generate(&self, _context: &Context<Self::Input, Self::Tool>) -> Result<ProviderResponse, String> {
            Ok(ProviderResponse {
                tool_calls: vec![],
                text: Some("Mock response".to_string()),
                stop_reason: StopReason::EndOfSequence,
            })
        }
    }

    #[derive(Clone)]
    struct MockTool {
        name: String,
        description: String,
    }

    #[async_trait::async_trait]
    impl Tool for MockTool {
        type Input = serde_json::Value;

        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            &self.description
        }

        async fn execute(&self, _input: Self::Input) -> Result<serde_json::Value, crate::ToolError> {
            Ok(serde_json::Value::String(format!("Result for {}", self.name)))
        }
    }

    struct MockExecutor {
        should_succeed: bool,
    }

    #[async_trait::async_trait]
    impl Executable for MockExecutor {
        type Prompt = String;
        type Output = String;
        type Provider = MockProvider;
        type Tool = MockTool;

        async fn execute(
            &self,
            context: &Context<Self::Prompt, Self::Tool>,
            _provider: &Self::Provider,
        ) -> Result<Self::Output, String> {
            if self.should_succeed {
                Ok(format!("Success after {} attempts", context.attempt))
            } else {
                Err(format!("Failure at attempt {}", context.attempt))
            }
        }
    }

    #[tokio::test]
    async fn test_agent_success() {
        let executor = MockExecutor { should_succeed: true };
        let provider = MockProvider;

        let agent = Agent::new(executor, provider);
        let result = agent.run("test input".to_string()).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Success after 0 attempts");
    }

    #[tokio::test]
    async fn test_agent_execution_failed() {
        let executor = MockExecutor { should_succeed: false };
        let provider = MockProvider;

        let agent = Agent::new(executor, provider);
        let result = agent.run("test input".to_string()).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            AgentError::ExecutionFailed { message } => {
                assert_eq!(message, "Failure at attempt 0");
            }
            _ => panic!("Expected ExecutionFailed error"),
        }
    }

    #[tokio::test]
    async fn test_agent_with_tools() {
        let executor = MockExecutor { should_succeed: true };
        let provider = MockProvider;

        let tools = vec![
            MockTool {
                name: "tool1".to_string(),
                description: "First tool".to_string(),
            },
            MockTool {
                name: "tool2".to_string(),
                description: "Second tool".to_string(),
            },
        ];

        let agent = Agent::new(executor, provider).with_tools(tools);
        let result = agent.run("test input".to_string()).await;

        assert!(result.is_ok());
    }
}
