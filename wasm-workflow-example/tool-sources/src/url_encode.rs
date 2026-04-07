use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use superwire_wasm_tool_sdk::{Tool, ToolExecutionError, ToolMetadata};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UrlEncodeAgentInput {
    #[schemars(description = "Text value provided by the model")]
    text: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UrlEncodeBoundInput {
    #[schemars(description = "Optional text provided by workflow bindings")]
    text: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct UrlEncodeOutput {
    #[schemars(description = "Original input text")]
    original: String,

    #[schemars(description = "Percent-encoded text")]
    encoded: String,
}

pub struct UrlEncode;

impl Tool for UrlEncode {
    type AgentInput = UrlEncodeAgentInput;
    type BoundInput = UrlEncodeBoundInput;
    type Output = UrlEncodeOutput;

    fn metadata() -> ToolMetadata {
        ToolMetadata::new("url_encode", "Encodes text as a URL-safe string")
    }

    fn execute(agent_input: Self::AgentInput, bound_input: Self::BoundInput) -> Result<Self::Output, ToolExecutionError> {
        let input_text = bound_input
            .text
            .or(agent_input.text)
            .ok_or_else(|| ToolExecutionError::new("missing_text", "missing required argument `text`"))?;

        let encoded_text = urlencoding::encode(&input_text).into_owned();

        Ok(UrlEncodeOutput {
            original: input_text,
            encoded: encoded_text,
        })
    }
}
