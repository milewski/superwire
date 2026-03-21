use crate::context::Context;
use crate::error::AgentError;
use crate::tool::RuntimeTool;
use crate::traits::{Executable, Provider};
use std::sync::Arc;

/// Configuration for the agent
#[derive(Default)]
pub struct AgentConfig {
    pub max_tokens: Option<usize>,
    pub temperature: Option<f32>,
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

    #[must_use]
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }
}

/// The main agent that executes once without retry logic
pub struct Agent<E, P>
where
    E: Executable<Provider = P>,
    P: Provider,
{
    executor: E,
    provider: P,
    tools: Vec<Arc<dyn RuntimeTool>>,
    config: AgentConfig,
}

impl<E, P> Agent<E, P>
where
    E: Executable<Provider = P>,
    E::Error: Into<AgentError>,
    P: Provider,
{
    pub fn new(executor: E, provider: P) -> Self {
        Self {
            executor,
            provider,
            tools: Vec::new(),
            config: AgentConfig {
                max_tokens: None,
                temperature: None,
            },
        }
    }

    #[must_use]
    pub fn with_tool<T: RuntimeTool + Default + 'static>(mut self) -> Self {
        self.tools.push(Arc::new(T::default()));
        self
    }

    #[must_use]
    pub fn with_config(mut self, config: AgentConfig) -> Self {
        self.config = config;
        self
    }

    pub async fn run(&self, prompt: impl Into<String>) -> Result<E::Output, AgentError> {
        let mut context = Context::new();
        context.add_user_message(prompt);

        if let Some(max_tokens) = self.config.max_tokens {
            if context.total_tokens >= max_tokens {
                return Err(AgentError::MaxTokensExceeded {
                    max_tokens,
                    used_tokens: context.total_tokens,
                });
            }
        }

        self.executor
            .execute(&context, &self.provider, &self.tools, &self.config)
            .await
            .map_err(|error| error.into())
    }
}
