use crate::superwire_wasm_tool_sdk::{Tool, ToolExecutionError, ToolMetadata};

mod schema_bindings {
    wit_bindgen::generate!({
        path: "wit/php_weather.wit",
        world: "tool-schema",
        additional_derives: [serde::Deserialize, serde::Serialize, schemars::JsonSchema],
    });
}

type PhpWeatherInput = schema_bindings::superwire::tool::schema::AgentInput;
type PhpWeatherBoundInput = schema_bindings::superwire::tool::schema::BoundInput;
type PhpWeatherOutput = schema_bindings::superwire::tool::schema::Output;

pub struct PhpWeather;

impl Tool for PhpWeather {
    type AgentInput = PhpWeatherInput;
    type BoundInput = PhpWeatherBoundInput;
    type Output = PhpWeatherOutput;

    fn metadata() -> ToolMetadata {
        ToolMetadata::new("php_weather", "Proxy tool forwarding execution to Laravel WeatherTool")
    }

    async fn execute(agent_input: Self::AgentInput, bound_input: Self::BoundInput) -> Result<Self::Output, ToolExecutionError> {
        let internal_token = bound_input.internal_token.clone();
        let request_payload = serde_json::json!({
            "agent_input": agent_input,
            "bound_input": bound_input,
        });
        let request_body = serde_json::to_string(&request_payload)
            .map_err(|error| ToolExecutionError::new("serialize_request_error", error.to_string()))?;
        let response_body = crate::superwire_wasm_tool_sdk::host::http_post_json(
            "http://127.0.0.1:8099/superwire/tools/weather/execute",
            &request_body,
            internal_token.as_deref(),
        )?;

        serde_json::from_str::<Self::Output>(&response_body)
            .map_err(|error| ToolExecutionError::new("parse_response_error", error.to_string()))
    }
}
