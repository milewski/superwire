mod agent;
mod context;
mod error;
mod executor;
mod json_validation;
mod message;
pub mod providers;
mod recovery_instruction;
pub mod tool;
mod traits;

#[cfg(test)]
mod tests;

#[doc(hidden)]
pub use inventory;

pub use agent::{Agent, AgentConfig, AgentRunResult, AgentRunStatistics};
pub use context::Context;
pub use error::{AgentError, ProviderError};
pub use executor::LoopExecutor;
pub use json_validation::{validate_json_against_schema_with_context, JsonValidationError, JsonValidationIssue};
pub use message::{Message, ToolCall, ToolResult};
pub use providers::{OllamaProvider, OpenAIProvider};
pub use tool::{DynamicTool, FinalizeOutput, FinalizeTool, RuntimeTool, Tool, ToolError};
pub use traits::{Executable, Provider, ProviderResponse, StopReason, ToolDefinition};
