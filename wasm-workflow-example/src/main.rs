use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use superwire_core::{tool, try_workflow, Tool};
use tools::weather::{WeatherInput, WeatherOutput};

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
    let city = std::env::var("CITY").unwrap_or_else(|_| "Shanghai".to_string());

    let input = WorkflowInput { city };
    let output: WorkflowOutput = try_workflow!("../my_workflow.wire", input).await?;

    println!("workflow output: {:#?}", output);

    let tool: Tool<WeatherInput, WeatherOutput> = tool!("../tools/weather.wasm")?;

    println!("tool definition: {:#?}", tool.definition());

    let direct_tool_output = tool
        .run(WeatherInput {
            city: Some("Shanghai".to_string()),
        })
        .await?;

    println!("direct tool output: {:#?}", direct_tool_output);

    println!(
        "summary: {} {} {}",
        direct_tool_output.city, direct_tool_output.summary, direct_tool_output.source
    );

    Ok(())
}
