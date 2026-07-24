use crate::error::ModelProviderError;
use serde_json::Value;

pub fn parse_model_json_output(agent_name: &str, content: &str) -> Result<Value, ModelProviderError> {
    let trimmed_content = content.trim();

    if trimmed_content.is_empty() {
        return Err(ModelProviderError::rejected(
            agent_name,
            "model response did not include assistant content",
        ));
    }

    let json_candidate = strip_markdown_json_fence(trimmed_content);

    serde_json::from_str(json_candidate).map_err(|error| {
        ModelProviderError::rejected_with_source(
            agent_name,
            format!("model response was not valid JSON: {error}; response content: {content}"),
            error,
        )
    })
}

fn strip_markdown_json_fence(content: &str) -> &str {
    let Some(stripped_start) = content.strip_prefix("```") else {
        return content;
    };

    let stripped_start = stripped_start.strip_prefix("json").unwrap_or(stripped_start).trim_start();
    let Some(stripped_end) = stripped_start.strip_suffix("```") else {
        return content;
    };

    stripped_end.trim()
}

#[cfg(test)]
mod tests {
    use super::parse_model_json_output;
    use serde_json::json;

    #[test]
    fn parses_plain_json_model_output() {
        let output = parse_model_json_output("agent", r#"{"message":"hello"}"#).expect("json should parse");

        assert_eq!(output, json!({ "message": "hello" }));
    }

    #[test]
    fn parses_json_output_wrapped_in_markdown_fence() {
        let output = parse_model_json_output("agent", "```json\n{\"message\":\"hello\"}\n```").expect("json should parse");

        assert_eq!(output, json!({ "message": "hello" }));
    }
}
