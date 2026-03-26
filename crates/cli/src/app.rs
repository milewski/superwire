use clap::{Parser, Subcommand};

use crate::commands::build::BuildCommand;
use crate::commands::check::CheckCommand;
use crate::commands::fmt::FormatCommand;
use crate::commands::run::RunCommand;
use crate::diagnostics::CommandError;

pub struct Application {
    arguments: CommandLineArguments,
}

impl Application {
    pub fn from_environment() -> Self {
        let arguments = CommandLineArguments::parse();

        Self { arguments }
    }

    pub fn run(self) -> ExitStatus {
        let execution_result = self.arguments.command.execute();

        match execution_result {
            Ok(()) => ExitStatus::from_exit_code(ExitCode::Success),
            Err(command_error) => {
                eprintln!("{command_error}");

                ExitStatus::from_exit_code(command_error.exit_code())
            }
        }
    }
}

#[derive(Debug, Parser)]
#[command(name = "engine-ai")]
#[command(about = "Workflow CLI for Engine AI")]
pub struct CommandLineArguments {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Check(CheckCommand),
    Fmt(FormatCommand),
    Run(RunCommand),
    Build(BuildCommand),
}

impl Command {
    fn execute(self) -> Result<(), CommandError> {
        match self {
            Self::Check(check_command) => check_command.execute(),
            Self::Fmt(format_command) => format_command.execute(),
            Self::Run(run_command) => run_command.execute(),
            Self::Build(build_command) => build_command.execute(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExitCode {
    Success,
    InvalidWorkflow,
    RuntimeFailure,
    InternalError,
}

impl ExitCode {
    pub const fn code(self) -> i32 {
        match self {
            Self::Success => 0,
            Self::InvalidWorkflow => 2,
            Self::RuntimeFailure => 3,
            Self::InternalError => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExitStatus {
    code: ExitCode,
}

impl ExitStatus {
    pub const fn from_exit_code(code: ExitCode) -> Self {
        Self { code }
    }

    pub const fn code(self) -> i32 {
        self.code.code()
    }
}
