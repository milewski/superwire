use engine_ai_core::dsl::DslParseError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CliError {
    #[error("{usage}")]
    MissingCommand { usage: String },

    #[error("unknown command `{command_name}`\n\n{usage}")]
    UnknownCommand { command_name: String, usage: String },

    #[error("missing required source file for `fmt`\n\n{usage}")]
    MissingFormatPath { usage: String },

    #[error("unexpected extra argument `{argument}` for `fmt`\n\n{usage}")]
    UnexpectedFormatArgument { argument: String, usage: String },

    #[error("failed to read source file `{path}`: {source}")]
    ReadSourceFile {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse DSL file `{path}`: {source}")]
    ParseSourceFile {
        path: String,
        #[source]
        source: DslParseError,
    },

    #[error("failed to write formatted source file `{path}`: {source}")]
    WriteSourceFile {
        path: String,
        #[source]
        source: std::io::Error,
    },
}
