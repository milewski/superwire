mod args;
mod error;

use args::parse_inputs;
use clap::Parser;
use engine_ai_core::execution::engine::ExecutionEngine;
use error::CliError;
use std::path::PathBuf;
use std::process;

#[derive(Parser, Debug)]
#[command(name = "cli")]
#[command(about = "Execute Engine AI workflows from the command line", long_about = None)]
struct CliArgs {
    #[arg(help = "Path to the workflow file (.ai)")]
    workflow_path: PathBuf,

    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    #[arg(help = "Workflow inputs as --key value pairs")]
    inputs: Vec<String>,
}

#[tokio::main]
async fn main() {
    colog::init();

    if let Err(error) = run().await {
        log::error!("{error}");
        process::exit(1);
    }
}

async fn run() -> Result<(), CliError> {
    let arguments = CliArgs::parse();

    if !arguments.workflow_path.exists() {
        return Err(CliError::WorkflowNotFound(
            arguments.workflow_path.display().to_string(),
        ));
    }

    let inputs = parse_inputs(arguments.inputs)?;

    log::info!("Executing workflow: {}", arguments.workflow_path.display());
    log::debug!("Inputs: {inputs:?}");

    let engine = ExecutionEngine::new();
    let result = engine
        .execute_workflow_with_inputs(
            arguments
                .workflow_path
                .to_str()
                .ok_or_else(|| CliError::InvalidArguments("Invalid workflow path encoding".to_string()))?,
            inputs,
        )
        .await?;

    let output = serde_json::to_string_pretty(&result)?;
    println!("{output}");

    Ok(())
}
