use crate::model::provider::ModelProvider;
use crate::model::response::parse_model_json_output;
use crate::model::types::{ModelRequest, ModelResponse};
use crate::runtime::ExecutorError;
use async_openai::config::OpenAIConfig;
use async_openai::types::{
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
    CreateChatCompletionRequestArgs, CreateChatCompletionResponse, ResponseFormat, ResponseFormatJsonSchema,
};
use async_openai::Client;
use async_trait::async_trait;

#[derive(Debug, Clone, Default)]
pub struct OpenAiModelProvider;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenAiResponseMode {
    JsonSchema,
    JsonObject,
    InstructionOnly,
}

#[async_trait]
impl ModelProvider for OpenAiModelProvider {
    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, ExecutorError> {
        let client = self.client(&request);
        let mut last_error = None;

        for response_mode in OpenAiResponseMode::fallback_order() {
            let completion_request = self.build_completion_request(&request, response_mode)?;
            let completion_result = client.chat().create(completion_request).await;
            let completion = match completion_result {
                Ok(completion) => completion,
                Err(error) => {
                    last_error = Some(error.to_string());

                    continue;
                }
            };
            let Some(content) = completion.extract_assistant_content() else {
                last_error = Some("model response did not include assistant content".to_string());

                continue;
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

        Err(ExecutorError::Model {
            agent_name: request.agent_name,
            message: last_error.unwrap_or_else(|| "model did not produce valid JSON output".to_string()),
        })
    }
}

impl OpenAiModelProvider {
    fn client(&self, request: &ModelRequest) -> Client<OpenAIConfig> {
        let config = OpenAIConfig::new()
            .with_api_base(request.provider_config.endpoint.trim_end_matches('/'))
            .with_api_key(request.provider_config.api_key.clone());

        Client::with_config(config)
    }

    fn build_completion_request(
        &self,
        request: &ModelRequest,
        response_mode: OpenAiResponseMode,
    ) -> Result<async_openai::types::CreateChatCompletionRequest, ExecutorError> {
        let output_schema_text = serde_json::to_string(&request.output_schema).map_err(|error| ExecutorError::Model {
            agent_name: request.agent_name.clone(),
            message: format!("failed to serialize output schema: {error}"),
        })?;
        let system_message = ChatCompletionRequestSystemMessageArgs::default()
            .content(format!(
                "You are executing a deterministic workflow agent. Respond only with a JSON value that matches this JSON Schema. Do not include markdown, prose, or code fences. Schema: {output_schema_text}"
            ))
            .build()
            .map_err(|error| ExecutorError::Model {
                agent_name: request.agent_name.clone(),
                message: format!("failed to build system message: {error}"),
            })?;
        let user_message = ChatCompletionRequestUserMessageArgs::default()
            .content(request.prompt.clone())
            .build()
            .map_err(|error| ExecutorError::Model {
                agent_name: request.agent_name.clone(),
                message: format!("failed to build user message: {error}"),
            })?;
        let mut completion_request = CreateChatCompletionRequestArgs::default();
        completion_request.model(request.model_name.clone()).messages(vec![
            ChatCompletionRequestMessage::System(system_message),
            ChatCompletionRequestMessage::User(user_message),
        ]);

        match response_mode {
            OpenAiResponseMode::JsonSchema => {
                completion_request.response_format(ResponseFormat::JsonSchema {
                    json_schema: ResponseFormatJsonSchema {
                        description: Some(format!("Output schema for agent `{}`", request.agent_name)),
                        name: format_response_schema_name(&request.agent_name),
                        schema: Some(request.output_schema.clone()),
                        strict: Some(true),
                    },
                });
            }
            OpenAiResponseMode::JsonObject => {
                completion_request.response_format(ResponseFormat::JsonObject);
            }
            OpenAiResponseMode::InstructionOnly => {}
        }

        completion_request.build().map_err(|error| ExecutorError::Model {
            agent_name: request.agent_name.clone(),
            message: format!("failed to build chat completion request: {error}"),
        })
    }
}

impl OpenAiResponseMode {
    fn fallback_order() -> [Self; 3] {
        [Self::JsonSchema, Self::JsonObject, Self::InstructionOnly]
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::JsonSchema => "json_schema",
            Self::JsonObject => "json_object",
            Self::InstructionOnly => "instruction_only",
        }
    }
}

trait ChatCompletionResponseExt {
    fn extract_assistant_content(&self) -> Option<String>;
}

impl ChatCompletionResponseExt for CreateChatCompletionResponse {
    fn extract_assistant_content(&self) -> Option<String> {
        self.choices
            .iter()
            .filter_map(|choice| choice.message.content.as_deref())
            .map(str::trim)
            .find(|content| !content.is_empty())
            .map(str::to_string)
    }
}

fn format_response_schema_name(agent_name: &str) -> String {
    let mut schema_name = agent_name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();

    if schema_name.is_empty() {
        schema_name = "agent_output".to_string();
    }

    schema_name.truncate(64);
    schema_name
}

#[cfg(test)]
mod tests {
    use super::{format_response_schema_name, OpenAiResponseMode};

    #[test]
    fn formats_response_schema_name_for_openai_constraints() {
        assert_eq!(format_response_schema_name("agent name!*"), "agent_name__");
    }

    #[test]
    fn orders_response_modes_from_strict_to_compatible() {
        assert_eq!(
            OpenAiResponseMode::fallback_order(),
            [
                OpenAiResponseMode::JsonSchema,
                OpenAiResponseMode::JsonObject,
                OpenAiResponseMode::InstructionOnly,
            ]
        );
    }
}
