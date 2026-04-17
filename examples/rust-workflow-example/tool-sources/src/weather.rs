use crate::superwire_wasm_tool_sdk::{Tool, ToolExecutionError, ToolMetadata};

mod schema_bindings {
    wit_bindgen::generate!({
        path: "wit/weather.wit",
        world: "tool-schema",
        additional_derives: [serde::Deserialize, serde::Serialize, schemars::JsonSchema],
    });
}

use crate::tool_metadata_from_wit;

pub struct Weather;

impl Tool for Weather {
    type AgentInput = schema_bindings::superwire::tool::schema::AgentInput;
    type BoundInput = schema_bindings::superwire::tool::schema::BoundInput;
    type Output = schema_bindings::superwire::tool::schema::Output;

    fn metadata() -> ToolMetadata {
        tool_metadata_from_wit!("../wit/weather.wit")
    }

    async fn execute(agent_input: Self::AgentInput, bound_input: Self::BoundInput) -> Result<Self::Output, ToolExecutionError> {
        let city_name = bound_input
            .city
            .or(agent_input.city)
            .ok_or_else(|| ToolExecutionError::new("missing_city", "missing required city parameter"))?;

        let weather_summary = resolve_weather_summary(&city_name);

        Ok(Self::Output {
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
