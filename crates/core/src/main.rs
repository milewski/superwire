use engine_ai_core::{parse_inline_workflow, try_workflow};
use serde::Deserialize;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[derive(Debug, Deserialize)]
    struct Output {
        greeting: String,
    }

    let inline = parse_inline_workflow! {
        provider openai {
            driver: "openai"
            api_endpoint: "http://192.168.50.22:1234/v1"
            models: ["qwen3.5-27b"]
        }

        agent greeting {
            model: openai("qwen3.5-27b")
            prompt: "return me a random number"
            output: string
        }

        output {
            greeting: agent.greeting
        }
    };

    let workflow_result: Output = try_workflow!(inline).await??;

    println!("{workflow_result:#?}");

    Ok(())
}
