//! Google Gemini provider: native Gemini API client with streaming support.
//!
//! Uses Google's `generateContent` API directly rather than the OpenAI-compatible
//! shim, enabling access to native Gemini features like safety settings,
//! grounding, and proper multimodal support.

use crate::openai::{
    owned_completion_stream, send_stream_event, ProviderResponseExt, StreamEventSenderExt, MAX_PROVIDER_PARTIAL_LINE_BYTES,
    MAX_PROVIDER_SSE_LINE_BYTES,
};
use crate::*;
use cersei_types::*;
use futures::StreamExt;

const GEMINI_API_BASE: &str = "https://generativelanguage.googleapis.com/v1beta";

// ─── Gemini provider ────────────────────────────────────────────────────────

pub struct Gemini {
    api_key: String,
    base_url: String,
    default_model: String,
    client: reqwest::Client,
}

impl Gemini {
    pub fn new(api_key: impl Into<String>) -> Self {
        let base_url = std::env::var("GEMINI_BASE_URL")
            .ok()
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| GEMINI_API_BASE.to_string());
        Self {
            api_key: api_key.into(),
            base_url,
            default_model: "gemini-3.1-pro-preview".to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// Create from `GOOGLE_API_KEY` or `GEMINI_API_KEY` environment variable.
    pub fn from_env() -> Result<Self> {
        let key = std::env::var("GOOGLE_API_KEY")
            .or_else(|_| std::env::var("GEMINI_API_KEY"))
            .map_err(|_| CerseiError::Auth("GOOGLE_API_KEY or GEMINI_API_KEY not set".into()))?;
        Ok(Self::new(key))
    }

    pub fn builder() -> GeminiBuilder {
        GeminiBuilder::default()
    }

    fn user_parts_for_message(message: &Message, tool_name_map: &std::collections::HashMap<String, String>) -> Vec<serde_json::Value> {
        match &message.content {
            MessageContent::Blocks(blocks) => blocks.iter().filter_map(|block| block.gemini_user_part(tool_name_map)).collect(),
            MessageContent::Text(_) => vec![serde_json::json!({ "text": message.get_all_text() })],
        }
    }
}

enum CerseiSourceType {
    Base64,
    Url,
}

impl CerseiSourceType {
    fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "base64" => Some(Self::Base64),
            "url" => Some(Self::Url),
            _ => None,
        }
    }
}

trait ContentBlockGeminiExt {
    fn gemini_user_part(&self, tool_name_map: &std::collections::HashMap<String, String>) -> Option<serde_json::Value>;
}

impl ContentBlockGeminiExt for ContentBlock {
    fn gemini_user_part(&self, tool_name_map: &std::collections::HashMap<String, String>) -> Option<serde_json::Value> {
        match self {
            Self::Text { text } => Some(serde_json::json!({ "text": text })),
            Self::Image { source } => source.gemini_part(),
            Self::Document { source, .. } => source.gemini_part(),
            Self::ToolResult { tool_use_id, content, .. } => {
                let function_name = tool_name_map.get(tool_use_id).cloned().unwrap_or_else(|| tool_use_id.clone());

                Some(serde_json::json!({
                    "functionResponse": {
                        "name": function_name,
                        "response": { "content": content.gemini_text() },
                    }
                }))
            }
            _ => None,
        }
    }
}

trait ToolResultContentGeminiExt {
    fn gemini_text(&self) -> String;
}

