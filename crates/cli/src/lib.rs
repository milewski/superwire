mod command;
mod commands;
mod error;
pub mod formatter;

pub use error::CliError;

pub fn run_from_environment() -> Result<(), CliError> {
    let command = command::Command::parse_from_environment()?;

    command.execute()
}
