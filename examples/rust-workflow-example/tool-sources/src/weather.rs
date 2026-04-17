use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use crate::superwire_wasm_tool_sdk::{Tool, ToolExecutionError, ToolMetadata};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WeatherInput {
    pub city: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WeatherBoundInput {
    pub city: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct WeatherOutput {
    pub city: String,
    pub summary: String,
}

pub struct Weather;

impl Tool for Weather {
    type AgentInput = WeatherInput;
    type BoundInput = WeatherBoundInput;
    type Output = WeatherOutput;

    fn metadata() -> ToolMetadata {
        ToolMetadata::new("weather", "Returns a weather summary for a city")
    }

    async fn execute(agent_input: Self::AgentInput, bound_input: Self::BoundInput) -> Result<Self::Output, ToolExecutionError> {
        let city_name = bound_input
            .city
            .or(agent_input.city)
            .ok_or_else(|| ToolExecutionError::new("missing_city", "missing required city parameter"))?;

        let weather_summary = resolve_weather_summary(&city_name);

        Ok(WeatherOutput {
            city: city_name,
            summary: weather_summary,
        })
    }
}

fn resolve_weather_summary(city_name: &str) -> String {
    let weather_options = [
        "clear skies, 22C",
        "light rain, 18C",
        "overcast, 16C",
        "sunny, 27C",
        "windy, 19C",
        "partly cloudy, 21C",
    ];

    let mut city_hash_value = 0u64;

    for city_byte in city_name.bytes() {
        city_hash_value = city_hash_value.wrapping_mul(131).wrapping_add(u64::from(city_byte));
    }

    let weather_index = (city_hash_value % weather_options.len() as u64) as usize;

    weather_options[weather_index].to_string()
}
