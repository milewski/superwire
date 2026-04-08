use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use superwire_wasm_tool_sdk::host;
use superwire_wasm_tool_sdk::{Tool, ToolExecutionError, ToolMetadata};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PhpWeatherInput {
    /// City name requested by the caller.
    pub city: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PhpWeatherOutput {
    /// City handled by the PHP endpoint.
    pub city: String,

    /// Weather summary returned by the PHP endpoint.
    pub summary: String,

    /// Source attribution returned by the PHP endpoint.
    pub source: String,
}

#[derive(Debug, Deserialize)]
struct PhpWeatherHttpResponse {
    city: Option<String>,
    summary: Option<String>,
    source: Option<String>,
    error: Option<String>,
}

pub struct PhpWeather;

impl Tool for PhpWeather {
    type AgentInput = PhpWeatherInput;
    type BoundInput = PhpWeatherInput;
    type Output = PhpWeatherOutput;

    fn metadata() -> ToolMetadata {
        ToolMetadata::new("php_weather", "Calls a local PHP endpoint through host_http_get")
    }

    async fn execute(agent_input: Self::AgentInput, bound_input: Self::BoundInput) -> Result<Self::Output, ToolExecutionError> {
        let city_name = bound_input
            .city
            .or(agent_input.city)
            .ok_or_else(|| ToolExecutionError::new("missing_city", "missing required argument `city`"))?;

        let encoded_city_name = urlencoding::encode(city_name.trim()).into_owned();
        let request_url = format!("http://127.0.0.1:8099/weather.php?city={encoded_city_name}");

        let response_body = host::http_get(&request_url)
            .map_err(|error| ToolExecutionError::new("php_endpoint_unreachable", error.message))?;

        let parsed_response = serde_json::from_str::<PhpWeatherHttpResponse>(&response_body)
            .map_err(|error| ToolExecutionError::new("php_response_invalid", format!("invalid php endpoint response: {error}")))?;

        if let Some(error_message) = parsed_response.error {
            return Err(ToolExecutionError::new("php_endpoint_error", error_message));
        }

        let Some(parsed_city_name) = parsed_response.city else {
            return Err(ToolExecutionError::new("php_response_missing_city", "php endpoint response is missing `city`"));
        };

        let Some(parsed_summary) = parsed_response.summary else {
            return Err(ToolExecutionError::new(
                "php_response_missing_summary",
                "php endpoint response is missing `summary`",
            ));
        };

        let Some(parsed_source) = parsed_response.source else {
            return Err(ToolExecutionError::new(
                "php_response_missing_source",
                "php endpoint response is missing `source`",
            ));
        };

        Ok(PhpWeatherOutput {
            city: parsed_city_name,
            summary: parsed_summary,
            source: parsed_source,
        })
    }
}
