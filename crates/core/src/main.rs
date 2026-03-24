use engine_ai_core::{parse_inline_workflow, try_workflow};
use schemars::JsonSchema;
use serde::Deserialize;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[allow(dead_code)]
    #[derive(Debug, Deserialize, JsonSchema)]
    struct Output {
        greeting: u16,
    }

    let inline = parse_inline_workflow! {
        provider openai {
            driver: "openai"
            api_endpoint: "http://169.254.83.107:1234/v1"
            models: ["qwen3.5-27b"]
        }

        agent greeting {
            model: openai("qwen3.5-27b")
            prompt: "generate a random number"
            output: number
        }

        output {
            greeting: agent.greeting
        }
    };

    let workflow_result: Output = try_workflow!(inline).await?;

    println!("{:#?}", workflow_result);

    Ok(())
}
