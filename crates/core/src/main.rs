use engine_ai_core::try_workflow;
use schemars::JsonSchema;
use serde::Deserialize;

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    #[allow(dead_code)]
    #[derive(Debug, Deserialize, JsonSchema)]
    struct Output {
        greeting: String,
    }

    let workflow_result: Output = try_workflow!("../workflows/minimum.ai").await?;

    println!("{workflow_result:?}");

    Ok(())
}
