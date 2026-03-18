use super::context::Context;
use super::error::AgentError;
use super::traits::{Executable, Provider, Tool, Validator};

/// Configuration for the validation-retry agent
#[derive(Default)]
pub struct AgentConfig {
    pub max_retries: Option<usize>,
    pub max_tokens: Option<usize>,
}

impl AgentConfig {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_max_retries(mut self, max_retries: usize) -> Self {
        self.max_retries = Some(max_retries);
        self
    }

    #[must_use]
    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }
}

/// The main validation-retry agent
pub struct Agent<E, V, P, T>
where
    E: Executable<Provider = P, Tool = T>,
    V: Validator<Output = E::Output>,
    P: Provider,
    T: Tool,
{
    executor: E,
    validator: V,
    provider: P,
    tools: Vec<T>,
    config: AgentConfig,
}

impl<E, V, P, T> Agent<E, V, P, T>
where
    E: Executable<Provider = P, Tool = T>,
    V: Validator<Output = E::Output>,
    P: Provider,
    T: Tool,
{
    pub fn new(executor: E, validator: V, provider: P) -> Self {
        Self {
            executor,
            validator,
            provider,
            tools: Vec::new(),
            config: AgentConfig {
                max_retries: None,
                max_tokens: None,
            },
        }
    }

    pub fn with_tools(mut self, tools: Vec<T>) -> Self {
        self.tools = tools;
        self
    }

    pub fn with_config(mut self, config: AgentConfig) -> Self {
        self.config = config;
        self
    }

    pub async fn run(&self, initial_input: E::Input) -> Result<E::Output, AgentError> {
        let mut context = Context::<E::Input, E::Tool>::new(initial_input);

        loop {
            if let Some(max) = self.config.max_retries {
                if context.attempt >= max {
                    return Err(AgentError::MaxRetriesExceeded { max_retries: max });
                }
            }

            if let Some(max_tokens) = self.config.max_tokens {
                if context.total_tokens >= max_tokens {
                    return Err(AgentError::MaxTokensExceeded {
                        max_tokens,
                        used_tokens: context.total_tokens,
                    });
                }
            }

            match self.executor.execute(&context, &self.provider).await {
                Ok(output) => match self.validator.validate(&output).await {
                    Ok(()) => return Ok(output),
                    Err(error) => {
                        context.add_validation_error(error);
                        context.increment_attempt();
                    }
                },
                Err(message) => {
                    return Err(AgentError::ExecutionFailed { message });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::context::Context;
    use super::super::error::{AgentError, ValidationError};
    use super::super::traits::{Executable, Provider, ProviderResponse, Tool, Validator};
    use super::*;
    use crate::agent::StopReason;

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

    impl Tool for MockTool {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            &self.description
        }

        fn parameters_schema(&self) -> schemars::Schema {
            schemars::Schema::default()
        }
    }

    struct MockExecutor {
        should_succeed: bool,
    }

    #[async_trait::async_trait]
    impl Executable for MockExecutor {
        type Input = String;
        type Output = String;
        type Provider = MockProvider;
        type Tool = MockTool;

        async fn execute(
            &self,
            context: &Context<Self::Input, Self::Tool>,
            _provider: &Self::Provider,
        ) -> Result<Self::Output, String> {
            if self.should_succeed {
                Ok(format!("Success after {} attempts", context.attempt))
            } else {
                Err(format!("Failure at attempt {}", context.attempt))
            }
        }
    }

    struct ValidationFailingExecutor;

    #[async_trait::async_trait]
    impl Executable for ValidationFailingExecutor {
        type Input = String;
        type Output = String;
        type Provider = MockProvider;
        type Tool = MockTool;

        async fn execute(
            &self,
            context: &Context<Self::Input, Self::Tool>,
            _provider: &Self::Provider,
        ) -> Result<Self::Output, String> {
            Ok(format!("Failure at attempt {}", context.attempt))
        }
    }

    struct MockValidator {
        fail_until_attempt: usize,
    }

    #[async_trait::async_trait]
    impl Validator for MockValidator {
        type Output = String;

        async fn validate(&self, output: &Self::Output) -> Result<(), ValidationError> {
            if output.contains("Success") {
                Ok(())
            } else {
                let attempt_str = output.split("attempt ").nth(1).and_then(|s| s.parse::<usize>().ok());

                if let Some(attempt) = attempt_str {
                    if attempt >= self.fail_until_attempt {
                        Ok(())
                    } else {
                        Err(ValidationError::new(format!("Still failing at attempt {}", attempt)))
                    }
                } else {
                    Err(ValidationError::new("Could not parse attempt".to_string()))
                }
            }
        }
    }

    #[tokio::test]
    async fn test_agent_success_first_attempt() {
        let executor = MockExecutor { should_succeed: true };
        let validator = MockValidator { fail_until_attempt: 0 };
        let provider = MockProvider;

        let agent = Agent::new(executor, validator, provider);

        let result = agent.run("test input".to_string()).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Success after 0 attempts");
    }

    #[tokio::test]
    async fn test_agent_success_after_retries() {
        let executor = ValidationFailingExecutor;
        let validator = MockValidator { fail_until_attempt: 2 };
        let provider = MockProvider;

        let agent = Agent::new(executor, validator, provider).with_config(AgentConfig::new().with_max_retries(5));

        let result = agent.run("test input".to_string()).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_agent_max_retries_exceeded() {
        let executor = ValidationFailingExecutor;
        let validator = MockValidator { fail_until_attempt: 10 };
        let provider = MockProvider;

        let agent = Agent::new(executor, validator, provider).with_config(AgentConfig::new().with_max_retries(3));

        let result = agent.run("test input".to_string()).await;

        assert!(result.is_err());

        match result.unwrap_err() {
            AgentError::MaxRetriesExceeded { max_retries } => {
                assert_eq!(max_retries, 3);
            }
            _ => panic!("Expected MaxRetriesExceeded error"),
        }
    }

    struct FailingExecutor;

    #[async_trait::async_trait]
    impl Executable for FailingExecutor {
        type Input = String;
        type Output = String;
        type Provider = MockProvider;
        type Tool = MockTool;

        async fn execute(
            &self,
            _context: &Context<Self::Input, Self::Tool>,
            _provider: &Self::Provider,
        ) -> Result<Self::Output, String> {
            Err("Execution failed".to_string())
        }
    }

    #[tokio::test]
    async fn test_agent_execution_failed() {
        let executor = FailingExecutor;
        let validator = MockValidator { fail_until_attempt: 0 };
        let provider = MockProvider;

        let agent = Agent::new(executor, validator, provider);

        let result = agent.run("test input".to_string()).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            AgentError::ExecutionFailed { message } => {
                assert_eq!(message, "Execution failed");
            }
            _ => panic!("Expected ExecutionFailed error"),
        }
    }

    #[tokio::test]
    async fn test_agent_config_builder() {
        let config = AgentConfig::new().with_max_retries(5).with_max_tokens(10000);

        assert_eq!(config.max_retries, Some(5));
        assert_eq!(config.max_tokens, Some(10000));
    }

    #[tokio::test]
    async fn test_agent_with_tools() {
        let executor = MockExecutor { should_succeed: true };
        let validator = MockValidator { fail_until_attempt: 0 };
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

        let agent = Agent::new(executor, validator, provider).with_tools(tools);

        let result = agent.run("test input".to_string()).await;
        assert!(result.is_ok());
    }
}
