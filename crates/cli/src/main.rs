mod args;
mod compile;
mod error;

use args::parse_inputs;
use clap::Parser;
use engine_ai_core::execution::engine::ExecutionEngine;
use engine_ai_core::formatter::Formatter;
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
    #[command(about = "Format .ai files or directories containing .ai files")]
    Fmt {
        #[arg(help = "Path to a .ai file or directory containing .ai files")]
        path: PathBuf,

        #[arg(short, long, help = "Check formatting without making changes")]
        check: bool,
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

#[allow(clippy::too_many_lines)]
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
        Command::Fmt { path, check } => {
            if !path.exists() {
                return Err(CliError::InvalidArguments(format!(
                    "Path '{}' does not exist",
                    path.display()
                )));
            }

            let formatter = Formatter::new();

            if path.is_file() {
                // Handle single file
                if check {
                    log::info!("Checking formatting of file: {}", path.display());

                    let result = formatter.format_file(&path)?;
                    if result.changed {
                        log::warn!("File is not properly formatted: {}", path.display());
                        return Err(CliError::InvalidArguments(
                            "File is not properly formatted. Run without --check to format it.".to_string(),
                        ));
                    }
                    log::info!("File is properly formatted");
                } else {
                    log::info!("Formatting file: {}", path.display());

                    let result = formatter.format_file(&path)?;
                    if result.changed {
                        formatter.write_file(&path, &result.content)?;
                        log::info!("Formatted file: {}", path.display());
                    } else {
                        log::info!("File was already properly formatted");
                    }
                }
            } else if path.is_dir() {
                // Handle directory
                if check {
                    log::info!("Checking formatting of .ai files in: {}", path.display());

                    let unformatted_files = formatter.check_directory(&path)?;

                    if unformatted_files.is_empty() {
                        log::info!("All .ai files are properly formatted");
                    } else {
                        log::warn!("Found {} unformatted files:", unformatted_files.len());
                        for file_path in &unformatted_files {
                            log::warn!("  {}", file_path.display());
                        }
                        return Err(CliError::InvalidArguments(
                            "Some files are not properly formatted. Run without --check to format them.".to_string(),
                        ));
                    }
                } else {
                    log::info!("Formatting .ai files in: {}", path.display());

                    let formatted_files = formatter.format_directory(&path)?;

                    if formatted_files.is_empty() {
                        log::info!("All .ai files were already properly formatted");
                    } else {
                        log::info!("Formatted {} files:", formatted_files.len());
                        for file_path in &formatted_files {
                            log::info!("  {}", file_path.display());
                        }
                    }
                }
            } else {
                return Err(CliError::InvalidArguments(format!(
                    "'{}' is neither a file nor a directory",
                    path.display()
                )));
            }

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

    let workflow_file = src_directory.join("workflow.ai");
    let cargo_file = project_directory.join("Cargo.toml");
    let main_file = src_directory.join("main.rs");
    let tools_mod_file = src_directory.join("tools.rs");
    let bash_tool_file = tools_directory.join("bash.rs");

    let sample_workflow = include_str!("../templates/workflow.ai");
    let cargo_template = include_str!("../templates/Cargo.toml.template");
    let main_template = include_str!("../templates/main.rs.template");
    let tools_mod_template = include_str!("../templates/tools.rs.template");
    let bash_tool_template = include_str!("../templates/tools/bash.rs.template");

    let cargo_content = cargo_template.replace("{{project_name}}", &project_name);

    std::fs::write(&workflow_file, sample_workflow)
        .map_err(|error| CliError::InvalidArguments(format!("Failed to write workflow file: {error}")))?;

    std::fs::write(&cargo_file, cargo_content)
        .map_err(|error| CliError::InvalidArguments(format!("Failed to write Cargo.toml: {error}")))?;

    std::fs::write(&main_file, main_template)
        .map_err(|error| CliError::InvalidArguments(format!("Failed to write main.rs: {error}")))?;

    std::fs::write(&tools_mod_file, tools_mod_template)
        .map_err(|error| CliError::InvalidArguments(format!("Failed to write tools.rs: {error}")))?;

    std::fs::write(&bash_tool_file, bash_tool_template)
        .map_err(|error| CliError::InvalidArguments(format!("Failed to write bash.rs: {error}")))?;

    log::info!("Created new Engine AI project: {}", project_directory.display());
    log::info!("  - Cargo.toml");
    log::info!("  - src/workflow.ai");
    log::info!("  - src/main.rs");
    log::info!("  - src/tools.rs");
    log::info!("  - src/tools/bash.rs");
    log::info!("");
    log::info!("Next steps:");
    log::info!("  cd {project_name}");
    log::info!("  cargo run");

    Ok(())
}
