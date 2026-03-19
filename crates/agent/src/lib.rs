mod agent;
mod context;
mod error;
mod executor;
mod message;
pub mod providers;
pub mod tool;
mod traits;

pub use agent::{Agent, AgentConfig};
pub use context::Context;
pub use error::{AgentError, ValidationError};
pub use executor::LoopExecutor;
pub use message::{Message, MessageRole, ToolCall, ToolResult};
pub use providers::{OllamaProvider, OpenAIProvider};
pub use tool::{DoneTool, Tool, ToolError};
pub use traits::{Executable, Provider, ProviderResponse, StopReason, Validator};
