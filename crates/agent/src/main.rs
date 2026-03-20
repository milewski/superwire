use async_trait::async_trait;
use engine_ai_agent::{Agent, AgentConfig, AgentError, LoopExecutor, OpenAIProvider, Tool, ToolError};
use rand::Rng;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct QuoteToolInput {
    topic: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct QuoteTool;

#[async_trait]
impl Tool for QuoteTool {
    type Input = QuoteToolInput;

    fn name(&self) -> &str {
        "random_quote"
    }

    fn description(&self) -> &str {
        "Return one random quote from a small hardcoded list."
    }

    async fn execute(&self, input: Self::Input) -> Result<serde_json::Value, ToolError> {
        let quotes = [
            "The winter is coming.",
            "The best of nobody-knows+.",
            "Android > iPhone.",
        ];
        let random_index = rand::thread_rng().gen_range(0..quotes.len());
        let selected_quote = quotes[random_index];

        Ok(serde_json::json!({
            "tool": self.name(),
            "topic": input.topic,
            "quote": selected_quote,
        }))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct RandomNumberToolInput {
    minimum: Option<i64>,
    maximum: Option<i64>,
}

#[derive(Debug, Clone, Default)]
struct RandomNumberTool;

#[async_trait]
impl Tool for RandomNumberTool {
    type Input = RandomNumberToolInput;

    fn name(&self) -> &str {
        "random_number"
    }

    fn description(&self) -> &str {
        "Return a random integer, optionally within a provided range."
    }

    async fn execute(&self, input: Self::Input) -> Result<serde_json::Value, ToolError> {
        let minimum = input.minimum.unwrap_or(0);
        let maximum = input.maximum.unwrap_or(100);

        if minimum > maximum {
            return Err(ToolError::new("minimum cannot be greater than maximum"));
        }

        let generated_number = rand::thread_rng().gen_range(minimum..=maximum);

        Ok(serde_json::json!({
            "tool": self.name(),
            "minimum": minimum,
            "maximum": maximum,
            "number": generated_number,
        }))
    }
}

#[tokio::main]
async fn main() -> Result<(), AgentError> {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
    struct TaskOutput {
        number: isize,
        quote: String,
    }

    println!("Testing with OpenAI-compatible endpoint...");

    let provider = OpenAIProvider::new_local("http://169.254.83.107:1234/v1", "qwen/qwen3.5-35b-a3b");

    println!("Running agent...");

    let executor = LoopExecutor::<OpenAIProvider, TaskOutput>::new()?;
    let result = Agent::new(executor, provider)
        .with_tool::<QuoteTool>()
        .with_tool::<RandomNumberTool>()
        .with_config(AgentConfig::new().with_max_tokens(10000))
        .run("Give me a random quote and a random number between 1 and 10.")
        .await?;

    println!("---------");
    println!("{:#?}", result);

    Ok(())
}
