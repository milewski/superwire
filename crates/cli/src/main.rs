mod args;
mod compile;
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
    #[command(subcommand)]
    command: Command,
}

#[derive(Parser, Debug)]
enum Command {
    #[command(about = "Run a workflow file")]
    Run {
        #[arg(help = "Path to the workflow file (.ai)")]
        workflow_path: PathBuf,

        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        #[arg(help = "Workflow inputs as --key value pairs")]
        inputs: Vec<String>,
    },
    #[command(about = "Compile a workflow file to a standalone executable")]
    Build {
        #[arg(help = "Path to the workflow file (.ai)")]
        workflow_path: PathBuf,

        #[arg(short, long, help = "Output path for the compiled executable")]
        output: PathBuf,
    },
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

    match arguments.command {
        Command::Run { workflow_path, inputs } => {
            if !workflow_path.exists() {
                return Err(CliError::WorkflowNotFound(workflow_path.display().to_string()));
            }

            let parsed_inputs = parse_inputs(inputs)?;

            log::info!("Executing workflow: {}", workflow_path.display());
            log::debug!("Inputs: {parsed_inputs:?}");

            let engine = ExecutionEngine::new();
            let result = engine
                .execute_workflow_with_inputs(
                    workflow_path
                        .to_str()
                        .ok_or_else(|| CliError::InvalidArguments("Invalid workflow path encoding".to_string()))?,
                    parsed_inputs,
                )
                .await?;

            let output = serde_json::to_string_pretty(&result)?;
            println!("{output}");

            Ok(())
        }
        Command::Build { workflow_path, output } => {
            if !workflow_path.exists() {
                return Err(CliError::WorkflowNotFound(workflow_path.display().to_string()));
            }

            log::info!("Compiling workflow: {}", workflow_path.display());
            log::info!("Output: {}", output.display());

            compile::compile_workflow(&workflow_path, &output)?;

            log::info!("Compilation successful!");

            Ok(())
        }
    }
}
