use indoc::formatdoc;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Copy)]
pub enum RecoveryInstruction<'a> {
    MustExitByCallingCompletionTool {
        success_tool_name: &'a str,
        error_tool_name: &'a str,
    },
    MustCallCompletionToolAloneToFinish {
        success_tool_name: &'a str,
        error_tool_name: &'a str,
    },
}

impl<'a> From<RecoveryInstruction<'a>> for String {
    fn from(value: RecoveryInstruction<'a>) -> Self {
        value.to_string()
    }
}

impl Display for RecoveryInstruction<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MustExitByCallingCompletionTool {
                success_tool_name,
                error_tool_name,
            } => {
                let message = formatdoc! {
                    "
                    You must finish by calling one completion tool: either '{success_tool_name}' or '{error_tool_name}'.

                    Critical rule: do not return success unless you have a definitive,
                    confident answer that fully satisfies the user's request.

                    If you are missing information, unsure, blocked, or unable to complete
                    any requirement, call '{error_tool_name}' and include a clear reason
                    describing what prevented completion.
                    ",
                    success_tool_name = success_tool_name,
                    error_tool_name = error_tool_name,
                };

                write!(formatter, "{message}")
            }

            Self::MustCallCompletionToolAloneToFinish {
                success_tool_name,
                error_tool_name,
            } => {
                let message = formatdoc! {
                    "
                    Ignored completion tool call because it was returned together with
                    other tool calls.

                    Call either '{success_tool_name}' or '{error_tool_name}' alone to finish.
                    ",
                    success_tool_name = success_tool_name,
                    error_tool_name = error_tool_name,
                };

                write!(formatter, "{message}")
            }
        }
    }
}
