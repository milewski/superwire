use crate::context::Context;
use crate::error::AgentError;
use crate::tool::RuntimeTool;
use crate::traits::{Executable, Provider};
use std::sync::Arc;

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

#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: Vec<Arc<dyn RuntimeTool>>,
}

impl ToolRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn register<T>(mut self) -> Self
    where
        T: RuntimeTool + Default + 'static,
    {
        self.tools.push(Arc::new(T::default()));
        self
    }

    #[must_use]
    pub fn tools(&self) -> &[Arc<dyn RuntimeTool>] {
        &self.tools
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
    P: Provider,
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
            .execute(&context, &self.provider, &self.tools)
            .await
            .map_err(|message| AgentError::ExecutionFailed { message })
    }
}
