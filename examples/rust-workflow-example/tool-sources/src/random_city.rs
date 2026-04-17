use crate::superwire_wasm_tool_sdk::{Tool, ToolExecutionError, ToolMetadata};

mod schema_bindings {
    wit_bindgen::generate!({
        path: "wit/random_city.wit",
        world: "tool-schema",
        additional_derives: [serde::Deserialize, serde::Serialize, schemars::JsonSchema],
    });
}

use crate::tool_metadata_from_wit;

pub struct RandomCity;

impl Tool for RandomCity {
    type AgentInput = schema_bindings::superwire::tool::schema::AgentInput;
    type BoundInput = schema_bindings::superwire::tool::schema::BoundInput;
    type Output = schema_bindings::superwire::tool::schema::Output;

    fn metadata() -> ToolMetadata {
        tool_metadata_from_wit!("../wit/random_city.wit")
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

        Ok(Self::Output {
            city: selected_city.to_string(),
        })
    }
}
