use serde_json::Value;

#[must_use]
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
        return serde_json::from_str(text_content).unwrap_or_else(|_error| Value::String(text_content.to_string()));
    }

    result
}

#[must_use]
pub fn normalize_mcp_prompt_value(prompt_value: &Value) -> String {
    if let Some(prompt) = prompt_value.as_str() {
        return prompt.to_string();
    }

    serde_json::to_string(prompt_value).unwrap_or_else(|_error| prompt_value.to_string())
}

#[must_use]
pub fn render_mcp_prompt_result(result: &Value) -> String {
    let Some(messages) = result.get("messages").and_then(Value::as_array) else {
        return normalize_mcp_prompt_value(result);
    };
    let mut rendered_messages = Vec::new();

    for message in messages {
        let role = message.get("role").and_then(Value::as_str).unwrap_or("message");
        let content = message.get("content").map_or_else(String::new, render_mcp_content_value);
        rendered_messages.push(format!("{role}: {content}"));
    }

    rendered_messages.join("\n")
}

#[must_use]
pub fn render_mcp_resource_result(result: &Value) -> String {
    let Some(contents) = result.get("contents").and_then(Value::as_array) else {
        return normalize_mcp_prompt_value(result);
    };
    let mut rendered_contents = Vec::new();

    for content in contents {
        rendered_contents.push(render_mcp_content_value(content));
    }

    rendered_contents.join("\n")
}

#[must_use]
pub fn render_mcp_prompt_text_result(result: &Value) -> String {
    result
        .get("messages")
        .and_then(Value::as_array)
        .map(|messages| {
            messages
                .iter()
                .filter_map(|message| message.pointer("/content/text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|content| !content.is_empty())
        .unwrap_or_else(|| result.to_string())
}

#[must_use]
pub fn render_mcp_resource_text_result(result: &Value) -> String {
    result
        .get("contents")
        .and_then(Value::as_array)
        .map(|contents| {
            contents
                .iter()
                .filter_map(|content| content.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|content| !content.is_empty())
        .unwrap_or_else(|| result.to_string())
}

fn render_mcp_content_value(content: &Value) -> String {
    if let Some(text) = content.as_str() {
        return text.to_string();
    }

    if let Some(text) = content.get("text").and_then(Value::as_str) {
        return text.to_string();
    }

    if let Some(blob) = content.get("blob").and_then(Value::as_str) {
        return blob.to_string();
    }

    normalize_mcp_prompt_value(content)
}
