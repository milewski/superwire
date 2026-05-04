use crate::model::types::ModelRequest;
use async_openai::types::{ChatCompletionTool, ChatCompletionToolChoiceOption, ResponseFormat};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize)]
pub(super) struct OpenAiChatCompletionRequest {
    pub(super) model: String,
    pub(super) messages: Vec<Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) tools: Vec<ChatCompletionTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tool_choice: Option<ChatCompletionToolChoiceOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) response_format: Option<ResponseFormat>,
}

#[derive(Debug, Clone)]
pub(super) struct OpenAiChatCompletionClient {
    client: reqwest::Client,
}

impl OpenAiChatCompletionClient {
    pub(super) fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    pub(super) async fn send(
        &self,
        request: &ModelRequest,
        completion_request: OpenAiChatCompletionRequest,
    ) -> Result<super::response::OpenAiChatCompletionResponse, String> {
        let endpoint = request.chat_completions_endpoint();
        log::debug!(
            "sending HTTP request to AI provider: agent={}, endpoint={}, model={}, messages={}, tools={}",
            request.agent_name,
            endpoint,
            completion_request.model,
            completion_request.messages.len(),
            completion_request.tools.len()
        );
        let response = self
            .client
            .post(endpoint)
            .bearer_auth(&request.provider_config.api_key)
            .json(&completion_request)
            .send()
            .await
            .map_err(|error| error.to_string())?;

        log::debug!("AI provider responded: agent={}, status={}", request.agent_name, response.status());

        OpenAiChatCompletionResponseText::from_response(response).await?.deserialize()
    }
}

impl ModelRequest {
    fn chat_completions_endpoint(&self) -> String {
        format!("{}/chat/completions", self.provider_config.endpoint.trim_end_matches('/'))
    }
}

struct OpenAiChatCompletionResponseText {
    status: reqwest::StatusCode,
    text: String,
}

impl OpenAiChatCompletionResponseText {
    async fn from_response(response: reqwest::Response) -> Result<Self, String> {
        let status = response.status();
        let text = response.text().await.map_err(|error| error.to_string())?;

        Ok(Self { status, text })
    }

    fn deserialize(self) -> Result<super::response::OpenAiChatCompletionResponse, String> {
        if !self.status.is_success() {
            return Err(OpenAiErrorResponse::message_from_response_text(&self.text)
                .unwrap_or_else(|| format!("provider returned HTTP {}: {}", self.status, self.text)));
        }

        serde_json::from_str(&self.text).map_err(|error| format!("failed to parse chat completion response: {error}"))
    }
}

#[derive(Debug, Deserialize)]
struct OpenAiErrorResponse {
    error: OpenAiErrorBody,
}

#[derive(Debug, Deserialize)]
struct OpenAiErrorBody {
    message: String,
}

impl OpenAiErrorResponse {
    fn message_from_response_text(response_text: &str) -> Option<String> {
        serde_json::from_str::<Self>(response_text)
            .ok()
            .map(|response| response.error.message)
    }
}
