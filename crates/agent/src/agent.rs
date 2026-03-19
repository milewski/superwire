use crate::context::Context;
use crate::error::AgentError;
use crate::traits::{Executable, Provider};

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
pub struct Agent<E, P>
where
    E: Executable<Provider = P>,
    P: Provider,
{
    executor: E,
    provider: P,
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
            config: AgentConfig { max_tokens: None },
        }
    }

    #[must_use]
    pub fn with_tools(self) -> Self {
        self
    }

    #[must_use]
    pub fn with_config(mut self, config: AgentConfig) -> Self {
        self.config = config;
        self
    }

    pub async fn run(&self, prompt: E::Prompt) -> Result<E::Output, AgentError>
    where
        E::Prompt: Clone + ToString,
    {
        let mut context = Context::new(prompt.to_string());
        context.add_user_message(prompt.to_string());

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
