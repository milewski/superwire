mod agent;
mod context;
mod error;
mod executor;
mod message;
pub mod providers;
pub mod tool;
mod traits;

pub use agent::{Agent, AgentConfig};
pub use context::Context;
pub use error::{AgentError, ValidationError};
pub use executor::LoopExecutor;
pub use message::{Message, MessageRole, ToolCall, ToolResult};
pub use providers::OpenAIProvider;
pub use tool::{DoneTool, Tool, ToolError};
pub use traits::{Executable, Provider, ProviderResponse, StopReason, Validator};

/// Example demonstrating how to use the Agent with LoopExecutor and OpenAI provider
///
/// This example shows:
/// - Creating a validator for output validation
/// - Setting up a LoopExecutor with the validator
/// - Using OpenAI provider for LLM interactions
/// - Defining custom tools
/// - Building and running an agent
///
/// Note: Requires OPENAI_API_KEY environment variable to be set
pub async fn demo() -> Result<String, AgentError> {
    use async_trait::async_trait;
    use serde::{Deserialize, Serialize};
    use std::sync::Arc;

    // Define the output structure that the agent should produce
    #[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
    struct TaskOutput {
        result: String,
        confidence: f32,
    }

    // Create a validator to ensure output meets requirements
    struct OutputValidator;

    #[async_trait]
    impl Validator for OutputValidator {
        type Output = TaskOutput;

        async fn validate(&self, output: &Self::Output) -> Result<(), ValidationError> {
            if output.confidence < 0.0 || output.confidence > 1.0 {
                return Err(ValidationError::new(
                    "Confidence must be between 0.0 and 1.0".to_string(),
                ));
            }

            if output.result.is_empty() {
                return Err(ValidationError::new("Result cannot be empty".to_string()));
            }

            Ok(())
        }
    }

    // Define a custom tool
    #[derive(Clone)]
    struct DemoTool {
        name: String,
        description: String,
    }

    #[async_trait]
    impl Tool for DemoTool {
        type Input = serde_json::Value;

        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            &self.description
        }

        async fn execute(&self, input: Self::Input) -> Result<serde_json::Value, ToolError> {
            Ok(serde_json::json!({
                "tool": self.name,
                "input": input,
                "result": "Tool executed successfully"
            }))
        }
    }

    // Get API key from environment
    let api_key = std::env::var("OPENAI_API_KEY")
        .map_err(|_| AgentError::ExecutionFailed {
            message: "OPENAI_API_KEY environment variable not set".to_string(),
        })?;

    // Set up the components
    let validator = Arc::new(OutputValidator);
    let executor = LoopExecutor::<OpenAIProvider<DemoTool>, DemoTool, OutputValidator, TaskOutput>::new(validator)
        .with_max_iterations(10);

    let provider = OpenAIProvider::new(api_key, "gpt-4".to_string());

    let tools = vec![
        DemoTool {
            name: "search".to_string(),
            description: "Search for information".to_string(),
        },
        DemoTool {
            name: "calculate".to_string(),
            description: "Perform calculations".to_string(),
        },
    ];

    // Create and configure the agent
    let agent = Agent::new(executor, provider)
        .with_tools(tools)
        .with_config(AgentConfig::new().with_max_tokens(10000));

    // Run the agent with a prompt
    let result = agent.run("Analyze the data and provide insights".to_string()).await?;

    Ok(result)
}
