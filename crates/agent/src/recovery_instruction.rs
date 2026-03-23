use indoc::formatdoc;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Copy)]
pub enum RecoveryInstruction<'a> {
    MustExitByCallingTool { tool_name: &'a str },
    MustCallToolAloneToFinish { tool_name: &'a str },
}

impl<'a> From<RecoveryInstruction<'a>> for String {
    fn from(value: RecoveryInstruction<'a>) -> Self {
        value.to_string()
    }
}

impl Display for RecoveryInstruction<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MustExitByCallingTool { tool_name } => {
                let message = formatdoc! {
                    "
                    You must finish by calling '{tool_name}'.

                    Critical rule: do not return success unless you have a definitive,
                    confident answer that fully satisfies the user's request.

                    If you are missing information, unsure, blocked, or unable to complete
                    any requirement, call '{tool_name}' with failure and include a clear
                    reason describing what prevented completion.
                    ",
                    tool_name = tool_name,
                };

                write!(formatter, "{message}")
            }

            Self::MustCallToolAloneToFinish { tool_name } => {
                let message = formatdoc! {
                    "
                    Ignored '{tool_name}' tool call because it was returned together
                    with other tool calls.

                    Call '{tool_name}' alone to finish.
                    ",
                    tool_name = tool_name,
                };

                write!(formatter, "{message}")
            }
        }
    }
}
