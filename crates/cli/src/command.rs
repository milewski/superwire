use crate::commands::format::FormatCommand;
use crate::error::CliError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommandKind {
    Format,
}

impl CommandKind {
    fn from_argument(command_argument: &str) -> Option<Self> {
        match command_argument {
            "fmt" => Some(Self::Format),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum Command {
    Format(FormatCommand),
}

impl Command {
    pub fn parse_from_environment() -> Result<Self, CliError> {
        let mut command_line_arguments = std::env::args();
        let executable_name = command_line_arguments.next().unwrap_or_else(|| "engine-ai-cli".to_owned());

        let usage = usage_for_executable(&executable_name);

        let Some(command_argument) = command_line_arguments.next() else {
            return Err(CliError::MissingCommand { usage });
        };

        let Some(command_kind) = CommandKind::from_argument(&command_argument) else {
            return Err(CliError::UnknownCommand {
                command_name: command_argument,
                usage,
            });
        };

        match command_kind {
            CommandKind::Format => Ok(Self::Format(FormatCommand::parse(command_line_arguments, usage)?)),
        }
    }

    pub fn execute(self) -> Result<(), CliError> {
        match self {
            Self::Format(format_command) => format_command.execute(),
        }
    }
}

fn usage_for_executable(executable_name: &str) -> String {
    format!("Usage:\n  {executable_name} fmt <source-file>\n\nCommands:\n  fmt    Format a DSL workflow file in place")
}
