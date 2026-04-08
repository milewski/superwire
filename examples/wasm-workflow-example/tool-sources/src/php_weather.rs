use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PhpWeatherInput {
    /// City name requested by the caller.
    pub city: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PhpWeatherBoundInput {
    /// City name requested by the caller.
    pub city: Option<String>,

    /// Internal token used by the PHP endpoint guard.
    pub internal_token: Option<String>,
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

superwire_wasm_tool_sdk::php_proxy_tool!(
    tool = PhpWeather,
    name = "php_weather",
    description = "Calls a local PHP endpoint through host_http_post_json",
    endpoint = "http://127.0.0.1:8099/weather.php",
    input = PhpWeatherInput,
    bound_input = PhpWeatherBoundInput,
    output = PhpWeatherOutput,
    token_field = "internal_token",
);