impl ToolResultContentGeminiExt for ToolResultContent {
    fn gemini_text(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Blocks(blocks) => blocks
                .iter()
                .filter_map(|block| {
                    if let ContentBlock::Text { text } = block {
                        Some(text.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }
}

trait GeminiSourceExt {
    fn gemini_part(&self) -> Option<serde_json::Value>;

    fn source_type(&self) -> Option<CerseiSourceType>;

    fn media_type(&self) -> Option<&str>;

    fn data(&self) -> Option<&str>;

    fn url(&self) -> Option<&str>;
}

impl GeminiSourceExt for ImageSource {
    fn gemini_part(&self) -> Option<serde_json::Value> {
        let media_type = self.media_type().unwrap_or("application/octet-stream");

        match self.source_type()? {
            CerseiSourceType::Base64 => Some(serde_json::json!({
                "inlineData": {
                    "mimeType": media_type,
                    "data": self.data()?,
                },
            })),
            CerseiSourceType::Url => Some(serde_json::json!({
                "fileData": {
                    "mimeType": media_type,
                    "fileUri": self.url()?,
                },
            })),
        }
    }

    fn source_type(&self) -> Option<CerseiSourceType> {
        CerseiSourceType::from_identifier(&self.source_type)
    }

    fn media_type(&self) -> Option<&str> {
        self.media_type.as_deref()
    }

    fn data(&self) -> Option<&str> {
        self.data.as_deref()
    }

    fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }
}

impl GeminiSourceExt for DocumentSource {
    fn gemini_part(&self) -> Option<serde_json::Value> {
        let media_type = self.media_type().unwrap_or("application/octet-stream");

        match self.source_type()? {
            CerseiSourceType::Base64 => Some(serde_json::json!({
                "inlineData": {
                    "mimeType": media_type,
                    "data": self.data()?,
                },
            })),
            CerseiSourceType::Url => Some(serde_json::json!({
                "fileData": {
                    "mimeType": media_type,
                    "fileUri": self.url()?,
                },
            })),
        }
    }

    fn source_type(&self) -> Option<CerseiSourceType> {
        CerseiSourceType::from_identifier(&self.source_type)
    }

    fn media_type(&self) -> Option<&str> {
        self.media_type.as_deref()
    }

    fn data(&self) -> Option<&str> {
        self.data.as_deref()
    }

    fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }
}

#[async_trait::async_trait]
impl Provider for Gemini {
    fn name(&self) -> &str {
        "google"
    }

    fn context_window(&self, model: &str) -> u64 {
        match model {
            m if m.contains("gemini-3.1") => 2_000_000,
            m if m.contains("gemini-3.0") => 1_000_000,
            m if m.contains("gemini-2.0") => 1_000_000,
            m if m.contains("gemini-1.5-pro") => 2_000_000,
            m if m.contains("gemini-1.5-flash") => 1_000_000,
            _ => 1_000_000,
        }
    }

    fn capabilities(&self, _model: &str) -> ProviderCapabilities {
        ProviderCapabilities {
            streaming: true,
            tool_use: true,
            vision: true,
            thinking: false,
            system_prompt: true,
            caching: false,
        }
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionStream> {
        let model = if request.model.is_empty() {
            self.default_model.clone()
        } else {
            request.model.clone()
        };

        // Build a map of tool_use_id → tool_name from conversation history
        let tool_name_map: std::collections::HashMap<String, String> = request
            .messages
            .iter()
            .flat_map(|m| match &m.content {
                MessageContent::Blocks(blocks) => blocks
                    .iter()
                    .filter_map(|b| {
                        if let ContentBlock::ToolUse { id, name, .. } = b {
                            Some((id.clone(), name.clone()))
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>(),
                _ => vec![],
            })
            .collect();

        // Build Gemini-native contents array
        let mut contents: Vec<serde_json::Value> = Vec::new();

        for msg in &request.messages {
            match msg.role {
                Role::User => {
                    let parts = Self::user_parts_for_message(msg, &tool_name_map);

                    if !parts.is_empty() {
                        contents.push(serde_json::json!({
                            "role": "user",
                            "parts": parts,
                        }));
                    }
                }
                Role::Assistant => {
                    let mut parts: Vec<serde_json::Value> = Vec::new();

                    if let MessageContent::Blocks(blocks) = &msg.content {
                        for block in blocks {
                            match block {
                                ContentBlock::Text { text } => {
                                    parts.push(serde_json::json!({ "text": text }));
                                }
                                ContentBlock::ToolUse { id, name, input } => {
                                    // Extract fc_id and thoughtSignature from encoded tool_id
                                    // Format: "gemini-tool-N::fc_id::thoughtSignature" or "gemini-tool-N"
                                    let segments: Vec<&str> = id.splitn(3, "::").collect();
                                    let mut fc = serde_json::json!({
                                        "name": name,
                                        "args": input,
                                    });
                                    let mut part_obj = serde_json::Map::new();
                                    if segments.len() >= 3 {
                                        // Has fc_id and thoughtSignature
                                        fc["id"] = serde_json::Value::String(segments[1].to_string());
                                        part_obj.insert("functionCall".to_string(), fc);
                                        part_obj.insert("thoughtSignature".to_string(), serde_json::Value::String(segments[2].to_string()));
                                    } else {
                                        part_obj.insert("functionCall".to_string(), fc);
                                    }
                                    parts.push(serde_json::Value::Object(part_obj));
                                }
                                _ => {}
                            }
                        }
                    } else {
                        parts.push(serde_json::json!({ "text": msg.get_all_text() }));
                    }

                    if !parts.is_empty() {
                        contents.push(serde_json::json!({
                            "role": "model",
                            "parts": parts,
                        }));
                    }
                }
                Role::System => {
                    // System messages handled separately via systemInstruction
                }
            }
        }

        // Build request body
        let mut body = serde_json::json!({
            "contents": contents,
            "generationConfig": {
                "maxOutputTokens": request.max_tokens,
            },
        });

        // System instruction (Gemini's equivalent of system prompt)
        if let Some(system) = &request.system {
            body["systemInstruction"] = serde_json::json!({
                "parts": [{ "text": system }],
            });
        }

        if let Some(temp) = request.temperature {
            body["generationConfig"]["temperature"] = serde_json::json!(temp);
        }

        if !request.stop_sequences.is_empty() {
            body["generationConfig"]["stopSequences"] = serde_json::json!(request.stop_sequences);
        }

        // Tool declarations
        if !request.tools.is_empty() {
            let function_declarations: Vec<serde_json::Value> = request
                .tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema,
                    })
                })
                .collect();
            body["tools"] = serde_json::json!([{
                "functionDeclarations": function_declarations,
            }]);
        }

        // Safety settings: use least restrictive defaults to avoid unexpected blocks
        body["safetySettings"] = serde_json::json!([
            { "category": "HARM_CATEGORY_HARASSMENT", "threshold": "BLOCK_ONLY_HIGH" },
            { "category": "HARM_CATEGORY_HATE_SPEECH", "threshold": "BLOCK_ONLY_HIGH" },
            { "category": "HARM_CATEGORY_SEXUALLY_EXPLICIT", "threshold": "BLOCK_ONLY_HIGH" },
            { "category": "HARM_CATEGORY_DANGEROUS_CONTENT", "threshold": "BLOCK_ONLY_HIGH" },
        ]);

        // SECURITY: never put the API key in the URL. Use the
        // `x-goog-api-key` header so that reqwest's error `Display` (which
        // prints the URL) cannot leak the secret into logs or error-wrapped
        // output.
        let url = format!("{}/models/{}:streamGenerateContent?alt=sse", self.base_url, model);

        let req = self
            .client
            .post(&url)
            .header("x-goog-api-key", &self.api_key)
            .header("content-type", "application/json")
            .json(&body)
            .build()
            .map_err(CerseiError::Http)?;
        let client = self.client.clone();
        let completion_stream = owned_completion_stream(move |tx| async move {
            let response_result = tokio::select! {
                () = tx.closed() => return,
                response_result = client.execute(req) => response_result,
            };

            match response_result {
                Ok(response) => {
                    if !response.status().is_success() {
                        let Some(message) = response.bounded_error_message(&tx).await else {
                            return;
                        };

                        send_stream_event!(tx, StreamEvent::Error { message });

                        return;
                    }

                    send_stream_event!(
                        tx,
                        StreamEvent::MessageStart {
                            id: String::new(),
                            model: String::new(),
                        }
                    );

                    let mut stream = response.bytes_stream();
                    let mut buffer = Vec::new();
                    let mut stream_bytes = 0_usize;
                    let mut block_index = 0_usize;
                    let mut total_input_tokens = 0_u64;
                    let mut total_output_tokens = 0_u64;
                    let mut saw_function_calls = false;
                    let mut saw_completion = false;

                    loop {
                        let chunk_result = tokio::select! {
                            () = tx.closed() => return,
                            chunk_result = stream.next() => chunk_result,
                        };
                        let Some(chunk_result) = chunk_result else {
                            break;
                        };

                        match chunk_result {
                            Ok(bytes) => {
                                let Some(resulting_stream_bytes) = stream_bytes.checked_add(bytes.len()) else {
                                    send_stream_event!(
                                        tx,
                                        StreamEvent::Error {
                                            message: "provider stream exceeded the configured limit".to_string(),
                                        }
                                    );

                                    return;
                                };
                                stream_bytes = resulting_stream_bytes;

                                if stream_bytes > crate::MAX_PROVIDER_STREAM_BYTES {
                                    send_stream_event!(
                                        tx,
                                        StreamEvent::Error {
                                            message: "provider stream exceeded the configured limit".to_string(),
                                        }
                                    );

                                    return;
                                }

                                buffer.extend_from_slice(&bytes);

                                let partial_line_bytes = buffer
                                    .iter()
                                    .rposition(|byte| *byte == b'\n')
                                    .map_or(buffer.len(), |line_end| buffer.len().saturating_sub(line_end + 1));

                                if partial_line_bytes > MAX_PROVIDER_PARTIAL_LINE_BYTES {
                                    send_stream_event!(
                                        tx,
                                        StreamEvent::Error {
                                            message: "provider SSE partial line exceeded the configured limit".to_string(),
                                        }
                                    );

                                    return;
                                }

                                while let Some(line_end) = buffer.iter().position(|byte| *byte == b'\n') {
                                    if line_end > MAX_PROVIDER_SSE_LINE_BYTES {
                                        send_stream_event!(
                                            tx,
                                            StreamEvent::Error {
                                                message: "provider SSE line exceeded the configured limit".to_string(),
                                            }
                                        );

                                        return;
                                    }

                                    let line = String::from_utf8_lossy(&buffer[..line_end]).into_owned();
                                    buffer.drain(..=line_end);
                                    let Some(data) = line.strip_prefix("data:") else {
                                        continue;
                                    };
                                    let data = data.trim();

                                    if data.is_empty() {
                                        continue;
                                    }

                                    let Ok(json) = serde_json::from_str::<serde_json::Value>(data) else {
                                        continue;
                                    };

                                    if let Some(metadata) = json.get("usageMetadata") {
                                        total_input_tokens = metadata
                                            .get("promptTokenCount")
                                            .and_then(serde_json::Value::as_u64)
                                            .unwrap_or(total_input_tokens);
                                        total_output_tokens = metadata
                                            .get("candidatesTokenCount")
                                            .and_then(serde_json::Value::as_u64)
                                            .unwrap_or(total_output_tokens);
                                    }

                                    let Some(candidates) = json.get("candidates").and_then(serde_json::Value::as_array) else {
                                        continue;
                                    };

                                    for candidate in candidates {
                                        if let Some(parts) = candidate
                                            .get("content")
                                            .and_then(|content| content.get("parts"))
                                            .and_then(serde_json::Value::as_array)
                                        {
                                            for part in parts {
                                                if let Some(text) = part.get("text").and_then(serde_json::Value::as_str) {
                                                    if block_index >= crate::MAX_PROVIDER_CONTENT_BLOCKS {
                                                        send_stream_event!(
                                                            tx,
                                                            StreamEvent::Error {
                                                                message: "provider content block index exceeded the configured limit"
                                                                    .to_string(),
                                                            }
                                                        );

                                                        return;
                                                    }

                                                    send_stream_event!(
                                                        tx,
                                                        StreamEvent::ContentBlockStart {
                                                            index: block_index,
                                                            block_type: "text".into(),
                                                            id: None,
                                                            name: None,
                                                        }
                                                    );
                                                    send_stream_event!(
                                                        tx,
                                                        StreamEvent::TextDelta {
                                                            index: block_index,
                                                            text: text.to_string(),
                                                        }
                                                    );
                                                    send_stream_event!(tx, StreamEvent::ContentBlockStop { index: block_index });
                                                    block_index += 1;
                                                }

                                                if let Some(function_call) = part.get("functionCall") {
                                                    if block_index >= crate::MAX_PROVIDER_CONTENT_BLOCKS {
                                                        send_stream_event!(
                                                            tx,
                                                            StreamEvent::Error {
                                                                message: "provider content block index exceeded the configured limit"
                                                                    .to_string(),
                                                            }
                                                        );

                                                        return;
                                                    }

                                                    saw_function_calls = true;
                                                    let function_name = function_call
                                                        .get("name")
                                                        .and_then(serde_json::Value::as_str)
                                                        .unwrap_or("")
                                                        .to_string();
                                                    let function_arguments = function_call
                                                        .get("args")
                                                        .cloned()
                                                        .unwrap_or(serde_json::Value::Object(Default::default()));
                                                    let thought_signature =
                                                        part.get("thoughtSignature").and_then(serde_json::Value::as_str).unwrap_or("");
                                                    let function_call_identifier =
                                                        function_call.get("id").and_then(serde_json::Value::as_str).unwrap_or("");
                                                    let tool_identifier = if thought_signature.is_empty() {
                                                        format!("gemini-tool-{block_index}")
                                                    } else {
                                                        format!(
                                                            "gemini-tool-{block_index}::{function_call_identifier}::{thought_signature}"
                                                        )
                                                    };
                                                    let serialized_arguments = match serde_json::to_string(&function_arguments) {
                                                        Ok(serialized_arguments) => serialized_arguments,
                                                        Err(error) => {
                                                            send_stream_event!(
                                                                tx,
                                                                StreamEvent::Error {
                                                                    message: format!(
                                                                        "failed to serialize provider tool arguments: {error}"
                                                                    ),
                                                                }
                                                            );

                                                            return;
                                                        }
                                                    };

                                                    if serialized_arguments.len() > crate::MAX_PROVIDER_TOOL_ARGUMENT_BYTES {
                                                        send_stream_event!(
                                                            tx,
                                                            StreamEvent::Error {
                                                                message: "provider tool arguments exceeded the configured limit"
                                                                    .to_string(),
                                                            }
                                                        );

                                                        return;
                                                    }

                                                    send_stream_event!(
                                                        tx,
                                                        StreamEvent::ContentBlockStart {
                                                            index: block_index,
                                                            block_type: "tool_use".into(),
                                                            id: Some(tool_identifier),
                                                            name: Some(function_name),
                                                        }
                                                    );
                                                    send_stream_event!(
                                                        tx,
                                                        StreamEvent::InputJsonDelta {
                                                            index: block_index,
                                                            partial_json: serialized_arguments,
                                                        }
                                                    );
                                                    send_stream_event!(tx, StreamEvent::ContentBlockStop { index: block_index });
                                                    block_index += 1;
                                                }
                                            }
                                        }

                                        let Some(finish_reason) = candidate.get("finishReason").and_then(serde_json::Value::as_str) else {
                                            continue;
                                        };
                                        saw_completion = true;
                                        let stop_reason = if saw_function_calls {
                                            StopReason::ToolUse
                                        } else {
                                            match finish_reason {
                                                "MAX_TOKENS" => StopReason::MaxTokens,
                                                "STOP" | "SAFETY" => StopReason::EndTurn,
                                                _ => StopReason::EndTurn,
                                            }
                                        };

                                        send_stream_event!(
                                            tx,
                                            StreamEvent::MessageDelta {
                                                stop_reason: Some(stop_reason),
                                                usage: Some(Usage {
                                                    input_tokens: total_input_tokens,
                                                    output_tokens: total_output_tokens,
                                                    ..Default::default()
                                                }),
                                            }
                                        );
                                    }
                                }
                            }
                            Err(error) => {
                                send_stream_event!(
                                    tx,
                                    StreamEvent::Error {
                                        message: error.to_string(),
                                    }
                                );

                                return;
                            }
                        }
                    }

                    if !String::from_utf8_lossy(&buffer).trim().is_empty() {
                        send_stream_event!(
                            tx,
                            StreamEvent::Error {
                                message: "provider SSE stream ended with an incomplete line".to_string(),
                            }
                        );

                        return;
                    }

                    if saw_completion {
                        send_stream_event!(tx, StreamEvent::MessageStop);
                    } else {
                        send_stream_event!(
                            tx,
                            StreamEvent::Error {
                                message: "provider stream ended before a completion event".to_string(),
                            }
                        );
                    }
                }
                Err(error) => {
                    send_stream_event!(
                        tx,
                        StreamEvent::Error {
                            message: error.to_string(),
                        }
                    );
                }
            }
        });

        Ok(completion_stream)
    }
}

// ─── Builder ─────────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct GeminiBuilder {
    api_key: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
    client: Option<reqwest::Client>,
}

impl GeminiBuilder {
    pub fn api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn client(mut self, client: reqwest::Client) -> Self {
        self.client = Some(client);
        self
    }

