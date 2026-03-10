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
    #[command(about = "Create a new Engine AI project with a sample workflow")]
    New {
        #[arg(help = "Project name (optional, defaults to current directory)")]
        name: Option<String>,
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
        Command::New { name } => {
            create_new_project(name)?;
            Ok(())
        }
    }
}

fn create_new_project(name: Option<String>) -> Result<(), CliError> {
    let project_name = name.ok_or_else(|| {
        CliError::InvalidArguments("Project name is required. Usage: cli new <project-name>".to_string())
    })?;

    let project_directory = PathBuf::from(&project_name);

    if project_directory.exists() {
        return Err(CliError::InvalidArguments(format!(
            "Directory '{}' already exists",
            project_directory.display()
        )));
    }

    std::fs::create_dir_all(&project_directory)
        .map_err(|error| CliError::InvalidArguments(format!("Failed to create directory '{project_name}': {error}")))?;

    let src_directory = project_directory.join("src");
    std::fs::create_dir_all(&src_directory)
        .map_err(|error| CliError::InvalidArguments(format!("Failed to create src directory: {error}")))?;

    let tools_directory = src_directory.join("tools");
    std::fs::create_dir_all(&tools_directory)
        .map_err(|error| CliError::InvalidArguments(format!("Failed to create tools directory: {error}")))?;

    let workflow_file = project_directory.join("workflow.ai");
    let cargo_file = project_directory.join("Cargo.toml");
    let main_file = src_directory.join("main.rs");
    let tools_mod_file = src_directory.join("tools.rs");
    let greeting_tool_file = tools_directory.join("greeting.rs");
    let calculator_tool_file = tools_directory.join("calculator.rs");

    let sample_workflow = include_str!("../templates/workflow.ai");
    let cargo_template = include_str!("../templates/Cargo.toml.template");
    let main_template = include_str!("../templates/main.rs.template");
    let tools_mod_template = include_str!("../templates/tools.rs.template");
    let greeting_tool_template = include_str!("../templates/tools/greeting.rs.template");
    let calculator_tool_template = include_str!("../templates/tools/calculator.rs.template");

    let cargo_content = cargo_template.replace("{{project_name}}", &project_name);

    std::fs::write(&workflow_file, sample_workflow)
        .map_err(|error| CliError::InvalidArguments(format!("Failed to write workflow file: {error}")))?;

    std::fs::write(&cargo_file, cargo_content)
        .map_err(|error| CliError::InvalidArguments(format!("Failed to write Cargo.toml: {error}")))?;

    std::fs::write(&main_file, main_template)
        .map_err(|error| CliError::InvalidArguments(format!("Failed to write main.rs: {error}")))?;

    std::fs::write(&tools_mod_file, tools_mod_template)
        .map_err(|error| CliError::InvalidArguments(format!("Failed to write tools.rs: {error}")))?;

    std::fs::write(&greeting_tool_file, greeting_tool_template)
        .map_err(|error| CliError::InvalidArguments(format!("Failed to write greeting.rs: {error}")))?;

    std::fs::write(&calculator_tool_file, calculator_tool_template)
        .map_err(|error| CliError::InvalidArguments(format!("Failed to write calculator.rs: {error}")))?;

    log::info!("Created new Engine AI project: {}", project_directory.display());
    log::info!("  - Cargo.toml");
    log::info!("  - workflow.ai");
    log::info!("  - src/main.rs");
    log::info!("  - src/tools.rs");
    log::info!("  - src/tools/greeting.rs");
    log::info!("  - src/tools/calculator.rs");
    log::info!("");
    log::info!("Next steps:");
    log::info!("  cd {project_name}");
    log::info!("  cargo run");

    Ok(())
}
