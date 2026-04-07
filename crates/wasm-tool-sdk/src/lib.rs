use schemars::{schema_for, JsonSchema};
use serde::de::DeserializeOwned;
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct ToolMetadata {
    pub name: String,
    pub description: String,
}

impl ToolMetadata {
    #[must_use]
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ToolDefinitionJson {
    pub name: String,
    pub description: String,
    pub parameters_schema_json: String,
    pub bound_parameters_schema_json: String,
    pub output_schema_json: String,
}

#[derive(Debug, Clone)]
pub struct ToolExecutionError {
    pub code: String,
    pub message: String,
}

impl ToolExecutionError {
    #[must_use]
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

#[allow(async_fn_in_trait)]
pub trait Tool {
    type AgentInput: DeserializeOwned + JsonSchema;
    type BoundInput: DeserializeOwned + JsonSchema;
    type Output: Serialize + JsonSchema;

    fn metadata() -> ToolMetadata;

    async fn execute(agent_input: Self::AgentInput, bound_input: Self::BoundInput) -> Result<Self::Output, ToolExecutionError>;
}

#[derive(Debug, Clone, Copy)]
enum StandardToolErrorCode {
    InvalidAgentInput,
    InvalidBoundInput,
    OutputSerializationFailed,
    HostHttpGetUnavailable,
    HostHttpGetFailed,
}

impl StandardToolErrorCode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidAgentInput => "invalid_agent_input",
            Self::InvalidBoundInput => "invalid_bound_input",
            Self::OutputSerializationFailed => "output_serialization_failed",
            Self::HostHttpGetUnavailable => "host_http_get_unavailable",
            Self::HostHttpGetFailed => "host_http_get_failed",
        }
    }
}

pub fn build_tool_definition_json<ToolType>() -> Result<ToolDefinitionJson, String>
where
    ToolType: Tool,
{
    let tool_metadata = ToolType::metadata();

    Ok(ToolDefinitionJson {
        name: tool_metadata.name,
        description: tool_metadata.description,
        parameters_schema_json: schema_json::<ToolType::AgentInput>()?,
        bound_parameters_schema_json: schema_json::<ToolType::BoundInput>()?,
        output_schema_json: schema_json::<ToolType::Output>()?,
    })
}

pub async fn execute_tool_json<ToolType>(agent_input_json: &str, bound_input_json: &str) -> Result<String, ToolExecutionError>
where
    ToolType: Tool,
{
    let parsed_agent_input = parse_json::<ToolType::AgentInput>(agent_input_json, StandardToolErrorCode::InvalidAgentInput)?;
    let parsed_bound_input = parse_json::<ToolType::BoundInput>(bound_input_json, StandardToolErrorCode::InvalidBoundInput)?;

    let execution_output = ToolType::execute(parsed_agent_input, parsed_bound_input).await?;

    serde_json::to_string(&execution_output).map_err(|error| {
        ToolExecutionError::new(
            StandardToolErrorCode::OutputSerializationFailed.as_str(),
            format!("failed to serialize tool output: {error}"),
        )
    })
}

pub fn execute_tool_json_blocking<ToolType>(agent_input_json: &str, bound_input_json: &str) -> Result<String, ToolExecutionError>
where
    ToolType: Tool,
{
    pollster::block_on(execute_tool_json::<ToolType>(agent_input_json, bound_input_json))
}

fn schema_json<SchemaType>() -> Result<String, String>
where
    SchemaType: JsonSchema,
{
    serde_json::to_string(&schema_for!(SchemaType))
        .map_err(|error| format!("failed to serialize schema for `{}`: {error}", std::any::type_name::<SchemaType>()))
}

fn parse_json<ValueType>(json_payload: &str, error_code: StandardToolErrorCode) -> Result<ValueType, ToolExecutionError>
where
    ValueType: DeserializeOwned,
{
    serde_json::from_str::<ValueType>(json_payload)
        .map_err(|error| ToolExecutionError::new(error_code.as_str(), format!("invalid json payload: {error}")))
}

pub mod host {
    use super::{StandardToolErrorCode, ToolExecutionError};
    use std::sync::OnceLock;

    type HostHttpGetFunction = fn(&str) -> Result<String, String>;

    static REGISTERED_HTTP_GET_FUNCTION: OnceLock<HostHttpGetFunction> = OnceLock::new();

    pub fn register_http_get(delegate: HostHttpGetFunction) {
        let _ = REGISTERED_HTTP_GET_FUNCTION.set(delegate);
    }

    pub fn http_get(request_url: &str) -> Result<String, ToolExecutionError> {
        let Some(http_get_function) = REGISTERED_HTTP_GET_FUNCTION.get().copied() else {
            return Err(ToolExecutionError::new(
                StandardToolErrorCode::HostHttpGetUnavailable.as_str(),
                "host http-get capability is not registered",
            ));
        };

        http_get_function(request_url)
            .map_err(|error_message| ToolExecutionError::new(StandardToolErrorCode::HostHttpGetFailed.as_str(), error_message))
    }
}
