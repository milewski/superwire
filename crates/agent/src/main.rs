use engine_ai_agent::{Agent, AgentConfig, AgentError, LoopExecutor, OpenAIProvider, Tool, ToolError};

#[tokio::main]
async fn main() -> Result<(), AgentError> {
    use async_trait::async_trait;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
    struct TaskOutput {
        result: String,
        confidence: f32,
    }

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

    println!("Testing with OpenAI-compatible endpoint...");

    let executor = LoopExecutor::<OpenAIProvider, TaskOutput>::new()
        .with_max_iterations(10);

    let provider = OpenAIProvider::new("".to_string(), "qwen3.5-9b".to_string())
        .with_base_url("http://100.76.74.102:1234/v1".to_string(), "".to_string());

    let agent = Agent::new(executor, provider)
        .with_config(AgentConfig::new().with_max_tokens(10000));

    println!("Running agent...");

    let result = agent
        .run("What is 2+2? Provide your answer with confidence level.".to_string())
        .await?;

    println!("\n=== RESULT ===");
    println!("{:#?}", result);

    Ok(())
}
