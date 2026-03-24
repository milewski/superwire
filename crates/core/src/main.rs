use engine_ai_core::runtime::{ScriptedProviderFactory, WorkflowRuntime};
use serde_json::{json, Value};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let workflow_source = include_str!("../workflows/minimum.ai");

    let mut scripted_outputs_by_agent_name = HashMap::<String, Value>::new();
    scripted_outputs_by_agent_name.insert(
        "greeting".to_owned(),
        Value::String("Hello from the integrated DSL + agent runtime.".to_owned()),
    );

    let workflow_runtime = WorkflowRuntime::new(ScriptedProviderFactory::new(scripted_outputs_by_agent_name));

    let workflow_result = workflow_runtime.execute_source(workflow_source, json!({}), json!({})).await?;

    println!("{}", serde_json::to_string_pretty(&workflow_result.output)?);

    Ok(())
}