    pub fn build(self) -> Result<Gemini> {
        let api_key = if let Some(key) = self.api_key {
            key
        } else {
            return Err(CerseiError::Auth(
                "No API key provided. Set GOOGLE_API_KEY or GEMINI_API_KEY or use .api_key()".into(),
            ));
        };

        Ok(Gemini {
            api_key,
            base_url: self.base_url.unwrap_or_else(|| GEMINI_API_BASE.to_string()),
            default_model: self.model.unwrap_or_else(|| "gemini-3.1-pro-preview".to_string()),
            client: self.client.unwrap_or_default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openai::test_support::MockProviderServer;
    use crate::openai::{MAX_PROVIDER_ERROR_BODY_BYTES, MAX_PROVIDER_PARTIAL_LINE_BYTES};
    use std::time::Duration;

    fn provider(base_url: &str) -> Gemini {
        Gemini::builder()
            .api_key("test-api-key")
            .base_url(base_url)
            .client(reqwest::Client::new())
            .build()
            .expect("Gemini provider should build")
    }

    #[tokio::test]
    async fn stream_recovers_after_malformed_event_and_preserves_completion_semantics() {
        let response_body = concat!(
            "data: not-json\n",
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"hello\"}]},\"finishReason\":\"STOP\"}],",
            "\"usageMetadata\":{\"promptTokenCount\":1,\"candidatesTokenCount\":1}}\n"
        );
        let mock_server = MockProviderServer::fixed(200, response_body.as_bytes().to_vec()).await;
        let response = provider(mock_server.endpoint())
            .complete_blocking(CompletionRequest::new("test-model"))
            .await
            .expect("valid Gemini stream should complete");

        assert_eq!(response.message.get_all_text(), "hello");
        assert_eq!(response.stop_reason, StopReason::EndTurn);
    }

    #[tokio::test]
    async fn stream_rejects_never_delimited_partial_line() {
        let response_body = vec![b'x'; MAX_PROVIDER_PARTIAL_LINE_BYTES + 1];
        let mock_server = MockProviderServer::fixed(200, response_body).await;
        let error = provider(mock_server.endpoint())
            .complete_blocking(CompletionRequest::new("test-model"))
            .await
            .expect_err("unterminated Gemini stream should fail");

        assert!(error.to_string().contains("partial line exceeded"));
    }

    #[tokio::test]
    async fn stream_rejects_oversized_sse_line() {
        let response_body = format!("data: {}\n", "x".repeat(MAX_PROVIDER_SSE_LINE_BYTES + 1)).into_bytes();
        let mock_server = MockProviderServer::fixed(200, response_body).await;
        let error = provider(mock_server.endpoint())
            .complete_blocking(CompletionRequest::new("test-model"))
            .await
            .expect_err("oversized Gemini SSE line should fail");

        assert!(error.to_string().contains("SSE line exceeded"));
    }

    #[tokio::test]
    async fn error_response_body_is_streamed_and_bounded() {
        let response_body = vec![b'e'; MAX_PROVIDER_ERROR_BODY_BYTES + 1];
        let mock_server = MockProviderServer::fixed(500, response_body).await;
        let error = provider(mock_server.endpoint())
            .complete_blocking(CompletionRequest::new("test-model"))
            .await
            .expect_err("oversized Gemini error response should fail");

        assert!(error.to_string().contains("error response body exceeded"));
    }

    #[tokio::test]
    async fn dropping_receiver_aborts_stream_task_and_connection() {
        let mut mock_server = MockProviderServer::endless(200, b"x".to_vec()).await;
        let completion_stream = provider(mock_server.endpoint())
            .complete(CompletionRequest::new("test-model"))
            .await
            .expect("Gemini stream should start");
        let mut event_receiver = completion_stream.into_receiver();

        tokio::time::timeout(Duration::from_secs(1), event_receiver.recv())
            .await
            .expect("Gemini message start should arrive")
            .expect("Gemini stream should remain open");
        drop(event_receiver);

        mock_server.wait_for_disconnect().await;
    }
}
