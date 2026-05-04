mod format;
mod request;
mod response;
mod tool;
mod transport;

#[cfg(test)]
mod tests;

use crate::model::provider::ModelProvider;
use crate::model::response::parse_model_json_output;
use crate::model::types::{ModelRequest, ModelResponse};
use crate::runtime::ExecutorError;
use async_trait::async_trait;
use request::OpenAiResponseMode;
use response::ChatCompletionResponseExt;
use transport::OpenAiChatCompletionClient;

const MAX_TOOL_CALL_ROUNDS: usize = 8;

#[derive(Debug, Clone, Default)]
pub struct OpenAiModelProvider;

#[async_trait]
impl ModelProvider for OpenAiModelProvider {
    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, ExecutorError> {
        let client = OpenAiChatCompletionClient::new();
        let mut last_error = None;

        for response_mode in OpenAiResponseMode::fallback_order() {
            let mut messages = self.build_initial_messages(&request)?;

            for _ in 0..MAX_TOOL_CALL_ROUNDS {
                let completion_request = self.build_completion_request(&request, response_mode, messages.clone())?;
                let completion_result = client.send(&request, completion_request).await;
                let completion = match completion_result {
                    Ok(completion) => completion,
                    Err(error) => {
                        last_error = Some(error);

                        break;
                    }
                };

                if let Some(tool_calls) = completion.extract_tool_calls() {
                    let tool_call_messages = self.execute_tool_calls(&request, &tool_calls)?;
                    let assistant_message = completion.extract_tool_call_message()?;
                    messages.push(assistant_message);
                    messages.extend(tool_call_messages);

                    continue;
                }

                let Some(content) = completion.extract_assistant_content() else {
                    last_error = Some("model response did not include assistant content".to_string());

                    break;
                };

                match parse_model_json_output(&request.agent_name, &content) {
                    Ok(output) => {
                        return Ok(ModelResponse {
                            output,
                            context: serde_json::json!({
                                "provider": "openai",
                                "model": request.model_name,
                                "response_mode": response_mode.as_str(),
                            }),
                        });
                    }
                    Err(error) => {
                        last_error = Some(error.to_string());
                    }
                }
            }
        }

        Err(ExecutorError::Model {
            agent_name: request.agent_name,
            message: last_error.unwrap_or_else(|| "model did not produce valid JSON output".to_string()),
        })
    }
}
