use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use superwire_core::runtime::WorkflowRuntime;

#[derive(Debug, Serialize, JsonSchema)]
struct WorkflowInput {
    city: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct WorkflowOutput {
    message: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let city_name = std::env::var("CITY").unwrap_or_else(|_| "Madrid".to_string());

    let workflow_runtime = WorkflowRuntime::<WorkflowInput, WorkflowOutput>::from_file("./my_workflow.wire")?;

    let workflow_output = workflow_runtime.run(WorkflowInput { city: city_name }).await?;

    println!("workflow output: {}", workflow_output.message);

    Ok(())
}
