use crate::model::provider::ModelProvider;
use crate::model::response::parse_model_json_output;
use crate::model::types::{ModelRequest, ModelResponse, ModelToolSource};
use crate::runtime::ExecutorError;
use async_openai::config::OpenAIConfig;
use async_openai::types::{
    ChatCompletionMessageToolCall, ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage,
    ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestToolMessageArgs, ChatCompletionRequestToolMessageContent,
    ChatCompletionRequestUserMessageArgs, ChatCompletionToolArgs, ChatCompletionToolChoiceOption, CreateChatCompletionRequestArgs,
    CreateChatCompletionResponse, FunctionObjectArgs, ResponseFormat, ResponseFormatJsonSchema,
};
use async_openai::Client;
use async_trait::async_trait;
use superwire_core::mcp::{McpClient, McpServerConfig};

const MAX_TOOL_CALL_ROUNDS: usize = 8;

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
            let mut messages = self.build_initial_messages(&request)?;

            for _ in 0..MAX_TOOL_CALL_ROUNDS {
                let completion_request = self.build_completion_request(&request, response_mode, messages.clone())?;
                let completion_result = client.chat().create(completion_request).await;
                let completion = match completion_result {
                    Ok(completion) => completion,
                    Err(error) => {
                        last_error = Some(error.to_string());

                        break;
                    }
                };

                if let Some(tool_calls) = completion.extract_tool_calls() {
                    let tool_call_messages = self.execute_tool_calls(&request, &tool_calls)?;
                    let assistant_message = ChatCompletionRequestAssistantMessageArgs::default()
                        .tool_calls(tool_calls)
                        .build()
                        .map_err(|error| ExecutorError::Model {
                            agent_name: request.agent_name.clone(),
                            message: format!("failed to build assistant tool call message: {error}"),
                        })?;
                    messages.push(ChatCompletionRequestMessage::Assistant(assistant_message));
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

impl OpenAiModelProvider {
    fn client(&self, request: &ModelRequest) -> Client<OpenAIConfig> {
        let config = OpenAIConfig::new()
            .with_api_base(request.provider_config.endpoint.trim_end_matches('/'))
            .with_api_key(request.provider_config.api_key.clone());

        Client::with_config(config)
    }

    fn build_initial_messages(&self, request: &ModelRequest) -> Result<Vec<ChatCompletionRequestMessage>, ExecutorError> {
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

        Ok(vec![
            ChatCompletionRequestMessage::System(system_message),
            ChatCompletionRequestMessage::User(user_message),
        ])
    }

    fn build_completion_request(
        &self,
        request: &ModelRequest,
        response_mode: OpenAiResponseMode,
        messages: Vec<ChatCompletionRequestMessage>,
    ) -> Result<async_openai::types::CreateChatCompletionRequest, ExecutorError> {
        let mut completion_request = CreateChatCompletionRequestArgs::default();
        completion_request.model(request.model_name.clone()).messages(messages);

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

        if !tools.is_empty() {
            completion_request.tools(tools).tool_choice(ChatCompletionToolChoiceOption::Auto);
        }

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

    fn execute_tool_calls(
        &self,
        request: &ModelRequest,
        tool_calls: &[ChatCompletionMessageToolCall],
    ) -> Result<Vec<ChatCompletionRequestMessage>, ExecutorError> {
        let mut messages = Vec::new();

        for tool_call in tool_calls {
            let tool_result = self.execute_tool_call(request, tool_call)?;
            let tool_result_text = serde_json::to_string(&tool_result).map_err(|error| ExecutorError::Model {
                agent_name: request.agent_name.clone(),
                message: format!("failed to serialize tool result: {error}"),
            })?;
            let tool_message = ChatCompletionRequestToolMessageArgs::default()
                .tool_call_id(tool_call.id.clone())
                .content(ChatCompletionRequestToolMessageContent::Text(tool_result_text))
                .build()
                .map_err(|error| ExecutorError::Model {
                    agent_name: request.agent_name.clone(),
                    message: format!("failed to build tool result message: {error}"),
                })?;

            messages.push(ChatCompletionRequestMessage::Tool(tool_message));
        }

        Ok(messages)
    }

    fn execute_tool_call(
        &self,
        request: &ModelRequest,
        tool_call: &ChatCompletionMessageToolCall,
    ) -> Result<serde_json::Value, ExecutorError> {
        let tool_definition = request
            .tools
            .iter()
            .find(|tool_definition| format_tool_name(&tool_definition.name) == tool_call.function.name)
            .ok_or_else(|| ExecutorError::Model {
                agent_name: request.agent_name.clone(),
                message: format!("model requested unknown tool `{}`", tool_call.function.name),
            })?;
        let mut arguments =
            serde_json::from_str::<serde_json::Value>(&tool_call.function.arguments).map_err(|error| ExecutorError::Model {
                agent_name: request.agent_name.clone(),
                message: format!("model provided invalid arguments for tool `{}`: {error}", tool_call.function.name),
            })?;

        if let (Some(argument_object), Some(binding_object)) = (arguments.as_object_mut(), tool_definition.bindings.as_object()) {
            for (binding_name, binding_value) in binding_object {
                argument_object.insert(binding_name.clone(), binding_value.clone());
            }
        }

        match &tool_definition.source {
            ModelToolSource::Mcp {
                server_name,
                tool_name,
                endpoint,
                headers,
            } => {
                let server_config = McpServerConfig {
                    name: server_name.clone().unwrap_or_else(|| "default".to_string()),
                    endpoint: endpoint.clone(),
                    headers: headers.clone(),
                };
                let result = McpClient::new(server_config)
                    .call_tool(tool_name, arguments)
                    .map_err(|error| ExecutorError::Model {
                        agent_name: request.agent_name.clone(),
                        message: error.to_string(),
                    })?;

                Ok(normalize_mcp_tool_result(result))
            }
            ModelToolSource::Local => Err(ExecutorError::Model {
                agent_name: request.agent_name.clone(),
                message: format!("tool `{}` is not backed by MCP", tool_definition.name),
            }),
        }
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

    fn extract_tool_calls(&self) -> Option<Vec<ChatCompletionMessageToolCall>>;
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

    fn extract_tool_calls(&self) -> Option<Vec<ChatCompletionMessageToolCall>> {
        self.choices.iter().find_map(|choice| {
            let tool_calls = choice.message.tool_calls.clone()?;

            (!tool_calls.is_empty()).then_some(tool_calls)
        })
    }
}

fn normalize_mcp_tool_result(result: serde_json::Value) -> serde_json::Value {
    if let Some(structured_content) = result.get("structuredContent") {
        return structured_content.clone();
    }

    if let Some(text_content) = result
        .get("content")
        .and_then(serde_json::Value::as_array)
        .and_then(|content| content.first())
        .and_then(|content_item| content_item.get("text"))
        .and_then(serde_json::Value::as_str)
    {
        return serde_json::from_str(text_content).unwrap_or_else(|_| serde_json::Value::String(text_content.to_string()));
    }

    result
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

fn format_tool_name(tool_name: &str) -> String {
    let mut formatted_tool_name = tool_name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();

    if formatted_tool_name.is_empty() {
        formatted_tool_name = "tool".to_string();
    }

    formatted_tool_name.truncate(64);
    formatted_tool_name
}

#[cfg(test)]
mod tests {
    use super::{format_response_schema_name, format_tool_name, OpenAiModelProvider, OpenAiResponseMode};
    use crate::model::{ModelRequest, ModelToolDefinition, ModelToolSource};
    use async_openai::types::{ChatCompletionMessageToolCall, ChatCompletionToolType, FunctionCall};
    use serde_json::{json, Value};
    use std::collections::BTreeMap;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use superwire_core::semantic::support::provider::OpenAIProviderConfig;

    #[test]
    fn formats_response_schema_name_for_openai_constraints() {
        assert_eq!(format_response_schema_name("agent name!*"), "agent_name__");
    }

    #[test]
    fn formats_tool_name_for_openai_constraints() {
        assert_eq!(format_tool_name("update user!*"), "update_user__");
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

    #[test]
    fn executes_mcp_tool_call_from_model_request() {
        let provider = OpenAiModelProvider;
        let server = TestMcpHttpServer::spawn();
        let request = ModelRequest {
            agent_name: "updater".to_string(),
            provider_config: OpenAIProviderConfig {
                endpoint: "https://api.openai.com/v1".to_string(),
                api_key: "test-api-key".to_string(),
            },
            model_name: "gpt-4.1-mini".to_string(),
            prompt: "Rename the user".to_string(),
            output_schema: serde_json::json!({ "type": "object" }),
            tools: vec![ModelToolDefinition {
                name: "update_user_name".to_string(),
                description: Some("Update a user name".to_string()),
                source: ModelToolSource::Mcp {
                    server_name: Some("local".to_string()),
                    tool_name: "update-user-name".to_string(),
                    endpoint: server.endpoint(),
                    headers: [("Authorization".to_string(), "Bearer test-token".to_string())].into(),
                },
                input_schema: serde_json::json!({ "type": "object" }),
                output_schema: serde_json::json!({ "type": "object" }),
                bindings: serde_json::json!({ "project_id": 14, "user_id": 123 }),
            }],
        };
        let tool_call = ChatCompletionMessageToolCall {
            id: "call_1".to_string(),
            r#type: ChatCompletionToolType::Function,
            function: FunctionCall {
                name: "update_user_name".to_string(),
                arguments: serde_json::json!({ "user_id": 999, "user_name": "Ada" }).to_string(),
            },
        };

        let result = provider
            .execute_tool_call(&request, &tool_call)
            .expect("MCP tool call should execute");

        assert_eq!(result, serde_json::json!({ "success": true }));
        assert_eq!(
            server.received_tool_arguments(),
            Some(serde_json::json!({
                "project_id": 14,
                "user_id": 123,
                "user_name": "Ada"
            }))
        );
    }

    struct TestMcpHttpServer {
        endpoint: String,
        received_tool_arguments: Arc<Mutex<Option<Value>>>,
    }

    impl TestMcpHttpServer {
        fn spawn() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("test MCP listener should bind");
            let endpoint = format!("http://{}", listener.local_addr().expect("local address should exist"));
            let received_tool_arguments = Arc::new(Mutex::new(None));
            let thread_received_tool_arguments = Arc::clone(&received_tool_arguments);

            thread::spawn(move || {
                for incoming_stream in listener.incoming().take(3) {
                    let stream = incoming_stream.expect("test MCP stream should open");
                    handle_mcp_request(stream, &thread_received_tool_arguments);
                }
            });

            Self {
                endpoint,
                received_tool_arguments,
            }
        }

        fn endpoint(&self) -> String {
            self.endpoint.clone()
        }

        fn received_tool_arguments(&self) -> Option<Value> {
            self.received_tool_arguments
                .lock()
                .expect("received tool arguments lock should not be poisoned")
                .clone()
        }
    }

    fn handle_mcp_request(mut stream: TcpStream, received_tool_arguments: &Arc<Mutex<Option<Value>>>) {
        let mut reader = BufReader::new(stream.try_clone().expect("stream clone should succeed"));
        let mut request_headers = BTreeMap::new();
        let mut content_length = 0_usize;
        let mut header_line = String::new();

        loop {
            header_line.clear();
            reader.read_line(&mut header_line).expect("header line should read");

            if header_line == "\r\n" || header_line.is_empty() {
                break;
            }

            if let Some(value) = header_line.to_ascii_lowercase().strip_prefix("content-length:") {
                content_length = value.trim().parse().expect("content length should parse");
            }

            if let Some((header_name, header_value)) = header_line.trim_end().split_once(':') {
                request_headers.insert(header_name.to_ascii_lowercase(), header_value.trim().to_string());
            }
        }

        assert_eq!(
            request_headers.get("authorization"),
            Some(&"Bearer test-token".to_string()),
            "expected MCP request authorization header"
        );

        let mut request_body = vec![0_u8; content_length];
        reader.read_exact(&mut request_body).expect("request body should read");
        let request: Value = serde_json::from_slice(&request_body).expect("request body should be JSON");

        if request.get("method").and_then(Value::as_str) == Some("tools/call") {
            *received_tool_arguments
                .lock()
                .expect("received tool arguments lock should not be poisoned") =
                request.get("params").and_then(|params| params.get("arguments")).cloned();
        }

        let response = if let Some(response_body) = response_for_method(request.get("method").and_then(Value::as_str)) {
            let response_body = response_body.to_string();

            format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                response_body.len(),
                response_body
            )
        } else {
            "HTTP/1.1 202 Accepted\r\ncontent-length: 0\r\n\r\n".to_string()
        };

        stream.write_all(response.as_bytes()).expect("response should write");
    }

    fn response_for_method(method: Option<&str>) -> Option<Value> {
        match method {
            Some("notifications/initialized") => None,
            Some("tools/call") => Some(json!({
                "jsonrpc": "2.0",
                "id": 2,
                "result": {
                    "content": [{ "type": "text", "text": "{\"success\":true}" }],
                    "structuredContent": { "success": true }
                }
            })),
            _ => Some(json!({ "jsonrpc": "2.0", "id": 1, "result": {} })),
        }
    }
}
