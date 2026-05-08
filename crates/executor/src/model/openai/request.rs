use super::format::format_tool_name;
use super::transport::OpenAiChatCompletionRequest;
use crate::model::types::ModelRequest;
use crate::runtime::ExecutorError;
use async_openai::types::{
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs, ChatCompletionToolArgs,
    ChatCompletionToolChoiceOption, FunctionObjectArgs,
};
use serde_json::Value;

impl super::OpenAiModelProvider {
    pub(super) fn build_initial_messages(&self, request: &ModelRequest) -> Result<Vec<Value>, ExecutorError> {
        let output_schema_text = serde_json::to_string(&request.output_schema).map_err(|error| ExecutorError::Model {
            agent_name: request.agent_name.clone(),
            message: format!("failed to serialize output schema: {error}"),
        })?;
        let system_message = ChatCompletionRequestSystemMessageArgs::default()
            .content(format!(
                "You are executing a deterministic workflow agent. You must finish by calling the internal `finalize` tool. Do not end with assistant text. For success, call `finalize` with type `success` and an `output` value that matches this JSON Schema: {output_schema_text}. If you cannot fulfill the request, call `finalize` with type `fail` and a clear `reason`. Never put failure or apology text in a success output."
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

        Ok(vec![
            ChatCompletionRequestMessage::System(system_message).to_json_value(&request.agent_name)?,
            ChatCompletionRequestMessage::User(user_message).to_json_value(&request.agent_name)?,
        ])
    }

    pub(super) fn build_completion_request(
        &self,
        request: &ModelRequest,
        messages: Vec<Value>,
    ) -> Result<OpenAiChatCompletionRequest, ExecutorError> {
        let tools = request
            .tools
            .iter()
            .map(|tool_definition| {
                let function = FunctionObjectArgs::default()
                    .name(format_tool_name(&tool_definition.name))
                    .description(
                        tool_definition
                            .description
                            .clone()
                            .unwrap_or_else(|| format!("Workflow tool `{}`", tool_definition.name)),
                    )
                    .parameters(tool_definition.input_schema.clone())
                    .strict(true)
                    .build()
                    .map_err(|error| ExecutorError::Model {
                        agent_name: request.agent_name.clone(),
                        message: format!("failed to build tool `{}`: {error}", tool_definition.name),
                    })?;

                ChatCompletionToolArgs::default()
                    .function(function)
                    .build()
                    .map_err(|error| ExecutorError::Model {
                        agent_name: request.agent_name.clone(),
                        message: format!("failed to build chat tool `{}`: {error}", tool_definition.name),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(OpenAiChatCompletionRequest {
            model: request.model_name.clone(),
            messages,
            tools,
            tool_choice: (!request.tools.is_empty()).then_some(ChatCompletionToolChoiceOption::Auto),
        })
    }
}

pub(super) trait ChatCompletionRequestMessageExt {
    fn to_json_value(self, agent_name: &str) -> Result<Value, ExecutorError>;
}

impl ChatCompletionRequestMessageExt for ChatCompletionRequestMessage {
    fn to_json_value(self, agent_name: &str) -> Result<Value, ExecutorError> {
        serde_json::to_value(self).map_err(|error| ExecutorError::Model {
            agent_name: agent_name.to_string(),
            message: format!("failed to serialize chat message: {error}"),
        })
    }
}
