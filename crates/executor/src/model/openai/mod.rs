mod format;
mod request;
mod response;
mod tool;
mod transport;

#[cfg(test)]
mod tests;

use crate::model::provider::ModelProvider;
use crate::model::types::{ModelRequest, ModelResponse};
use crate::runtime::ExecutorError;
use async_trait::async_trait;
use response::ChatCompletionResponseExt;
use serde_json::json;
use tool::FinalizeResult;
use transport::OpenAiChatCompletionClient;

const MAX_TOOL_CALL_ROUNDS: usize = 8;

#[derive(Debug, Clone, Default)]
pub struct OpenAiModelProvider;

#[async_trait]
impl ModelProvider for OpenAiModelProvider {
    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, ExecutorError> {
        let client = OpenAiChatCompletionClient::new();
        let mut last_error = None;
        let mut messages = self.build_initial_messages(&request)?;

        log::info!(
            "starting OpenAI generation: agent={}, model={}, tools={}",
            request.agent_name,
            request.model_name,
            request.tools.len()
        );

        for round_index in 0..MAX_TOOL_CALL_ROUNDS {
            let completion_request = self.build_completion_request(&request, messages.clone())?;
            log::debug!(
                "sending provider request: agent={}, round={}, messages={}, tools={}",
                request.agent_name,
                round_index + 1,
                completion_request.messages.len(),
                completion_request.tools.len()
            );
            let completion_result = client.send(&request, completion_request).await;
            let completion = match completion_result {
                Ok(completion) => completion,
                Err(error) => {
                    log::warn!(
                        "provider request failed: agent={}, round={}, error={}",
                        request.agent_name,
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
                let tool_call_round = self.execute_tool_calls(&request, &tool_calls)?;

                if let Some(finalize_result) = tool_call_round.finalize_result {
                    return self.complete_generation(&request, finalize_result);
                }

                let assistant_message = completion.extract_tool_call_message()?;
                messages.push(assistant_message);
                messages.extend(tool_call_round.messages);

                continue;
            }

            if let Some(assistant_message) = completion.extract_assistant_content_message() {
                messages.push(assistant_message);
            }

            messages.push(json!({
                "role": "user",
                "content": "To finish this agent run you must call the internal `finalize` tool. Call `finalize` with `{\"type\":\"success\",\"output\":...}` when the output is ready and matches the schema, or `{\"type\":\"fail\",\"reason\":\"...\"}` when you cannot fulfill the request. Do not answer with plain text.",
            }));
            last_error = completion
                .extract_assistant_content()
                .map(|content| format!("model stopped with text instead of calling finalize: {content}"))
                .or_else(|| Some("model response did not include finalize tool call".to_string()));
        }

        Err(ExecutorError::Model {
            agent_name: request.agent_name,
            message: last_error.unwrap_or_else(|| "model did not call finalize".to_string()),
        })
    }
}

impl OpenAiModelProvider {
    fn complete_generation(&self, request: &ModelRequest, finalize_result: FinalizeResult) -> Result<ModelResponse, ExecutorError> {
        match finalize_result {
            FinalizeResult::Success(output) => {
                log::info!("OpenAI generation completed: agent={}", request.agent_name);

                Ok(ModelResponse {
                    output,
                    context: json!({
                        "provider": "openai",
                        "model": request.model_name,
                    }),
                })
            }
            FinalizeResult::Fail(reason) => Err(ExecutorError::Model {
                agent_name: request.agent_name.clone(),
                message: format!("agent finalized with failure: {reason}"),
            }),
        }
    }
}
