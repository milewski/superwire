//! OpenAI-compatible provider (works with OpenAI, Azure, Ollama, etc.)

use crate::*;
use cersei_types::*;
use futures::StreamExt;
use std::future::Future;
use tokio::sync::mpsc;

pub(crate) const PROVIDER_STREAM_CHANNEL_CAPACITY: usize = 64;
pub(crate) const MAX_PROVIDER_SSE_LINE_BYTES: usize = 256 * 1024;
pub(crate) const MAX_PROVIDER_PARTIAL_LINE_BYTES: usize = MAX_PROVIDER_SSE_LINE_BYTES;
pub(crate) const MAX_PROVIDER_ERROR_BODY_BYTES: usize = 64 * 1024;

pub(crate) trait StreamEventSenderExt {
    fn send_if_open(&self, event: StreamEvent) -> impl Future<Output = bool> + Send;
}

impl StreamEventSenderExt for mpsc::Sender<StreamEvent> {
    fn send_if_open(&self, event: StreamEvent) -> impl Future<Output = bool> + Send {
        let event_sender = self.clone();

        async move { event_sender.send(event).await.is_ok() }
    }
}

pub(crate) trait ProviderResponseExt {
    fn bounded_error_message(self, event_sender: &mpsc::Sender<StreamEvent>) -> impl Future<Output = Option<String>> + Send;
}

impl ProviderResponseExt for reqwest::Response {
    fn bounded_error_message(self, event_sender: &mpsc::Sender<StreamEvent>) -> impl Future<Output = Option<String>> + Send {
        let event_sender = event_sender.clone();

        async move {
            let status = self.status().as_u16();
            let mut response_stream = self.bytes_stream();
            let mut response_body = Vec::new();

            loop {
                let chunk_result = tokio::select! {
                    () = event_sender.closed() => return None,
                    chunk_result = response_stream.next() => chunk_result,
                };
                let Some(chunk_result) = chunk_result else {
                    break;
                };
                let response_chunk = match chunk_result {
                    Ok(response_chunk) => response_chunk,
                    Err(error) => return Some(format!("HTTP {status}: failed to read provider error response: {error}")),
                };
                let Some(resulting_length) = response_body.len().checked_add(response_chunk.len()) else {
                    return Some(format!("HTTP {status}: provider error response body exceeded the configured limit"));
                };

                if resulting_length > MAX_PROVIDER_ERROR_BODY_BYTES {
                    return Some(format!("HTTP {status}: provider error response body exceeded the configured limit"));
                }

                response_body.extend_from_slice(&response_chunk);
            }

            let response_text = String::from_utf8_lossy(&response_body);
            let response_text = response_text.trim();

            if response_text.is_empty() {
                Some(format!("HTTP {status}"))
            } else {
                Some(format!("HTTP {status}: {response_text}"))
            }
        }
    }
}

macro_rules! send_stream_event {
    ($event_sender:expr, $event:expr) => {
        if !$event_sender.send_if_open($event).await {
            return;
        }
    };
}

pub(crate) use send_stream_event;

pub(crate) fn owned_completion_stream<StreamReader, StreamReaderFuture>(stream_reader: StreamReader) -> CompletionStream
where
    StreamReader: FnOnce(mpsc::Sender<StreamEvent>) -> StreamReaderFuture + Send + 'static,
    StreamReaderFuture: Future<Output = ()> + Send + 'static,
{
    let (event_sender, event_receiver) = mpsc::channel(PROVIDER_STREAM_CHANNEL_CAPACITY);
    let cancellation_sender = event_sender.clone();
    let mut stream_task = tokio::spawn(stream_reader(event_sender));

    tokio::spawn(async move {
        tokio::select! {
            () = cancellation_sender.closed() => {
                stream_task.abort();
                let _ = stream_task.await;
            }
            stream_result = &mut stream_task => {
                if let Err(error) = stream_result {
                    let _ = cancellation_sender
                        .send(StreamEvent::Error {
                            message: format!("provider stream task failed: {error}"),
                        })
                        .await;
                }
            }
        }
    });

    CompletionStream::new(event_receiver)
}

const OPENAI_API_BASE: &str = "https://api.openai.com/v1";

