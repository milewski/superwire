use clap::{Parser, Subcommand};
use serde_json::json;

use crate::commands::fmt::FormatCommand;
use crate::commands::tool::ToolCommand;
use crate::commands::tools::ToolsCommand;
use crate::commands::workflow::WorkflowCommand;
use crate::diagnostics::CommandError;

pub struct Application {
    arguments: CommandLineArguments,
}

impl Application {
    pub fn from_environment() -> Self {
        Self {
            arguments: CommandLineArguments::parse(),
        }
    }

    pub fn run(self) -> ExitStatus {
        match self.arguments.command.execute() {
            Ok(()) => ExitStatus::from_exit_code(ExitCode::Success),
            Err(command_error) => {
                if std::env::var("SUPERWIRE_ERROR_FORMAT").ok().as_deref() == Some("json") {
                    let error_payload = json!({
                        "code": command_error.code(),
                        "message": command_error.message(),
                        "details": command_error.details(),
                    });

                    eprintln!("{error_payload}");
                } else {
                    eprintln!("{command_error}");
                }

                ExitStatus::from_exit_code(command_error.exit_code())
            }
        }
    }
}

#[derive(Debug, Parser)]
#[command(name = "cli")]
#[command(about = "SuperWire CLI")]
pub struct CommandLineArguments {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Fmt(FormatCommand),
    Tool(ToolCommand),
    Tools(ToolsCommand),
    Workflow(WorkflowCommand),
}

impl Command {
    fn execute(self) -> Result<(), CommandError> {
        match self {
            Self::Fmt(format_command) => format_command.execute(),
            Self::Tool(tool_command) => tool_command.execute(),
            Self::Tools(tools_command) => tools_command.execute(),
            Self::Workflow(workflow_command) => workflow_command.execute(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExitCode {
    Success,
    InvalidInput,
    InternalError,
}

impl ExitCode {
    pub const fn code(self) -> i32 {
        match self {
            Self::Success => 0,
            Self::InvalidInput => 2,
            Self::InternalError => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExitStatus {
    exit_code: ExitCode,
}

impl ExitStatus {
    pub const fn from_exit_code(exit_code: ExitCode) -> Self {
        Self { exit_code }
    }

    pub const fn code(self) -> i32 {
        self.exit_code.code()
    }
}
