//! Anthropic provider: Claude API client with streaming SSE support.

use crate::openai::{
    owned_completion_stream, send_stream_event, ProviderResponseExt, StreamEventSenderExt, MAX_PROVIDER_PARTIAL_LINE_BYTES,
    MAX_PROVIDER_SSE_LINE_BYTES,
};
use crate::*;
use cersei_types::*;
use futures::StreamExt;

const ANTHROPIC_API_BASE: &str = "https://api.anthropic.com";
const ANTHROPIC_API_VERSION: &str = "2023-06-01";
const ANTHROPIC_BETA_HEADER: &str = "interleaved-thinking-2025-04-14,token-efficient-tools-2025-02-19";

// ─── Anthropic provider ──────────────────────────────────────────────────────

#[allow(dead_code)]
pub struct Anthropic {
    auth: Auth,
    base_url: String,
    default_model: String,
    thinking_budget: Option<u32>,
    max_retries: u32,
    client: reqwest::Client,
}

impl Anthropic {
    pub fn new(auth: Auth) -> Self {
        let base_url = std::env::var("ANTHROPIC_BASE_URL")
            .ok()
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| ANTHROPIC_API_BASE.to_string());
        Self {
            auth,
            base_url,
            default_model: "claude-sonnet-4-6".to_string(),
            thinking_budget: None,
            max_retries: 5,
            client: reqwest::Client::new(),
        }
    }

    /// Create from `ANTHROPIC_API_KEY` environment variable.
    pub fn from_env() -> Result<Self> {
        let key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| CerseiError::Auth("ANTHROPIC_API_KEY not set".into()))?;
        Ok(Self::new(Auth::ApiKey(key)))
    }

    pub fn builder() -> AnthropicBuilder {
        AnthropicBuilder::default()
    }

    async fn auth_headers(&self) -> Result<Vec<(String, String)>> {
        match &self.auth {
            Auth::ApiKey(key) => Ok(vec![("x-api-key".into(), key.clone())]),
            Auth::Bearer(token) => Ok(vec![("authorization".into(), format!("Bearer {}", token))]),
            Auth::OAuth { token, .. } => Ok(vec![("authorization".into(), format!("Bearer {}", token.access_token))]),
            Auth::Custom(provider) => {
                let (name, value) = provider.get_credentials().await?;
                Ok(vec![(name, value)])
            }
        }
    }
}

#[async_trait::async_trait]
impl Provider for Anthropic {
    fn name(&self) -> &str {
        "anthropic"
    }

    fn context_window(&self, model: &str) -> u64 {
        match model {
            m if m.contains("opus") => 200_000,
            m if m.contains("sonnet") => 200_000,
            m if m.contains("haiku") => 200_000,
            _ => 200_000,
        }
    }

