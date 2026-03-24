use engine_ai_core::try_workflow;
use serde::Deserialize;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[derive(Debug, Deserialize)]
    struct Output {
        greeting: String,
    }

    let workflow_result: Result<Output, _> = try_workflow!("../workflows/minimum.ai").await?;
    let workflow_output = workflow_result?;

    println!("{}", workflow_output.greeting);

    Ok(())
}
