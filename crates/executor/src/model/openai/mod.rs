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

        log::info!(
            "starting OpenAI generation: agent={}, model={}, tools={}",
            request.agent_name,
            request.model_name,
            request.tools.len()
        );

        for response_mode in OpenAiResponseMode::response_modes(request.response_format) {
            let mut messages = self.build_initial_messages(&request)?;

            log::debug!("agent `{}` entering response mode `{}`", request.agent_name, response_mode.as_str());

            for round_index in 0..MAX_TOOL_CALL_ROUNDS {
                let completion_request = self.build_completion_request(&request, response_mode, messages.clone())?;
                log::debug!(
                    "sending provider request: agent={}, mode={}, round={}, messages={}, tools={}",
                    request.agent_name,
                    response_mode.as_str(),
                    round_index + 1,
                    completion_request.messages.len(),
                    completion_request.tools.len()
                );
                let completion_result = client.send(&request, completion_request).await;
                let completion = match completion_result {
                    Ok(completion) => completion,
                    Err(error) => {
                        log::warn!(
                            "provider request failed: agent={}, mode={}, round={}, error={}",
                            request.agent_name,
                            response_mode.as_str(),
                            round_index + 1,
                            error
                        );
                        last_error = Some(error);

                        break;
                    }
                };

                if let Some(tool_calls) = completion.extract_tool_calls() {
                    log::info!(
                        "provider requested tool calls: agent={}, count={}",
                        request.agent_name,
                        tool_calls.len()
                    );
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
                        log::info!(
                            "OpenAI generation completed: agent={}, mode={}",
                            request.agent_name,
                            response_mode.as_str()
                        );
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
                        log::debug!(
                            "failed to parse model output: agent={}, mode={}, error={}",
                            request.agent_name,
                            response_mode.as_str(),
                            error
                        );
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
