use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use crate::superwire_wasm_tool_sdk::{Tool, ToolExecutionError, ToolMetadata};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RandomCityInput {}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RandomCityBoundInput {}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RandomCityOutput {
    pub city: String,
}

pub struct RandomCity;

impl Tool for RandomCity {
    type AgentInput = RandomCityInput;
    type BoundInput = RandomCityBoundInput;
    type Output = RandomCityOutput;

    fn metadata() -> ToolMetadata {
        ToolMetadata::new("random_city", "Returns a city name")
    }

    async fn execute(_agent_input: Self::AgentInput, _bound_input: Self::BoundInput) -> Result<Self::Output, ToolExecutionError> {
        let city_options = [
            "Lisbon",
            "Tokyo",
            "Nairobi",
            "Sao Paulo",
            "Reykjavik",
            "Seoul",
        ];

        let selected_city = city_options[2];

        Ok(RandomCityOutput {
            city: selected_city.to_string(),
        })
    }
}
