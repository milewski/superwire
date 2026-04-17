use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::Serialize;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
pub struct BuiltToolDefinitionJson {
    pub name: String,
    pub description: String,
    pub parameters_schema_json: String,
    pub bound_parameters_schema_json: String,
    pub output_schema_json: String,
}

#[allow(async_fn_in_trait)]
pub trait Tool {
    type AgentInput: DeserializeOwned + JsonSchema;
    type BoundInput: DeserializeOwned + JsonSchema;
    type Output: Serialize + JsonSchema;

    fn metadata() -> ToolMetadata;

    async fn execute(agent_input: Self::AgentInput, bound_input: Self::BoundInput) -> Result<Self::Output, ToolExecutionError>;
}

pub fn build_tool_definition_json<ToolType>() -> Result<BuiltToolDefinitionJson, String>
where
    ToolType: Tool,
{
    let metadata = ToolType::metadata();
    let parameters_schema = schemars::schema_for!(ToolType::AgentInput);
    let bound_parameters_schema = schemars::schema_for!(ToolType::BoundInput);
    let output_schema = schemars::schema_for!(ToolType::Output);

    let parameters_schema_json =
        serde_json::to_string(&parameters_schema).map_err(|error| format!("failed to serialize parameters schema: {error}"))?;
    let bound_parameters_schema_json =
        serde_json::to_string(&bound_parameters_schema).map_err(|error| format!("failed to serialize bound parameters schema: {error}"))?;
    let output_schema_json =
        serde_json::to_string(&output_schema).map_err(|error| format!("failed to serialize output schema: {error}"))?;

    Ok(BuiltToolDefinitionJson {
        name: metadata.name,
        description: metadata.description,
        parameters_schema_json,
        bound_parameters_schema_json,
        output_schema_json,
    })
}

pub fn execute_tool_json_blocking<ToolType>(agent_input_json: &str, bound_input_json: &str) -> Result<String, ToolExecutionError>
where
    ToolType: Tool,
{
    let parsed_agent_input = serde_json::from_str::<ToolType::AgentInput>(agent_input_json)
        .map_err(|error| ToolExecutionError::new("invalid_agent_input", format!("failed to parse agent input json: {error}")))?;

    let parsed_bound_input = serde_json::from_str::<ToolType::BoundInput>(bound_input_json)
        .map_err(|error| ToolExecutionError::new("invalid_bound_input", format!("failed to parse bound input json: {error}")))?;

    let execution_output = pollster::block_on(ToolType::execute(parsed_agent_input, parsed_bound_input))?;

    serde_json::to_string(&execution_output)
        .map_err(|error| ToolExecutionError::new("serialization_error", format!("failed to serialize tool output json: {error}")))
}

pub mod host {
    use super::ToolExecutionError;
    use std::sync::{OnceLock, RwLock};

    type HttpGetDelegate = fn(&str) -> Result<String, String>;
    type HttpPostJsonDelegate = fn(&str, &str, Option<&str>) -> Result<String, String>;

    static HTTP_GET_DELEGATE: OnceLock<RwLock<Option<HttpGetDelegate>>> = OnceLock::new();
    static HTTP_POST_JSON_DELEGATE: OnceLock<RwLock<Option<HttpPostJsonDelegate>>> = OnceLock::new();

    fn http_get_delegate_storage() -> &'static RwLock<Option<HttpGetDelegate>> {
        HTTP_GET_DELEGATE.get_or_init(|| RwLock::new(None))
    }

    fn http_post_json_delegate_storage() -> &'static RwLock<Option<HttpPostJsonDelegate>> {
        HTTP_POST_JSON_DELEGATE.get_or_init(|| RwLock::new(None))
    }

    pub fn register_http_get(delegate: HttpGetDelegate) {
        let mut storage = http_get_delegate_storage().write().expect("http_get delegate lock poisoned");
        *storage = Some(delegate);
    }

    pub fn register_http_post_json(delegate: HttpPostJsonDelegate) {
        let mut storage = http_post_json_delegate_storage()
            .write()
            .expect("http_post_json delegate lock poisoned");
        *storage = Some(delegate);
    }

    pub fn http_get(request_url: &str) -> Result<String, ToolExecutionError> {
        let storage = http_get_delegate_storage().read().expect("http_get delegate lock poisoned");
        let Some(delegate) = *storage else {
            return Err(ToolExecutionError::new(
                "host_not_registered",
                "http_get delegate is not registered",
            ));
        };

        delegate(request_url).map_err(|message| ToolExecutionError::new("host_http_get_failed", message))
    }

    pub fn http_post_json(request_url: &str, request_body_json: &str, internal_token: Option<&str>) -> Result<String, ToolExecutionError> {
        let storage = http_post_json_delegate_storage()
            .read()
            .expect("http_post_json delegate lock poisoned");
        let Some(delegate) = *storage else {
            return Err(ToolExecutionError::new(
                "host_not_registered",
                "http_post_json delegate is not registered",
            ));
        };

        delegate(request_url, request_body_json, internal_token)
            .map_err(|message| ToolExecutionError::new("host_http_post_json_failed", message))
    }
}

#[macro_export]
macro_rules! php_proxy_tool {
    ($($token:tt)*) => {};
}
