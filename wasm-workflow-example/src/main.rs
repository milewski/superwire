use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use superwire_core::runtime::WorkflowRuntime;
use superwire_core::try_workflow;

#[derive(Debug, Serialize, JsonSchema)]
struct WorkflowInput {
    city: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct WorkflowOutput {
    weather: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let city = std::env::var("CITY").unwrap_or_else(|_| "Madrid".to_string());

    // let runtime = WorkflowRuntime::<WorkflowInput, WorkflowOutput>::from_file("./my_workflow.wire")?;
    // let output = runtime.run(WorkflowInput { city }).await?;

    let output: WorkflowOutput = try_workflow!("../my_workflow.wire", WorkflowInput { city }).await?;
    //
    println!("{:#?}", output);

    Ok(())
}
