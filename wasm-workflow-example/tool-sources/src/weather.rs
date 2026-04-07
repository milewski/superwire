use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use superwire_wasm_tool_sdk::host;
use superwire_wasm_tool_sdk::{Tool, ToolExecutionError, ToolMetadata};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct OptionalWeatherInput {
    /// Optional city name provided by the model.
    city: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BoundWeatherInput {
    /// Optional city name bound by workflow input or secrets.
    city: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct WeatherOutput {
    /// City that was requested.
    city: String,

    /// Current weather summary returned by wttr.in.
    summary: String,

    /// Data source name.
    source: String,
}

pub struct Weather;

impl Tool for Weather {
    type AgentInput = OptionalWeatherInput;
    type BoundInput = BoundWeatherInput;
    type Output = WeatherOutput;

    fn metadata() -> ToolMetadata {
        ToolMetadata::new("weather", "Fetches current weather from wttr.in")
    }

    async fn execute(agent_input: Self::AgentInput, bound_input: Self::BoundInput) -> Result<Self::Output, ToolExecutionError> {
        let city_name = bound_input
            .city
            .or(agent_input.city)
            .ok_or_else(|| ToolExecutionError::new(WeatherToolErrorCode::MissingCity.as_str(), "missing required argument `city`"))?;

        let encoded_city_name = encode_url_path_segment(city_name.trim());
        let weather_service_url = format!("https://wttr.in/{encoded_city_name}?format=%C+%t");

        let weather_service_response = host::http_get(&weather_service_url)
            .map_err(|error| ToolExecutionError::new(WeatherToolErrorCode::HttpError.as_str(), error.message))?;

        let weather_summary = weather_service_response.trim();

        if weather_summary.is_empty() {
            return Err(ToolExecutionError::new(
                WeatherToolErrorCode::EmptyResponse.as_str(),
                "weather service returned an empty response",
            ));
        }

        Ok(WeatherOutput {
            city: city_name,
            summary: weather_summary.to_string(),
            source: "wttr.in".to_string(),
        })
    }
}

#[derive(Debug, Clone, Copy)]
enum WeatherToolErrorCode {
    MissingCity,
    HttpError,
    EmptyResponse,
}

impl WeatherToolErrorCode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::MissingCity => "missing_city",
            Self::HttpError => "http_error",
            Self::EmptyResponse => "empty_response",
        }
    }
}

fn encode_url_path_segment(path_segment: &str) -> String {
    let mut encoded_path_segment = String::new();

    for path_segment_byte in path_segment.bytes() {
        if path_segment_byte.is_ascii_alphanumeric() || path_segment_byte == b'-' || path_segment_byte == b'_' {
            encoded_path_segment.push(char::from(path_segment_byte));

            continue;
        }

        encoded_path_segment.push('%');
        encoded_path_segment.push_str(&format!("{path_segment_byte:02X}"));
    }

    encoded_path_segment
}