pub struct OpenAi {
    auth: Auth,
    base_url: String,
    default_model: String,
    client: reqwest::Client,
}

impl OpenAi {
    pub fn new(auth: Auth) -> Self {
        let base_url = std::env::var("OPENAI_BASE_URL")
            .ok()
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| OPENAI_API_BASE.to_string());
        Self {
            auth,
            base_url,
            default_model: "gpt-4o".to_string(),
            client: reqwest::Client::new(),
        }
    }

    pub fn from_env() -> Result<Self> {
        let key = std::env::var("OPENAI_API_KEY").map_err(|_| CerseiError::Auth("OPENAI_API_KEY not set".into()))?;
        Ok(Self::new(Auth::ApiKey(key)))
    }

    pub fn builder() -> OpenAiBuilder {
        OpenAiBuilder::default()
    }

    fn append_user_message(api_messages: &mut Vec<serde_json::Value>, message: &Message) {
        match &message.content {
            MessageContent::Blocks(blocks) => {
                for block in blocks {
                    if let Some(tool_message) = block.openai_tool_message() {
                        api_messages.push(tool_message);
                    }
                }

                if let Some(user_content) = blocks.openai_user_content() {
                    api_messages.push(serde_json::json!({
                        "role": "user",
                        "content": user_content,
                    }));
                }
            }
            MessageContent::Text(_) => {
                api_messages.push(serde_json::json!({
                    "role": "user",
                    "content": message.get_all_text(),
                }));
            }
        }
    }
}

enum OpenAiContentType {
    Text,
    ImageUrl,
    VideoUrl,
    File,
}

