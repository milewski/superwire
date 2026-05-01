use crate::runtime::ExecutorError;
use serde_json::Value;

pub fn parse_model_json_output(agent_name: &str, content: &str) -> Result<Value, ExecutorError> {
    let trimmed_content = content.trim();

    if trimmed_content.is_empty() {
        return Err(ExecutorError::Model {
            agent_name: agent_name.to_string(),
            message: "model response did not include assistant content".to_string(),
        });
    }

    let json_candidate = strip_markdown_json_fence(trimmed_content);

    serde_json::from_str(json_candidate).map_err(|error| ExecutorError::Model {
        agent_name: agent_name.to_string(),
        message: format!("model response was not valid JSON: {error}; response content: {content}"),
    })
}

pub fn normalize_mcp_tool_result(result: Value) -> Value {
    if let Some(structured_content) = result.get("structuredContent") {
        return structured_content.clone();
    }

    if let Some(text_content) = result
        .get("content")
        .and_then(Value::as_array)
        .and_then(|content| content.first())
        .and_then(|content_item| content_item.get("text"))
        .and_then(Value::as_str)
    {
        return serde_json::from_str(text_content).unwrap_or_else(|_| Value::String(text_content.to_string()));
    }

    result
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
