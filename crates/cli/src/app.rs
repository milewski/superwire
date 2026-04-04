use clap::{Parser, Subcommand};

use crate::commands::fmt::FormatCommand;
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
                eprintln!("{command_error}");
                ExitStatus::from_exit_code(command_error.exit_code())
            }
        }
    }
}

#[derive(Debug, Parser)]
#[command(name = "cli")]
#[command(about = "Engine AI CLI")]
pub struct CommandLineArguments {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Fmt(FormatCommand),
}

impl Command {
    fn execute(self) -> Result<(), CommandError> {
        match self {
            Self::Fmt(format_command) => format_command.execute(),
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
