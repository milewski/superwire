use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PhpWeatherInput {
    /// Optional city coming from model arguments.
    pub city: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PhpWeatherBoundInput {
    /// Optional city coming from workflow binding.
    pub city: Option<String>,

    /// Shared internal token validated by Laravel middleware.
    pub internal_token: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PhpWeatherOutput {
    /// City resolved by the PHP tool implementation.
    pub city: String,

    /// Weather summary text.
    pub summary: String,

    /// Source attribution.
    pub source: String,
}

superwire_wasm_tool_sdk::php_proxy_tool!(
    tool = PhpWeather,
    name = "php_weather",
    description = "Proxy tool forwarding execution to Laravel WeatherTool",
    endpoint = "http://127.0.0.1:8099/superwire/tools/weather/execute",
    input = PhpWeatherInput,
    bound_input = PhpWeatherBoundInput,
    output = PhpWeatherOutput,
    token_field = "internal_token",
);
