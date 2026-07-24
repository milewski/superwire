use serde_json::Value;
use superwire_types::PromptValueFormat;

#[must_use]
pub fn normalize_mcp_tool_result(result: Value) -> Value {
    let Value::Object(mut result_object) = result else {
        return result;
    };

    if let Some(structured_content) = result_object.remove("structuredContent") {
        return structured_content;
    }

    let Some(content) = result_object.remove("content") else {
        return Value::Object(result_object);
    };
    let Value::Array(mut content_items) = content else {
        result_object.insert("content".to_string(), content);

        return Value::Object(result_object);
    };

    if content_items.len() == 1 {
        if let Some(Value::String(text_content)) = content_items
            .first_mut()
            .and_then(Value::as_object_mut)
            .and_then(|content_item| content_item.remove("text"))
        {
            return serde_json::from_str(&text_content).unwrap_or(Value::String(text_content));
        }
    }

    Value::Array(content_items)
}

#[must_use]
pub fn normalize_mcp_prompt_value(prompt_value: &Value) -> String {
    if let Some(prompt) = prompt_value.as_str() {
        return prompt.to_string();
    }

    prompt_value.to_prompt_text()
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unwraps_only_one_text_content_item() {
        let normalized_result = normalize_mcp_tool_result(json!({
            "content": [{ "type": "text", "text": "{\"value\":7}" }]
        }));

        assert_eq!(normalized_result, json!({ "value": 7 }));
    }

    #[test]
    fn preserves_every_unstructured_content_item() {
        let content = json!([
            { "type": "text", "text": "first" },
            { "type": "image", "data": "aW1hZ2U=", "mimeType": "image/png" }
        ]);
        let normalized_result = normalize_mcp_tool_result(json!({
            "content": content,
            "isError": false
        }));

        assert_eq!(normalized_result, content);
    }

    #[test]
    fn preserves_single_non_text_content_as_content_array() {
        let content = json!([{ "type": "resource_link", "uri": "file:///report.json" }]);
        let normalized_result = normalize_mcp_tool_result(json!({ "content": content }));

        assert_eq!(normalized_result, content);
    }
}