    fn capabilities(&self, _model: &str) -> ProviderCapabilities {
        ProviderCapabilities {
            streaming: true,
            tool_use: true,
            vision: true,
            thinking: true,
            system_prompt: true,
            caching: true,
        }
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionStream> {
        let model = if request.model.is_empty() {
            self.default_model.clone()
        } else {
            request.model.clone()
        };

        // Build API messages
        let api_messages: Vec<serde_json::Value> = request
            .messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content,
                })
            })
            .collect();

        // Build request body
        let mut body = serde_json::json!({
            "model": model,
            "max_tokens": request.max_tokens,
            "messages": api_messages,
            "stream": true,
        });

        if let Some(system) = &request.system {
            body["system"] = serde_json::Value::String(system.clone());
        }

        if !request.tools.is_empty() {
            let api_tools: Vec<serde_json::Value> = request
                .tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "name": t.name,
                        "description": t.description,
                        "input_schema": t.input_schema,
                    })
                })
                .collect();
            body["tools"] = serde_json::Value::Array(api_tools);
        }

        if let Some(temp) = request.temperature {
            body["temperature"] = serde_json::json!(temp);
        }

        if !request.stop_sequences.is_empty() {
            body["stop_sequences"] = serde_json::json!(request.stop_sequences);
        }

        // Thinking config
        let thinking_budget = request.options.get::<u32>("thinking_budget").or(self.thinking_budget);
        if let Some(budget) = thinking_budget {
            body["thinking"] = serde_json::json!({
                "type": "enabled",
                "budget_tokens": budget,
            });
        }

        // Build HTTP request
        let url = format!("{}/v1/messages", self.base_url);
        let mut req_builder = self
            .client
            .post(&url)
            .header("anthropic-version", ANTHROPIC_API_VERSION)
            .header("anthropic-beta", ANTHROPIC_BETA_HEADER)
            .header("content-type", "application/json");

        for (name, value) in self.auth_headers().await? {
            req_builder = req_builder.header(&name, &value);
        }

        let request = req_builder.json(&body).build().map_err(CerseiError::Http)?;
        let client = self.client.clone();
        let completion_stream = owned_completion_stream(move |tx| async move {
            let response_result = tokio::select! {
                () = tx.closed() => return,
                response_result = client.execute(request) => response_result,
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

                    let mut stream = response.bytes_stream();
                    let mut buffer = Vec::new();
                    let mut stream_bytes = 0_usize;
                    let mut tool_argument_bytes = std::collections::HashMap::<usize, usize>::new();
                    let mut saw_message_stop = false;

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

                                while let Some(frame_end) = buffer.windows(2).position(|window| window == b"\n\n") {
                                    if frame_end > crate::MAX_PROVIDER_SSE_FRAME_BYTES {
                                        send_stream_event!(
                                            tx,
                                            StreamEvent::Error {
                                                message: "provider SSE frame exceeded the configured limit".to_string(),
                                            }
                                        );

                                        return;
                                    }

                                    let event_string = String::from_utf8_lossy(&buffer[..frame_end]).into_owned();
                                    buffer.drain(..frame_end + 2);

                                    if event_string.lines().any(|line| line.len() > MAX_PROVIDER_SSE_LINE_BYTES) {
                                        send_stream_event!(
                                            tx,
                                            StreamEvent::Error {
                                                message: "provider SSE line exceeded the configured limit".to_string(),
                                            }
                                        );

                                        return;
                                    }

                                    let Some(event) = parse_sse_event(&event_string) else {
                                        continue;
                                    };
                                    let event_index = match &event {
                                        StreamEvent::ContentBlockStart { index, .. }
                                        | StreamEvent::TextDelta { index, .. }
                                        | StreamEvent::InputJsonDelta { index, .. }
                                        | StreamEvent::ThinkingDelta { index, .. }
                                        | StreamEvent::ContentBlockStop { index } => Some(*index),
                                        _ => None,
                                    };

                                    if event_index.is_some_and(|index| index >= crate::MAX_PROVIDER_CONTENT_BLOCKS) {
                                        send_stream_event!(
                                            tx,
                                            StreamEvent::Error {
                                                message: "provider content block index exceeded the configured limit".to_string(),
                                            }
                                        );

                                        return;
                                    }

                                    if let StreamEvent::InputJsonDelta { index, partial_json } = &event {
                                        let accumulated_bytes = tool_argument_bytes.entry(*index).or_default();
                                        let Some(resulting_bytes) = accumulated_bytes.checked_add(partial_json.len()) else {
                                            send_stream_event!(
                                                tx,
                                                StreamEvent::Error {
                                                    message: "provider tool arguments exceeded the configured limit".to_string(),
                                                }
                                            );

                                            return;
                                        };

                                        if resulting_bytes > crate::MAX_PROVIDER_TOOL_ARGUMENT_BYTES {
                                            send_stream_event!(
                                                tx,
                                                StreamEvent::Error {
                                                    message: "provider tool arguments exceeded the configured limit".to_string(),
                                                }
                                            );

                                            return;
                                        }

                                        *accumulated_bytes = resulting_bytes;
                                    }

                                    if matches!(event, StreamEvent::MessageStop) {
                                        saw_message_stop = true;
                                    }

                                    send_stream_event!(tx, event);
                                }

                                if buffer.len() > crate::MAX_PROVIDER_SSE_FRAME_BYTES {
                                    send_stream_event!(
                                        tx,
                                        StreamEvent::Error {
                                            message: "provider SSE frame exceeded the configured limit".to_string(),
                                        }
                                    );

                                    return;
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
                                message: "provider SSE stream ended with an incomplete frame".to_string(),
                            }
                        );

                        return;
                    }

                    if !saw_message_stop {
                        send_stream_event!(
                            tx,
                            StreamEvent::Error {
                                message: "provider SSE stream ended before the completion delimiter".to_string(),
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

// ─── SSE parser ──────────────────────────────────────────────────────────────

fn parse_sse_event(raw: &str) -> Option<StreamEvent> {
    let mut event_type = String::new();
    let mut data = String::new();

    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("event: ") {
            event_type = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("data: ") {
            data = rest.trim().to_string();
        }
    }

    let json: serde_json::Value = serde_json::from_str(&data).ok()?;

    match event_type.as_str() {
        "message_start" => {
            let msg = &json["message"];
            Some(StreamEvent::MessageStart {
                id: msg["id"].as_str().unwrap_or("").to_string(),
                model: msg["model"].as_str().unwrap_or("").to_string(),
            })
        }
        "content_block_start" => {
            let index = json["index"].as_u64().unwrap_or(0) as usize;
            let block_type = json["content_block"]["type"].as_str().unwrap_or("text").to_string();
            Some(StreamEvent::ContentBlockStart {
                index,
                block_type,
                id: json["content_block"]["id"].as_str().map(String::from),
                name: json["content_block"]["name"].as_str().map(String::from),
            })
        }
        "content_block_delta" => {
            let index = json["index"].as_u64().unwrap_or(0) as usize;
            let delta = &json["delta"];
            let delta_type = delta["type"].as_str().unwrap_or("");
            match delta_type {
                "text_delta" => Some(StreamEvent::TextDelta {
                    index,
                    text: delta["text"].as_str().unwrap_or("").to_string(),
                }),
                "input_json_delta" => Some(StreamEvent::InputJsonDelta {
                    index,
                    partial_json: delta["partial_json"].as_str().unwrap_or("").to_string(),
                }),
                "thinking_delta" => Some(StreamEvent::ThinkingDelta {
                    index,
                    thinking: delta["thinking"].as_str().unwrap_or("").to_string(),
                }),
                _ => None,
            }
        }
        "content_block_stop" => {
            let index = json["index"].as_u64().unwrap_or(0) as usize;
            Some(StreamEvent::ContentBlockStop { index })
        }
        "message_delta" => {
            let stop_reason = json["delta"]["stop_reason"].as_str().and_then(|s| match s {
                "end_turn" => Some(StopReason::EndTurn),
                "max_tokens" => Some(StopReason::MaxTokens),
                "tool_use" => Some(StopReason::ToolUse),
                "stop_sequence" => Some(StopReason::StopSequence),
                _ => None,
            });
            let usage = json["usage"].as_object().map(|usage| Usage {
                input_tokens: usage.get("input_tokens").and_then(|value| value.as_u64()).unwrap_or(0),
                output_tokens: usage.get("output_tokens").and_then(|value| value.as_u64()).unwrap_or(0),
                ..Default::default()
            });
            Some(StreamEvent::MessageDelta { stop_reason, usage })
        }
        "message_stop" => Some(StreamEvent::MessageStop),
        "ping" => Some(StreamEvent::Ping),
        "error" => Some(StreamEvent::Error {
            message: json["error"]["message"].as_str().unwrap_or("Unknown error").to_string(),
        }),
        _ => None,
    }
}

// ─── Builder ─────────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct AnthropicBuilder {
    api_key: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
    thinking_budget: Option<u32>,
    oauth_token: Option<OAuthToken>,
    max_retries: Option<u32>,
    client: Option<reqwest::Client>,
}

impl AnthropicBuilder {
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

    pub fn thinking(mut self, budget_tokens: u32) -> Self {
        self.thinking_budget = Some(budget_tokens);
        self
    }

    pub fn oauth(mut self, token: OAuthToken) -> Self {
        self.oauth_token = Some(token);
        self
    }

    pub fn max_retries(mut self, n: u32) -> Self {
        self.max_retries = Some(n);
        self
    }

    pub fn client(mut self, client: reqwest::Client) -> Self {
        self.client = Some(client);
        self
    }

    pub fn build(self) -> Result<Anthropic> {
        let auth = if let Some(token) = self.oauth_token {
            Auth::OAuth {
                client_id: String::new(),
                token,
            }
        } else if let Some(key) = self.api_key {
            Auth::ApiKey(key)
        } else {
            return Err(CerseiError::Auth(
                "No API key or OAuth token provided. Set ANTHROPIC_API_KEY or use .oauth()".into(),
            ));
        };

        Ok(Anthropic {
            auth,
            base_url: self.base_url.unwrap_or_else(|| ANTHROPIC_API_BASE.to_string()),
            default_model: self.model.unwrap_or_else(|| "claude-sonnet-4-6".to_string()),
            thinking_budget: self.thinking_budget,
            max_retries: self.max_retries.unwrap_or(5),
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

    fn provider(base_url: &str) -> Anthropic {
        Anthropic::builder()
            .api_key("test-api-key")
            .base_url(base_url)
            .client(reqwest::Client::new())
            .build()
            .expect("Anthropic provider should build")
    }

    #[tokio::test]
    async fn stream_recovers_after_unknown_event_and_preserves_completion_semantics() {
        let response_body = concat!(
            "event: message_start\n",
            "data: {\"message\":{\"id\":\"message-1\",\"model\":\"test-model\"}}\n\n",
            "event: ignored\n",
            "data: {}\n\n",
            "event: content_block_start\n",
            "data: {\"index\":0,\"content_block\":{\"type\":\"text\"}}\n\n",
            "event: content_block_delta\n",
            "data: {\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hello\"}}\n\n",
            "event: content_block_stop\n",
            "data: {\"index\":0}\n\n",
            "event: message_delta\n",
            "data: {\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}\n\n",
            "event: message_stop\n",
            "data: {}\n\n"
        );
        let mock_server = MockProviderServer::fixed(200, response_body.as_bytes().to_vec()).await;
        let response = provider(mock_server.endpoint())
            .complete_blocking(CompletionRequest::new("test-model"))
            .await
            .expect("valid Anthropic stream should complete");

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
            .expect_err("unterminated Anthropic stream should fail");

        assert!(error.to_string().contains("partial line exceeded"));
    }

    #[tokio::test]
    async fn stream_rejects_oversized_frame_composed_of_bounded_lines() {
        let bounded_line = format!("data: {}\n", "x".repeat(200 * 1024));
        let line_count = crate::MAX_PROVIDER_SSE_FRAME_BYTES / bounded_line.len() + 1;
        let response_body = bounded_line.repeat(line_count).into_bytes();
        let mock_server = MockProviderServer::fixed(200, response_body).await;
        let error = provider(mock_server.endpoint())
            .complete_blocking(CompletionRequest::new("test-model"))
            .await
            .expect_err("oversized Anthropic frame should fail");

        assert!(error.to_string().contains("SSE frame exceeded"));
    }

    #[tokio::test]
    async fn stream_rejects_accumulated_tool_arguments_over_limit() {
        const ARGUMENT_FRAGMENT_BYTES: usize = 200 * 1024;

        let argument_fragment = "a".repeat(ARGUMENT_FRAGMENT_BYTES);
        let fragment_count = crate::MAX_PROVIDER_TOOL_ARGUMENT_BYTES / ARGUMENT_FRAGMENT_BYTES + 1;
        let mut response_body = String::new();

        for _fragment_index in 0..fragment_count {
            let event_data = serde_json::json!({
                "index": 0,
                "delta": {
                    "type": "input_json_delta",
                    "partial_json": argument_fragment.as_str(),
                }
            });

            response_body.push_str("event: content_block_delta\n");
            response_body.push_str("data: ");
            response_body.push_str(&event_data.to_string());
            response_body.push_str("\n\n");
        }

        let mock_server = MockProviderServer::fixed(200, response_body.into_bytes()).await;
        let error = provider(mock_server.endpoint())
            .complete_blocking(CompletionRequest::new("test-model"))
            .await
            .expect_err("oversized Anthropic tool arguments should fail");

        assert!(error.to_string().contains("tool arguments exceeded"));
    }

    #[tokio::test]
    async fn error_response_body_is_streamed_and_bounded() {
        let response_body = vec![b'e'; MAX_PROVIDER_ERROR_BODY_BYTES + 1];
        let mock_server = MockProviderServer::fixed(500, response_body).await;
        let error = provider(mock_server.endpoint())
            .complete_blocking(CompletionRequest::new("test-model"))
            .await
            .expect_err("oversized Anthropic error response should fail");

        assert!(error.to_string().contains("error response body exceeded"));
    }

    #[tokio::test]
    async fn dropping_receiver_aborts_stream_task_and_connection() {
        let mut mock_server = MockProviderServer::endless(200, b"event: ping\ndata: {}\n\n".to_vec()).await;
        let completion_stream = provider(mock_server.endpoint())
            .complete(CompletionRequest::new("test-model"))
            .await
            .expect("Anthropic stream should start");
        let mut event_receiver = completion_stream.into_receiver();

        tokio::time::timeout(Duration::from_secs(1), event_receiver.recv())
            .await
            .expect("Anthropic response should start")
            .expect("Anthropic stream should emit an event only after response data");
        drop(event_receiver);

        mock_server.wait_for_disconnect().await;
    }
}
