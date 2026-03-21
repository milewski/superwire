use indoc::formatdoc;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Copy)]
pub enum RecoveryInstruction<'a> {
    MustExitByCallingTool { iteration: usize, tool_name: &'a str },
}

impl<'a> From<RecoveryInstruction<'a>> for String {
    fn from(value: RecoveryInstruction<'a>) -> Self {
        value.to_string()
    }
}

impl Display for RecoveryInstruction<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MustExitByCallingTool { iteration, tool_name } => {
                let message = match iteration {
                    0 => formatdoc! {
                        "
                        You must finish by calling '{tool_name}'.

                        Critical rule: do not return success unless you have a definitive,
                        confident answer that fully satisfies the user's request.

                        If you are missing information, unsure, blocked, or unable to complete
                        any requirement, call '{tool_name}' with failure and include a clear
                        reason describing what prevented completion.
                        ",
                        tool_name = tool_name,
                    },
                    1 => formatdoc! {
                        "
                        Quality gate: treat success as ready to ship.

                        If your answer is uncertain, partial, or speculative, it is not success.
                        In that case call '{tool_name}' with failure and explain the uncertainty
                        or blocker.
                        ",
                        tool_name = tool_name,
                    },
                    2 => formatdoc! {
                        "
                        Reliability rule: false-positive success is worse than failure.

                        When confidence is not high enough to stand behind the final result,
                        call '{tool_name}' with failure and provide the exact limitation.
                        ",
                        tool_name = tool_name,
                    },
                    3 => formatdoc! {
                        "
                        Decision rule: choose success only if every required part is completed
                        correctly and you are confident it is accurate.

                        Otherwise choose failure and call '{tool_name}' with a concrete reason.
                        ",
                        tool_name = tool_name,
                    },
                    _ => formatdoc! {
                        "
                        Final instruction for this turn: call '{tool_name}' now.

                        If there is any doubt, incompleteness, or blocker, return `failure`.
                        Return `success` only with a definitive answer.
                        ",
                        tool_name = tool_name,
                    },
                };

                write!(formatter, "{message}")
            }
        }
    }
}