impl OpenAiContentType {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::ImageUrl => "image_url",
            Self::VideoUrl => "video_url",
            Self::File => "file",
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

trait OpenAiContentBlocksExt {
    fn openai_user_content(&self) -> Option<serde_json::Value>;
}

impl OpenAiContentBlocksExt for [ContentBlock] {
    fn openai_user_content(&self) -> Option<serde_json::Value> {
        let text_content = self
            .iter()
            .filter_map(|content_block| match content_block {
                ContentBlock::Text { text } if !text.is_empty() => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let has_asset_content = self
            .iter()
            .any(|content_block| matches!(content_block, ContentBlock::Image { .. } | ContentBlock::Document { .. }));

        if !has_asset_content && !text_content.is_empty() {
            return Some(serde_json::Value::String(text_content.join("\n")));
        }

        let content_parts = self
            .iter()
            .filter_map(ContentBlockOpenAiExt::openai_user_content_part)
            .collect::<Vec<_>>();

        if content_parts.is_empty() {
            return None;
        }

        if content_parts.len() == 1
            && content_parts
                .first()
                .and_then(|content_part| content_part.get(OpenAiContentType::Text.as_str()))
                .is_some()
        {
            return content_parts
                .first()
                .and_then(|content_part| content_part.get(OpenAiContentType::Text.as_str()))
                .cloned();
        }

        Some(serde_json::Value::Array(content_parts))
    }
}

trait ContentBlockOpenAiExt {
    fn openai_tool_message(&self) -> Option<serde_json::Value>;

    fn openai_user_content_part(&self) -> Option<serde_json::Value>;
}

impl ContentBlockOpenAiExt for ContentBlock {
    fn openai_tool_message(&self) -> Option<serde_json::Value> {
        let Self::ToolResult { tool_use_id, content, .. } = self else {
            return None;
        };

        Some(serde_json::json!({
            "role": "tool",
            "tool_call_id": tool_use_id,
            "content": content.openai_text(),
        }))
    }

    fn openai_user_content_part(&self) -> Option<serde_json::Value> {
        match self {
            Self::Text { text } if !text.is_empty() => Some(serde_json::json!({
                "type": OpenAiContentType::Text.as_str(),
                "text": text,
            })),
            Self::Image { source } => source.openai_image_content_part(),
            Self::Document { source, title, .. } => source.openai_document_content_part(title.as_deref()),
            _ => None,
        }
    }
}

trait ToolResultContentOpenAiExt {
    fn openai_text(&self) -> String;
}

impl ToolResultContentOpenAiExt for ToolResultContent {
    fn openai_text(&self) -> String {
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

trait ImageSourceOpenAiExt {
    fn openai_image_content_part(&self) -> Option<serde_json::Value>;
}

impl ImageSourceOpenAiExt for ImageSource {
    fn openai_image_content_part(&self) -> Option<serde_json::Value> {
        let url = self.openai_url_value()?;

        Some(serde_json::json!({
            "type": OpenAiContentType::ImageUrl.as_str(),
            "image_url": {
                "url": url,
            },
        }))
    }
}

trait DocumentSourceOpenAiExt {
    fn openai_document_content_part(&self, title: Option<&str>) -> Option<serde_json::Value>;
}

impl DocumentSourceOpenAiExt for DocumentSource {
    fn openai_document_content_part(&self, title: Option<&str>) -> Option<serde_json::Value> {
        if self.is_openai_video_source() {
            let url = self.openai_url_value()?;

            return Some(serde_json::json!({
                "type": OpenAiContentType::VideoUrl.as_str(),
                "video_url": {
                    "url": url,
                },
            }));
        }

        let file_data = self.openai_url_value()?;
        let filename = title.unwrap_or("document");

        Some(serde_json::json!({
            "type": OpenAiContentType::File.as_str(),
            "file": {
                "file_data": file_data,
                "filename": filename,
            },
        }))
    }
}

trait OpenAiSourceExt {
    fn openai_url_value(&self) -> Option<String>;

    fn source_type(&self) -> Option<CerseiSourceType>;

    fn media_type(&self) -> Option<&str>;

    fn data(&self) -> Option<&str>;

    fn url(&self) -> Option<&str>;
}

impl OpenAiSourceExt for ImageSource {
    fn openai_url_value(&self) -> Option<String> {
        match self.source_type()? {
            CerseiSourceType::Base64 => Some(format!(
                "data:{};base64,{}",
                self.media_type().unwrap_or("application/octet-stream"),
                self.data()?
            )),
            CerseiSourceType::Url => Some(self.url()?.to_string()),
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

impl OpenAiSourceExt for DocumentSource {
    fn openai_url_value(&self) -> Option<String> {
        match self.source_type()? {
            CerseiSourceType::Base64 => Some(format!(
                "data:{};base64,{}",
                self.media_type().unwrap_or("application/octet-stream"),
                self.data()?
            )),
            CerseiSourceType::Url => Some(self.url()?.to_string()),
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

trait DocumentSourceVideoExt {
    fn is_openai_video_source(&self) -> bool;
}

impl DocumentSourceVideoExt for DocumentSource {
    fn is_openai_video_source(&self) -> bool {
        if self
            .media_type
            .as_deref()
            .is_some_and(|media_type| media_type.starts_with("video/"))
        {
            return true;
        }

        self.url.as_deref().is_some_and(|url| {
            let normalized_url = url.split('?').next().unwrap_or(url).to_ascii_lowercase();

            ["mp4", "mpeg", "mov", "webm", "avi", "mkv"].iter().any(|extension| {
                normalized_url
                    .rsplit_once('.')
                    .is_some_and(|(_, source_extension)| source_extension == *extension)
            })
        })
    }
}

#[async_trait::async_trait]
impl Provider for OpenAi {
    fn name(&self) -> &str {
        "openai"
    }

    fn context_window(&self, model: &str) -> u64 {
        match model {
            m if m.contains("gpt-5") => 1_000_000,
            m if m.starts_with("o1") || m.starts_with("o3") => 200_000,
            m if m.contains("gpt-4o") => 128_000,
            m if m.contains("gpt-4-turbo") => 128_000,
            m if m.contains("gpt-4") => 8_192,
            m if m.contains("gpt-3.5") => 16_385,
            _ => 128_000,
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

        // Build OpenAI-format messages
        let mut api_messages: Vec<serde_json::Value> = Vec::new();

        if let Some(system) = &request.system {
            api_messages.push(serde_json::json!({
                "role": "system",
                "content": system,
            }));
        }

        for msg in &request.messages {
            match msg.role {
                Role::User => {
                    Self::append_user_message(&mut api_messages, msg);
                }
                Role::Assistant => {
                    // Check for tool_use blocks — serialize as tool_calls
                    if let MessageContent::Blocks(blocks) = &msg.content {
                        let tool_uses: Vec<&ContentBlock> = blocks.iter().filter(|b| matches!(b, ContentBlock::ToolUse { .. })).collect();
                        if !tool_uses.is_empty() {
                            let tool_calls: Vec<serde_json::Value> = tool_uses
                                .iter()
                                .map(|b| {
                                    if let ContentBlock::ToolUse { id, name, input } = b {
                                        serde_json::json!({
                                            "id": id,
                                            "type": "function",
                                            "function": {
                                                "name": name,
                                                "arguments": input.to_string(),
                                            }
                                        })
                                    } else {
                                        serde_json::json!({})
                                    }
                                })
                                .collect();

                            let text_content: String = blocks
                                .iter()
                                .filter_map(|b| {
                                    if let ContentBlock::Text { text } = b {
                                        Some(text.as_str())
                                    } else {
                                        None
                                    }
                                })
                                .collect::<Vec<_>>()
                                .join("");

                            let mut asst_msg = serde_json::json!({
                                "role": "assistant",
                                "tool_calls": tool_calls,
                            });
                            if !text_content.is_empty() {
                                asst_msg["content"] = serde_json::json!(text_content);
                            }
                            api_messages.push(asst_msg);
                        } else {
                            api_messages.push(serde_json::json!({
                                "role": "assistant",
                                "content": msg.get_all_text(),
                            }));
                        }
                    } else {
                        api_messages.push(serde_json::json!({
                            "role": "assistant",
                            "content": msg.get_all_text(),
                        }));
                    }
                }
                Role::System => {
                    api_messages.push(serde_json::json!({
                        "role": "system",
                        "content": msg.get_all_text(),
                    }));
                }
            }
        }

        // GPT-5+ and o-series use max_completion_tokens; older models use max_tokens
        let use_new_param = model.starts_with("gpt-5") || model.starts_with("o1") || model.starts_with("o3");

        let mut body = if use_new_param {
            serde_json::json!({
                "model": model,
                "messages": api_messages,
                "max_completion_tokens": request.max_tokens,
                "stream": true,
                "stream_options": { "include_usage": true },
            })
        } else {
            serde_json::json!({
                "model": model,
                "messages": api_messages,
                "max_tokens": request.max_tokens,
                "stream": true,
                "stream_options": { "include_usage": true },
            })
        };

        if let Some(temp) = request.temperature {
            body["temperature"] = serde_json::json!(temp);
        }

        if !request.tools.is_empty() {
            let tools: Vec<serde_json::Value> = request
                .tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.input_schema,
                        }
                    })
                })
                .collect();
            body["tools"] = serde_json::Value::Array(tools);
        }

        let url = format!("{}/chat/completions", self.base_url);
        let auth_header = match &self.auth {
            Auth::ApiKey(key) | Auth::Bearer(key) => format!("Bearer {}", key),
            Auth::OAuth { token, .. } => format!("Bearer {}", token.access_token),
            Auth::Custom(_) => String::new(),
        };

        let req = self
            .client
            .post(&url)
            .header("authorization", &auth_header)
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
                    let mut text_started = false;
                    // Track tool calls being assembled across chunks.
                    // OpenAI sends the id and name first, then argument fragments.
                    let mut tool_calls: std::collections::HashMap<usize, (String, String, String)> = std::collections::HashMap::new();
                    let mut has_tool_calls = false;

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

                                    if data == "[DONE]" {
                                        for (tool_index, (tool_id, tool_name, tool_arguments)) in &tool_calls {
                                            send_stream_event!(
                                                tx,
                                                StreamEvent::ContentBlockStart {
                                                    index: *tool_index + 1,
                                                    block_type: "tool_use".into(),
                                                    id: Some(tool_id.clone()),
                                                    name: Some(tool_name.clone()),
                                                }
                                            );
                                            send_stream_event!(
                                                tx,
                                                StreamEvent::InputJsonDelta {
                                                    index: *tool_index + 1,
                                                    partial_json: tool_arguments.clone(),
                                                }
                                            );
                                            send_stream_event!(tx, StreamEvent::ContentBlockStop { index: *tool_index + 1 });
                                        }

                                        if text_started {
                                            send_stream_event!(tx, StreamEvent::ContentBlockStop { index: 0 });
                                        }

                                        let stop_reason = if has_tool_calls { StopReason::ToolUse } else { StopReason::EndTurn };

                                        send_stream_event!(
                                            tx,
                                            StreamEvent::MessageDelta {
                                                stop_reason: Some(stop_reason),
                                                usage: None,
                                            }
                                        );
                                        send_stream_event!(tx, StreamEvent::MessageStop);

                                        return;
                                    }

                                    let Ok(json) = serde_json::from_str::<serde_json::Value>(data) else {
                                        continue;
                                    };
                                    let delta = &json["choices"][0]["delta"];
                                    let finish_reason = json["choices"][0]["finish_reason"].as_str();

                                    if let Some(text) = delta["content"].as_str() {
                                        if !text_started {
                                            text_started = true;
                                            send_stream_event!(
                                                tx,
                                                StreamEvent::ContentBlockStart {
                                                    index: 0,
                                                    block_type: "text".into(),
                                                    id: None,
                                                    name: None,
                                                }
                                            );
                                        }

                                        send_stream_event!(
                                            tx,
                                            StreamEvent::TextDelta {
                                                index: 0,
                                                text: text.to_string(),
                                            }
                                        );
                                    }

                                    if let Some(tool_call_values) = delta["tool_calls"].as_array() {
                                        has_tool_calls = true;

                                        for tool_call_value in tool_call_values {
                                            let tool_index = tool_call_value["index"].as_u64().unwrap_or(0) as usize;

                                            if tool_index >= crate::MAX_PROVIDER_CONTENT_BLOCKS {
                                                send_stream_event!(
                                                    tx,
                                                    StreamEvent::Error {
                                                        message: "provider content block index exceeded the configured limit".to_string(),
                                                    }
                                                );

                                                return;
                                            }

                                            let tool_call = tool_calls
                                                .entry(tool_index)
                                                .or_insert_with(|| (String::new(), String::new(), String::new()));

                                            if let Some(tool_id) = tool_call_value["id"].as_str() {
                                                tool_call.0 = tool_id.to_string();
                                            }

                                            if let Some(tool_name) = tool_call_value["function"]["name"].as_str() {
                                                tool_call.1 = tool_name.to_string();
                                            }

                                            if let Some(arguments) = tool_call_value["function"]["arguments"].as_str() {
                                                let Some(argument_length) = tool_call.2.len().checked_add(arguments.len()) else {
                                                    send_stream_event!(
                                                        tx,
                                                        StreamEvent::Error {
                                                            message: "provider tool arguments exceeded the configured limit".to_string(),
                                                        }
                                                    );

                                                    return;
                                                };

                                                if argument_length > crate::MAX_PROVIDER_TOOL_ARGUMENT_BYTES {
                                                    send_stream_event!(
                                                        tx,
                                                        StreamEvent::Error {
                                                            message: "provider tool arguments exceeded the configured limit".to_string(),
                                                        }
                                                    );

                                                    return;
                                                }

                                                tool_call.2.push_str(arguments);
                                            }
                                        }
                                    }

                                    if let Some(usage) = json["usage"].as_object() {
                                        let input_tokens = usage.get("prompt_tokens").and_then(serde_json::Value::as_u64).unwrap_or(0);
                                        let output_tokens = usage.get("completion_tokens").and_then(serde_json::Value::as_u64).unwrap_or(0);

                                        send_stream_event!(
                                            tx,
                                            StreamEvent::MessageDelta {
                                                stop_reason: finish_reason.and_then(|reason| {
                                                    match reason {
                                                        "stop" => Some(StopReason::EndTurn),
                                                        "tool_calls" => Some(StopReason::ToolUse),
                                                        "length" => Some(StopReason::MaxTokens),
                                                        _ => None,
                                                    }
                                                }),
                                                usage: Some(Usage {
                                                    input_tokens,
                                                    output_tokens,
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

                    let message = if String::from_utf8_lossy(&buffer).trim().is_empty() {
                        "provider SSE stream ended before the completion delimiter"
                    } else {
                        "provider SSE stream ended with an incomplete line"
                    };

                    send_stream_event!(
                        tx,
                        StreamEvent::Error {
                            message: message.to_string(),
                        }
                    );
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
pub struct OpenAiBuilder {
    api_key: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
    client: Option<reqwest::Client>,
}

impl OpenAiBuilder {
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

    pub fn build(self) -> Result<OpenAi> {
        let auth = if let Some(key) = self.api_key {
            Auth::ApiKey(key)
        } else {
            return Err(CerseiError::Auth(
                "No API key provided. Set OPENAI_API_KEY or use .api_key()".into(),
            ));
        };

        Ok(OpenAi {
            auth,
            base_url: self.base_url.unwrap_or_else(|| OPENAI_API_BASE.to_string()),
            default_model: self.model.unwrap_or_else(|| "gpt-4o".to_string()),
            client: self.client.unwrap_or_default(),
        })
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::io;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};
    use tokio::sync::oneshot;

    pub(crate) struct MockProviderServer {
        endpoint: String,
        disconnect_receiver: Option<oneshot::Receiver<()>>,
        server_task: tokio::task::JoinHandle<()>,
    }

    impl MockProviderServer {
        pub(crate) async fn fixed(status: u16, body: Vec<u8>) -> Self {
            Self::spawn(status, MockResponseBody::Fixed(body)).await
        }

        pub(crate) async fn endless(status: u16, repeated_chunk: Vec<u8>) -> Self {
            Self::spawn(status, MockResponseBody::Endless(repeated_chunk)).await
        }

        async fn spawn(status: u16, response_body: MockResponseBody) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").await.expect("mock provider listener should bind");
            let local_address = listener.local_addr().expect("mock provider address should resolve");
            let (disconnect_sender, disconnect_receiver) = oneshot::channel();
            let server_task = tokio::spawn(async move {
                let (mut socket, _peer_address) = listener.accept().await.expect("mock provider should accept request");

                read_request(&mut socket).await.expect("mock provider request should be readable");

                match response_body {
                    MockResponseBody::Fixed(body) => {
                        let response_header = format!(
                            "HTTP/1.1 {status} {}\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                            status_reason(status),
                            body.len()
                        );

                        if socket.write_all(response_header.as_bytes()).await.is_ok() {
                            let _ = socket.write_all(&body).await;
                        }
                    }
                    MockResponseBody::Endless(repeated_chunk) => {
                        let response_header = format!(
                            "HTTP/1.1 {status} {}\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n",
                            status_reason(status)
                        );

                        if socket.write_all(response_header.as_bytes()).await.is_err() {
                            let _ = disconnect_sender.send(());

                            return;
                        }

                        loop {
                            let chunk_header = format!("{:X}\r\n", repeated_chunk.len());
                            let write_result = async {
                                socket.write_all(chunk_header.as_bytes()).await?;
                                socket.write_all(&repeated_chunk).await?;
                                socket.write_all(b"\r\n").await?;
                                socket.flush().await
                            }
                            .await;

                            if write_result.is_err() {
                                let _ = disconnect_sender.send(());

                                return;
                            }

                            tokio::time::sleep(Duration::from_millis(5)).await;
                        }
                    }
                }
            });

            Self {
                endpoint: format!("http://{local_address}"),
                disconnect_receiver: Some(disconnect_receiver),
                server_task,
            }
        }

        pub(crate) fn endpoint(&self) -> &str {
            &self.endpoint
        }

        pub(crate) async fn wait_for_disconnect(&mut self) {
            let disconnect_receiver = self
                .disconnect_receiver
                .take()
                .expect("disconnect receiver should only be awaited once");

            tokio::time::timeout(Duration::from_secs(2), disconnect_receiver)
                .await
                .expect("provider connection should terminate after receiver drop")
                .expect("mock provider should observe connection termination");
        }
    }

    impl Drop for MockProviderServer {
        fn drop(&mut self) {
            self.server_task.abort();
        }
    }

    enum MockResponseBody {
        Fixed(Vec<u8>),
        Endless(Vec<u8>),
    }

    async fn read_request(socket: &mut TcpStream) -> io::Result<()> {
        let mut request_bytes = Vec::new();
        let mut read_buffer = [0_u8; 4096];

        loop {
            let bytes_read = socket.read(&mut read_buffer).await?;

            if bytes_read == 0 {
                return Ok(());
            }

            request_bytes.extend_from_slice(&read_buffer[..bytes_read]);

            let Some(header_end) = request_bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
                continue;
            };
            let body_start = header_end + 4;
            let request_headers = String::from_utf8_lossy(&request_bytes[..header_end]);
            let content_length = request_headers
                .lines()
                .find_map(|line| {
                    let (header_name, header_value) = line.split_once(':')?;

                    header_name
                        .eq_ignore_ascii_case("content-length")
                        .then(|| header_value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);

            if request_bytes.len() >= body_start.saturating_add(content_length) {
                return Ok(());
            }
        }
    }

    fn status_reason(status: u16) -> &'static str {
        match status {
            200 => "OK",
            400 => "Bad Request",
            429 => "Too Many Requests",
            500 => "Internal Server Error",
            _ => "Response",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::MockProviderServer;
    use super::*;
    use std::time::Duration;

    fn provider(base_url: &str) -> OpenAi {
        OpenAi::builder()
            .api_key("test-api-key")
            .base_url(base_url)
            .client(reqwest::Client::new())
            .build()
            .expect("OpenAI provider should build")
    }

    #[tokio::test]
    async fn stream_recovers_after_malformed_event_and_preserves_completion_semantics() {
        let response_body = concat!(
            "data: not-json\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"hello\"},\"finish_reason\":null}]}\n",
            "data: [DONE]\n"
        );
        let mock_server = MockProviderServer::fixed(200, response_body.as_bytes().to_vec()).await;
        let response = provider(mock_server.endpoint())
            .complete_blocking(CompletionRequest::new("test-model"))
            .await
            .expect("valid OpenAI stream should complete");

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
            .expect_err("unterminated OpenAI stream should fail");

        assert!(error.to_string().contains("partial line exceeded"));
    }

    #[tokio::test]
    async fn stream_rejects_accumulated_tool_arguments_over_limit() {
        const ARGUMENT_FRAGMENT_BYTES: usize = 200 * 1024;

        let argument_fragment = "a".repeat(ARGUMENT_FRAGMENT_BYTES);
        let fragment_count = crate::MAX_PROVIDER_TOOL_ARGUMENT_BYTES / ARGUMENT_FRAGMENT_BYTES + 1;
        let mut response_body = String::new();

        for _fragment_index in 0..fragment_count {
            let event = serde_json::json!({
                "choices": [{
                    "delta": {
                        "tool_calls": [{
                            "index": 0,
                            "function": {
                                "arguments": argument_fragment,
                            }
                        }]
                    },
                    "finish_reason": serde_json::Value::Null,
                }]
            });

            response_body.push_str("data: ");
            response_body.push_str(&event.to_string());
            response_body.push('\n');
        }

        let mock_server = MockProviderServer::fixed(200, response_body.into_bytes()).await;
        let error = provider(mock_server.endpoint())
            .complete_blocking(CompletionRequest::new("test-model"))
            .await
            .expect_err("oversized OpenAI tool arguments should fail");

        assert!(error.to_string().contains("tool arguments exceeded"));
    }

    #[tokio::test]
    async fn error_response_body_is_streamed_and_bounded() {
        let response_body = vec![b'e'; MAX_PROVIDER_ERROR_BODY_BYTES + 1];
        let mock_server = MockProviderServer::fixed(500, response_body).await;
        let error = provider(mock_server.endpoint())
            .complete_blocking(CompletionRequest::new("test-model"))
            .await
            .expect_err("oversized OpenAI error response should fail");

        assert!(error.to_string().contains("error response body exceeded"));
    }

    #[tokio::test]
    async fn dropping_receiver_aborts_stream_task_and_connection() {
        let mut mock_server = MockProviderServer::endless(200, b"x".to_vec()).await;
        let completion_stream = provider(mock_server.endpoint())
            .complete(CompletionRequest::new("test-model"))
            .await
            .expect("OpenAI stream should start");
        let mut event_receiver = completion_stream.into_receiver();

        tokio::time::timeout(Duration::from_secs(1), event_receiver.recv())
            .await
            .expect("OpenAI message start should arrive")
            .expect("OpenAI stream should remain open");
        drop(event_receiver);

        mock_server.wait_for_disconnect().await;
    }
}
