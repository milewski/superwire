use super::fixtures;
use crate::api::ValidationRequest;
use crate::event::ExecutorEventKind;
use crate::model::{ModelToolSource, ToolCallLimitScope};
use crate::service::ExecutorService;
use crate::tests::support::{request, TrackingModelProvider};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use superwire_core::workflow_source;

#[tokio::test]
async fn agent_tool_definitions_are_passed_to_model_provider() {
    let server = TestMcpHttpServer::spawn([("authorization".to_string(), "Bearer test-token".to_string())]);
    let workflow_source = workflow_source! {
        provider openai from openai {
            endpoint: "https://api.openai.com/v1"
            api_key: "test-api-key"
        }

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        mcp local {
            endpoint: "__ENDPOINT__"
            headers {
                Authorization: "Bearer test-token"
            }
        }

        input {
            user_id: number
        }

        tool local_update_user from mcp.local.tool.update_user_name {
            bindings {
                user_id: input.user_id
            }
        }

        agent updater {
            model: model.openai_model
            uses: [tool.local_update_user]
            instruction: "Rename the user"
            output {
                value: string
            }
        }

        output {
            value: agent.updater.value
        }
    }
    .replace("__ENDPOINT__", &server.endpoint());

    let model_provider = TrackingModelProvider::new(vec![json!({ "value": "renamed" })]);
    let service = ExecutorService::new(model_provider.clone());

    service
        .execute(request_with_input(&workflow_source, json!({ "user_id": 123 })))
        .await
        .expect("execution should pass tool metadata to provider");

    let recorded_requests = model_provider
        .recorded_requests
        .lock()
        .expect("tracking lock should not be poisoned");

    let request = recorded_requests.first().expect("model request should be recorded");
    let tool_definition = request.tools.first().expect("tool definition should be present");

    assert_eq!(tool_definition.name, "local_update_user");
    assert_eq!(tool_definition.description.as_deref(), Some("Update a user name"));
    assert_eq!(
        tool_definition.source,
        ModelToolSource::Mcp {
            server_name: Some("local".to_string()),
            tool_name: "update_user_name".to_string(),
            endpoint: server.endpoint(),
            headers: [("Authorization".to_string(), "Bearer test-token".to_string())].into(),
        }
    );

    assert_eq!(tool_definition.bindings, json!({ "user_id": 123 }));
    let input_schema = tool_definition.input_schema.json_value();
    let output_schema = tool_definition.output_schema.json_value();

    assert_eq!(input_schema["required"], json!(["user_name"]));
    assert_eq!(input_schema.pointer("/properties/user_id"), None);
    assert_eq!(output_schema["required"], json!(["success"]));
}

pub(crate) struct TestMcpHttpServer {
    endpoint: String,
    recorded_methods: Arc<Mutex<Vec<TestMcpMethod>>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TestMcpMethod {
    Initialized,
    ToolsList,
    ToolsCall,
    PromptsList,
    ResourcesList,
    ResourcesRead,
    PromptsGet,
    Unknown,
}

#[derive(Clone, Copy)]
enum JsonSchemaType {
    String,
    Number,
    Boolean,
    Object,
}

struct SchemaField {
    name: &'static str,
    schema: Value,
}

struct TestMcpCatalog;

impl TestMcpHttpServer {
    pub(crate) fn spawn(expected_headers: impl IntoIterator<Item = (String, String)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test MCP listener should bind");
        let endpoint = format!("http://{}", listener.local_addr().expect("local address should exist"));
        let expected_headers = expected_headers.into_iter().collect::<BTreeMap<_, _>>();
        let catalog = TestMcpCatalog;
        let recorded_methods = Arc::new(Mutex::new(Vec::new()));
        let server_recorded_methods = Arc::clone(&recorded_methods);

        thread::spawn(move || {
            for incoming_stream in listener.incoming().take(12) {
                let stream = incoming_stream.expect("test MCP stream should open");
                handle_mcp_request(stream, &expected_headers, &catalog, &server_recorded_methods);
            }
        });

        Self {
            endpoint,
            recorded_methods,
        }
    }

    pub(crate) fn endpoint(&self) -> String {
        self.endpoint.clone()
    }

    pub(crate) fn method_count(&self, method: TestMcpMethod) -> usize {
        self.recorded_methods
            .lock()
            .expect("MCP method records lock should not be poisoned")
            .iter()
            .filter(|recorded_method| **recorded_method == method)
            .count()
    }
}

impl TestMcpMethod {
    fn from_request(request: &Value) -> Self {
        match request.get("method").and_then(Value::as_str) {
            Some("notifications/initialized") => Self::Initialized,
            Some("tools/list") => Self::ToolsList,
            Some("tools/call") => Self::ToolsCall,
            Some("prompts/list") => Self::PromptsList,
            Some("resources/list") => Self::ResourcesList,
            Some("resources/read") => Self::ResourcesRead,
            Some("prompts/get") => Self::PromptsGet,
            _ => Self::Unknown,
        }
    }
}

impl JsonSchemaType {
    fn as_str(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::Object => "object",
        }
    }
}

impl TestMcpCatalog {
    fn response_for(&self, method: TestMcpMethod, request: &Value) -> Option<Value> {
        match method {
            TestMcpMethod::Initialized => None,
            TestMcpMethod::ToolsList => Some(jsonrpc_result(2, json!({ "tools": self.tools() }))),
            TestMcpMethod::ToolsCall => Some(jsonrpc_result(3, self.tool_call_result(request))),
            TestMcpMethod::PromptsList => Some(jsonrpc_result(2, json!({ "prompts": self.prompts() }))),
            TestMcpMethod::ResourcesList => Some(jsonrpc_result(2, json!({ "resources": self.resources() }))),
            TestMcpMethod::ResourcesRead => Some(jsonrpc_result(3, self.project_readme_content())),
            TestMcpMethod::PromptsGet => Some(jsonrpc_result(2, self.system_prompt_result())),
            TestMcpMethod::Unknown => Some(jsonrpc_result(1, json!({}))),
        }
    }

    fn tool_call_result(&self, request: &Value) -> Value {
        match request.pointer("/params/name").and_then(Value::as_str) {
            Some("fetch_qualitative_question_answers") => self.fetch_qualitative_question_answers_result(),
            _ => json!({ "content": [{ "type": "text", "text": "{}" }] }),
        }
    }

    fn fetch_qualitative_question_answers_result(&self) -> Value {
        let structured_content = json!({
            "task_group_title": "Ignored group title",
            "answers": [
                {
                    "participant_name": "jon",
                    "participant_id": 1,
                    "task_title": "Example",
                    "task_id": 1,
                    "task_type": "open_written",
                    "answer": {
                        "text": "hello world",
                        "attachments": null
                    }
                }
            ]
        });

        json!({
            "content": [
                {
                    "type": "text",
                    "text": structured_content.to_string()
                }
            ],
            "isError": false,
            "structuredContent": structured_content
        })
    }

    fn tools(&self) -> Vec<Value> {
        vec![
            mcp_tool(
                "update_user_name",
                "Update a user name",
                object_schema(
                    [
                        schema_field("user_id", primitive_schema(JsonSchemaType::Number)),
                        schema_field("user_name", string_enum_schema(["Ada", "Grace"])),
                    ],
                    ["user_id", "user_name"],
                ),
                object_schema([schema_field("success", primitive_schema(JsonSchemaType::Boolean))], ["success"]),
            ),
            mcp_tool(
                "list_participants",
                "List participants",
                object_schema(
                    [
                        schema_field("project_id", primitive_schema(JsonSchemaType::Number)),
                        schema_field("task_id", primitive_schema(JsonSchemaType::Number)),
                    ],
                    ["project_id", "task_id"],
                ),
                object_schema(
                    [schema_field("participants", array_schema(primitive_schema(JsonSchemaType::Object)))],
                    ["participants"],
                ),
            ),
            self.edit_project_for_workspace_tool(),
            mcp_tool(
                "create-sorting-task",
                "Create sorting task",
                object_schema(
                    [
                        schema_field("project_id", primitive_schema(JsonSchemaType::Number)),
                        schema_field("task_id", primitive_schema(JsonSchemaType::Number)),
                        schema_field("title", primitive_schema(JsonSchemaType::String)),
                    ],
                    ["project_id", "task_id", "title"],
                ),
                object_schema([schema_field("task_id", primitive_schema(JsonSchemaType::Number))], ["task_id"]),
            ),
            mcp_tool(
                "update-task-status",
                "Update task status",
                object_schema(
                    [
                        schema_field("project_id", primitive_schema(JsonSchemaType::Number)),
                        schema_field("task_id", primitive_schema(JsonSchemaType::Number)),
                        schema_field("status", string_enum_schema(["todo", "done"])),
                    ],
                    ["project_id", "task_id", "status"],
                ),
                object_schema([schema_field("success", primitive_schema(JsonSchemaType::Boolean))], ["success"]),
            ),
            mcp_tool(
                "assign-task",
                "Assign task",
                object_schema(
                    [
                        schema_field("project_id", primitive_schema(JsonSchemaType::Number)),
                        schema_field("task_id", primitive_schema(JsonSchemaType::Number)),
                        schema_field("user_id", primitive_schema(JsonSchemaType::Number)),
                    ],
                    ["project_id", "task_id", "user_id"],
                ),
                object_schema([schema_field("success", primitive_schema(JsonSchemaType::Boolean))], ["success"]),
            ),
            self.fetch_qualitative_question_answers_tool(),
        ]
    }

    fn fetch_qualitative_question_answers_tool(&self) -> Value {
        mcp_tool(
            "fetch_qualitative_question_answers",
            "Fetch qualitative question answers",
            json!({
                "type": "object",
                "properties": {
                    "project_id": {
                        "description": "The ID of the project",
                        "type": "integer"
                    },
                    "task_group_id": {
                        "description": "The ID of the task group to fetch answers from. If omitted, answers are fetched across all task groups in the project.",
                        "type": ["integer", "null"]
                    },
                    "task_types": {
                        "description": "The task types to filter by (e.g., video_recording, open_written)",
                        "type": "array",
                        "items": {
                            "description": "A valid task type value",
                            "type": "string",
                            "enum": [
                                "picture",
                                "video_recording",
                                "multimedia",
                                "likert_scale",
                                "multiple_choice",
                                "open_written",
                                "numerical",
                                "sorting"
                            ]
                        }
                    }
                }
            }),
            object_schema(
                [schema_field("answers", array_schema(primitive_schema(JsonSchemaType::Object)))],
                ["answers"],
            ),
        )
    }

    fn edit_project_for_workspace_tool(&self) -> Value {
        mcp_tool(
            "edit_project_for_workspace",
            "Edit project for workspace",
            object_schema(
                [
                    schema_field(
                        "name",
                        json!({
                            "type": ["array", "null"],
                            "items": {
                                "type": "object",
                                "properties": {
                                    "language": {
                                        "type": "string",
                                        "enum": ["en_US", "es", "fr"]
                                    },
                                    "value": {
                                        "type": "string"
                                    }
                                },
                                "required": ["language", "value"]
                            },
                            "minItems": 1,
                            "uniqueItems": true
                        }),
                    ),
                    schema_field(
                        "primary_language",
                        json!({
                            "type": ["string", "null"],
                            "enum": ["en_US", "es", "fr"]
                        }),
                    ),
                    schema_field(
                        "languages",
                        json!({
                            "type": ["array", "null"],
                            "items": {
                                "type": "string",
                                "enum": ["en_US", "es", "fr"]
                            },
                            "minItems": 1,
                            "uniqueItems": true
                        }),
                    ),
                    schema_field(
                        "description",
                        json!({
                            "type": ["array", "null"],
                            "items": {
                                "type": "object",
                                "properties": {
                                    "language": {
                                        "type": "string",
                                        "enum": ["en_US", "es", "fr"]
                                    },
                                    "value": {
                                        "type": "string"
                                    }
                                },
                                "required": ["language", "value"]
                            },
                            "minItems": 1
                        }),
                    ),
                    schema_field("project_id", json!({ "type": "integer" })),
                ],
                ["name", "primary_language", "languages", "description", "project_id"],
            ),
            object_schema(
                [schema_field("project_id", primitive_schema(JsonSchemaType::Number))],
                ["project_id"],
            ),
        )
    }

    fn resources(&self) -> Vec<Value> {
        vec![json!({
            "name": "project_readme",
            "title": "Project README",
            "description": "The project readme file",
            "mimeType": "text/markdown",
            "uri": "file://resources/project_readme"
        })]
    }

    fn prompts(&self) -> Vec<Value> {
        vec![json!({
            "name": "system_prompt",
            "title": "System Prompt",
            "description": "The system prompt"
        })]
    }

    fn project_readme_content(&self) -> Value {
        json!({
            "contents": [
                {
                    "uri": "file://resources/project_readme",
                    "mimeType": "text/markdown",
                    "text": "# Project README\nUse stable sorting."
                }
            ]
        })
    }

    fn system_prompt_result(&self) -> Value {
        json!({
            "messages": [
                {
                    "role": "system",
                    "content": {
                        "type": "text",
                        "text": "Follow project conventions."
                    }
                }
            ]
        })
    }
}

fn handle_mcp_request(
    mut stream: TcpStream,
    expected_headers: &BTreeMap<String, String>,
    catalog: &TestMcpCatalog,
    recorded_methods: &Arc<Mutex<Vec<TestMcpMethod>>>,
) {
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

    for (header_name, header_value) in expected_headers {
        assert_eq!(
            request_headers.get(header_name),
            Some(header_value),
            "expected MCP request header `{header_name}`"
        );
    }

    let mut request_body = vec![0_u8; content_length];
    reader.read_exact(&mut request_body).expect("request body should read");
    let request: Value = serde_json::from_slice(&request_body).expect("request body should be JSON");
    let method = TestMcpMethod::from_request(&request);

    recorded_methods
        .lock()
        .expect("MCP method records lock should not be poisoned")
        .push(method);

    let response = if let Some(response_body) = catalog.response_for(method, &request) {
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

fn jsonrpc_result(request_id: u64, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "result": result
    })
}

fn mcp_tool(name: &str, description: &str, input_schema: Value, output_schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema,
        "outputSchema": output_schema
    })
}

fn object_schema(fields: impl IntoIterator<Item = SchemaField>, required_fields: impl IntoIterator<Item = &'static str>) -> Value {
    let properties = fields
        .into_iter()
        .map(|field| (field.name.to_string(), field.schema))
        .collect::<serde_json::Map<_, _>>();
    let required_fields = required_fields.into_iter().collect::<Vec<_>>();

    json!({
        "type": JsonSchemaType::Object.as_str(),
        "properties": properties,
        "required": required_fields
    })
}

fn schema_field(name: &'static str, schema: Value) -> SchemaField {
    SchemaField { name, schema }
}

fn primitive_schema(schema_type: JsonSchemaType) -> Value {
    json!({ "type": schema_type.as_str() })
}

fn string_enum_schema(values: impl IntoIterator<Item = &'static str>) -> Value {
    json!({
        "type": JsonSchemaType::String.as_str(),
        "enum": values.into_iter().collect::<Vec<_>>()
    })
}

fn array_schema(item_schema: Value) -> Value {
    json!({
        "type": "array",
        "items": item_schema
    })
}

fn request_with_input(fixture: &str, input: serde_json::Value) -> crate::api::ExecutionRequest {
    let mut execution_request = request(fixture);
    execution_request.input = input;

    execution_request
}

fn fixture_with_mcp_endpoint(workflow_source: &str, endpoint: &str) -> String {
    let endpoint_value = serde_json::to_string(endpoint).expect("endpoint string should serialize");

    workflow_source
        .replace("secrets.mcp_endpoint", &endpoint_value)
        .replace("secrets {\n    mcp_endpoint: string\n}\n\n", "")
}

#[tokio::test]
async fn mcp_resource_and_prompt_imports_are_added_to_agent_prompt() {
    let server = TestMcpHttpServer::spawn([]);
    let workflow_source = workflow_source! {
        provider openai from openai {
            endpoint: "https://api.openai.com/v1"
            api_key: "test-api-key"
        }

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        mcp local {
            endpoint: "__ENDPOINT__"
        }

        input {
            workspace_id: string
        }

        resource project_readme from mcp.local.resource.project_readme {
            bindings {
                workspace_id: input.workspace_id
            }
        }

        prompt system_prompt from mcp.local.prompt.system_prompt {
            bindings {
                workspace_id: input.workspace_id
            }
        }

        agent updater {
            model: model.openai_model
            instruction: "Rename the user"
            output {
                value: string
            }
        }

        output {
            value: agent.updater
        }
    }
    .replace("__ENDPOINT__", &server.endpoint());

    let model_provider = TrackingModelProvider::new(vec![json!({ "value": "done" })]);
    let service = ExecutorService::new(model_provider.clone());

    service
        .execute(request_with_input(&workflow_source, json!({ "workspace_id": "workspace-1" })))
        .await
        .expect("execution should include MCP imports in prompt");

    let recorded_requests = model_provider
        .recorded_requests
        .lock()
        .expect("tracking lock should not be poisoned");

    let request = recorded_requests.first().expect("model request should be recorded");

    assert!(request.prompt.contains("MCP prompt `system_prompt`"));
    assert!(request.prompt.contains("Follow project conventions."));
    assert!(request.prompt.contains("MCP resource `project_readme`"));
    assert!(request.prompt.contains("# Project README"));
    assert!(request.prompt.contains("Rename the user"));
}

#[tokio::test]
async fn fixture_exposes_root_and_agent_level_max_calls_configuration() {
    let server = TestMcpHttpServer::spawn([]);
    let workflow_source = fixture_with_mcp_endpoint(fixtures::TOOL_MAX_CALLS_SCOPES, &server.endpoint());
    let model_provider = TrackingModelProvider::new(vec![json!({ "value": "first" }), json!({ "value": "second" })]);
    let service = ExecutorService::new(model_provider.clone());

    service
        .execute(request_with_input(&workflow_source, Value::Null))
        .await
        .expect("execution should prepare tool definitions for both agents");

    let recorded_requests = model_provider
        .recorded_requests
        .lock()
        .expect("tracking lock should not be poisoned")
        .clone();

    assert_eq!(recorded_requests.len(), 2);

    for model_request in recorded_requests {
        let tool_definition = model_request.tools.first().expect("tool definition should exist for each agent");

        assert_eq!(tool_definition.max_calls, Some(1));
        assert_eq!(
            tool_definition.max_calls_scope,
            ToolCallLimitScope::Agent {
                agent_name: model_request.agent_name.clone(),
            }
        );
    }
}

#[tokio::test]
async fn explicit_mcp_resource_and_prompt_calls_are_available_as_values() {
    let server = TestMcpHttpServer::spawn([]);
    let workflow_source = workflow_source! {
        mcp local {
            endpoint: "__ENDPOINT__"
        }

        input {
            workspace_id: string
        }

        resource project_readme from mcp.local.resource.project_readme {
            bindings {
                workspace_id: input.workspace_id
            }
        }

        prompt system_prompt from mcp.local.prompt.system_prompt

        dynamic {
            readme: read resource.project_readme {
                bindings {
                    section: "setup"
                }
            }
            instructions: render prompt.system_prompt {
                bindings {
                    readme: dynamic.readme
                }
            }
        }

        output {
            readme: dynamic.readme
            instructions: dynamic.instructions
        }
    }
    .replace("__ENDPOINT__", &server.endpoint());
    let model_provider = TrackingModelProvider::new(Vec::new());
    let service = ExecutorService::new(model_provider);

    let output = service
        .execute(request_with_input(&workflow_source, json!({ "workspace_id": "workspace-1" })))
        .await
        .expect("explicit MCP calls should execute successfully")
        .output;

    assert!(output["readme"].as_str().is_some_and(|readme| readme.contains("# Project README")));
    assert!(output["instructions"]
        .as_str()
        .is_some_and(|instructions| instructions.contains("Follow project conventions.")));
}

#[tokio::test]
async fn mcp_read_resource_fixture_executes() {
    let server = TestMcpHttpServer::spawn([]);
    let workflow_source = fixture_with_mcp_endpoint(fixtures::MCP_READ_RESOURCE, &server.endpoint());
    let model_provider = TrackingModelProvider::new(Vec::new());
    let service = ExecutorService::new(model_provider);

    let output = service
        .execute(request_with_input(&workflow_source, json!({ "workspace_id": "workspace-1" })))
        .await
        .expect("MCP read resource fixture should execute successfully")
        .output;

    assert!(output["readme"].as_str().is_some_and(|readme| readme.contains("# Project README")));
}

#[tokio::test]
async fn mcp_render_prompt_fixture_executes() {
    let server = TestMcpHttpServer::spawn([]);
    let workflow_source = fixture_with_mcp_endpoint(fixtures::MCP_RENDER_PROMPT, &server.endpoint());
    let model_provider = TrackingModelProvider::new(Vec::new());
    let service = ExecutorService::new(model_provider);

    let output = service
        .execute(request_with_input(&workflow_source, json!({ "workspace_id": "workspace-1" })))
        .await
        .expect("MCP render prompt fixture should execute successfully")
        .output;

    assert!(output["instructions"]
        .as_str()
        .is_some_and(|instructions| instructions.contains("Follow project conventions.")));
}

#[tokio::test]
async fn mcp_render_prompt_executes_inside_null_fallback() {
    let server = TestMcpHttpServer::spawn([]);
    let workflow_source = workflow_source! {
        mcp local {
            endpoint: "__ENDPOINT__"
        }

        prompt system_prompt from mcp.local.prompt.system_prompt

        provider openai from openai {
            endpoint: "https://api.openai.com/v1"
            api_key: "test-api-key"
        }

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        agent analyzer {
            model: model.openai_model
            instruction: "Prompt: {{ render prompt.system_prompt ?? "" }}"

            output {
                summary: string
            }
        }

        output {
            summary: agent.analyzer.summary
        }
    }
    .replace("__ENDPOINT__", &server.endpoint());
    let model_provider = TrackingModelProvider::new(vec![json!({ "summary": "done" })]);
    let service = ExecutorService::new(model_provider.clone());

    let output = service
        .execute(request_with_input(&workflow_source, Value::Null))
        .await
        .expect("MCP prompt render inside null fallback should execute successfully")
        .output;

    let prompt = model_provider
        .recorded_prompts()
        .into_iter()
        .next()
        .expect("model prompt should be recorded");

    assert_eq!(output["summary"], "done");
    assert!(prompt.contains("Follow project conventions."));
}

#[tokio::test]
async fn mcp_read_render_dependency_fixture_executes() {
    let server = TestMcpHttpServer::spawn([]);
    let workflow_source = fixture_with_mcp_endpoint(fixtures::MCP_READ_RENDER_DEPENDENCIES, &server.endpoint());
    let model_provider = TrackingModelProvider::new(Vec::new());
    let service = ExecutorService::new(model_provider);

    let output = service
        .execute(request_with_input(&workflow_source, json!({ "workspace_id": "workspace-1" })))
        .await
        .expect("MCP read/render dependency fixture should execute successfully")
        .output;

    assert!(output["readme"].as_str().is_some_and(|readme| readme.contains("# Project README")));
    assert!(output["instructions"]
        .as_str()
        .is_some_and(|instructions| instructions.contains("Follow project conventions.")));
}

#[tokio::test]
async fn accepts_null_input_when_all_input_fields_are_consumed_by_bindings() {
    let server = TestMcpHttpServer::spawn([]);
    let workflow_source = workflow_source! {
        provider openai from openai {
            endpoint: "https://api.openai.com/v1"
            api_key: "test-api-key"
        }

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        mcp local {
            endpoint: "__ENDPOINT__"
        }

        input {
            project_id: number
            task_id: number
        }

        tool list_participants from mcp.local.tool.list_participants {
            bindings {
                project_id: input.project_id
                task_id: input.task_id
            }
        }

        agent updater {
            model: model.openai_model
            uses: [tool.list_participants]
            instruction: "List participants"
            output {
                value: string
            }
        }

        output {
            value: agent.updater
        }
    }
    .replace("__ENDPOINT__", &server.endpoint());

    let model_provider = TrackingModelProvider::new(vec![serde_json::json!({ "value": "done" })]);
    let service = ExecutorService::new(model_provider.clone());

    service
        .execute(request(&workflow_source))
        .await
        .expect("execution should accept null input when all fields are consumed by bindings");
}

#[test]
fn validation_does_not_execute_workflow_dynamic_tool_calls() {
    let server = TestMcpHttpServer::spawn([]);
    let workflow_source = workflow_source! {
        provider openai from openai {
            endpoint: "https://api.openai.com/v1"
            api_key: "test-api-key"
        }

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        mcp local {
            endpoint: "__ENDPOINT__"
        }

        input {
            project_id: number
            task_id: number
        }

        tool list_participants from mcp.local.tool.list_participants {
            bindings {
                project_id: input.project_id
                task_id: input.task_id
            }
        }

        dynamic {
            data: call tool.list_participants
        }

        agent updater {
            model: model.openai_model
            instruction: "List participants"
            output {
                value: string
            }
        }

        output {
            value: dynamic.data
        }
    }
    .replace("__ENDPOINT__", &server.endpoint());

    let service = ExecutorService::new(TrackingModelProvider::new(Vec::new()));

    service
        .validate(ValidationRequest {
            workflow_source: Some(workflow_source),
            workflow_source_base64: None,
            secrets: Value::Null,
        })
        .expect("validation should be static and succeed without input values");

    assert_eq!(server.method_count(TestMcpMethod::ToolsList), 1);
    assert_eq!(server.method_count(TestMcpMethod::ToolsCall), 0);
}

#[test]
fn validation_does_not_execute_agent_dynamic_tool_calls() {
    let server = TestMcpHttpServer::spawn([]);
    let workflow_source = workflow_source! {
        provider openai from openai {
            endpoint: "https://api.openai.com/v1"
            api_key: "test-api-key"
        }

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        mcp local {
            endpoint: "__ENDPOINT__"
        }

        input {
            project_id: number
            task_id: number
        }

        tool list_participants from mcp.local.tool.list_participants {
            bindings {
                project_id: input.project_id
                task_id: input.task_id
            }
        }

        agent updater {
            model: model.openai_model

            dynamic {
                data: call tool.list_participants
            }

            instruction: "List participants"
            output {
                value: string
            }
        }

        output {
            value: agent.updater
        }
    }
    .replace("__ENDPOINT__", &server.endpoint());

    let service = ExecutorService::new(TrackingModelProvider::new(Vec::new()));

    service
        .validate(ValidationRequest {
            workflow_source: Some(workflow_source),
            workflow_source_base64: None,
            secrets: Value::Null,
        })
        .expect("validation should not execute agent-local dynamic values");

    assert_eq!(server.method_count(TestMcpMethod::ToolsList), 1);
    assert_eq!(server.method_count(TestMcpMethod::ToolsCall), 0);
}

#[test]
fn validation_does_not_fetch_mcp_prompt_imports() {
    let server = TestMcpHttpServer::spawn([]);
    let workflow_source = workflow_source! {
        provider openai from openai {
            endpoint: "https://api.openai.com/v1"
            api_key: "test-api-key"
        }

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        mcp local {
            endpoint: "__ENDPOINT__"
        }

        input {
            project_id: number
            task_id: number
        }

        prompt system_prompt from mcp.local.prompt.system_prompt {
            bindings {
                project_id: input.project_id
                type_id: input.task_id
                type: "task"
            }
        }

        agent updater {
            model: model.openai_model
            instruction: "Summarize task"
            output {
                value: string
            }
        }

        output {
            value: agent.updater
        }
    }
    .replace("__ENDPOINT__", &server.endpoint());

    let service = ExecutorService::new(TrackingModelProvider::new(Vec::new()));

    service
        .validate(ValidationRequest {
            workflow_source: Some(workflow_source),
            workflow_source_base64: None,
            secrets: Value::Null,
        })
        .expect("validation should list prompt imports without rendering them");

    assert_eq!(server.method_count(TestMcpMethod::ToolsList), 1);
    assert_eq!(server.method_count(TestMcpMethod::PromptsList), 1);
    assert_eq!(server.method_count(TestMcpMethod::PromptsGet), 0);
}

#[test]
fn validation_does_not_read_mcp_resource_imports() {
    let server = TestMcpHttpServer::spawn([]);
    let workflow_source = workflow_source! {
        provider openai from openai {
            endpoint: "https://api.openai.com/v1"
            api_key: "test-api-key"
        }

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        mcp local {
            endpoint: "__ENDPOINT__"
        }

        input {
            workspace_id: string
        }

        resource project_readme from mcp.local.resource.project_readme {
            bindings {
                workspace_id: input.workspace_id
            }
        }

        agent updater {
            model: model.openai_model
            instruction: "Summarize project"
            output {
                value: string
            }
        }

        output {
            value: agent.updater
        }
    }
    .replace("__ENDPOINT__", &server.endpoint());

    let service = ExecutorService::new(TrackingModelProvider::new(Vec::new()));

    service
        .validate(ValidationRequest {
            workflow_source: Some(workflow_source),
            workflow_source_base64: None,
            secrets: Value::Null,
        })
        .expect("validation should list resource imports without reading them");

    assert_eq!(server.method_count(TestMcpMethod::ToolsList), 1);
    assert_eq!(server.method_count(TestMcpMethod::ResourcesList), 1);
    assert_eq!(server.method_count(TestMcpMethod::ResourcesRead), 0);
}

#[test]
fn validation_rejects_dynamic_tool_call_missing_required_input() {
    let server = TestMcpHttpServer::spawn([]);
    let workflow_source = workflow_source! {
        provider openai from openai {
            endpoint: "https://api.openai.com/v1"
            api_key: "test-api-key"
        }

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        mcp local {
            endpoint: "__ENDPOINT__"
        }

        tool list_participants from mcp.local.tool.list_participants

        dynamic {
            data: call tool.list_participants
        }

        output {
            value: dynamic.data
        }
    }
    .replace("__ENDPOINT__", &server.endpoint());

    let service = ExecutorService::new(TrackingModelProvider::new(Vec::new()));
    let error = service
        .validate(ValidationRequest {
            workflow_source: Some(workflow_source),
            workflow_source_base64: None,
            secrets: Value::Null,
        })
        .expect_err("validation should reject the missing required tool input statically");

    let error_message = error.to_string();

    assert!(
        error_message.contains("Missing `dynamic` declaration") || error_message.contains("missing required `input` field `project_id`"),
        "unexpected validation error: {error_message}"
    );

    assert_eq!(server.method_count(TestMcpMethod::ToolsList), 1);
    assert_eq!(server.method_count(TestMcpMethod::ToolsCall), 0);
}

#[test]
fn validation_accepts_dynamic_tool_call_missing_nullable_input() {
    let server = TestMcpHttpServer::spawn([]);
    let workflow_source = workflow_source! {
        provider openai from openai {
            endpoint: "https://api.openai.com/v1"
            api_key: "test-api-key"
        }

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        mcp local {
            endpoint: "__ENDPOINT__"
        }

        tool fetch_answers from mcp.local.tool.fetch_qualitative_question_answers

        dynamic {
            data: call tool.fetch_answers {
                input {
                    project_id: 31
                    task_types: ["open_written"]
                }
            }
        }

        output {
            value: dynamic.data
        }
    }
    .replace("__ENDPOINT__", &server.endpoint());

    let service = ExecutorService::new(TrackingModelProvider::new(Vec::new()));

    service
        .validate(ValidationRequest {
            workflow_source: Some(workflow_source),
            workflow_source_base64: None,
            secrets: Value::Null,
        })
        .expect("validation should accept omitted nullable MCP tool input");

    assert_eq!(server.method_count(TestMcpMethod::ToolsList), 1);
    assert_eq!(server.method_count(TestMcpMethod::ToolsCall), 0);
}

#[tokio::test]
async fn mcp_tool_call_projects_result_to_declared_output_schema() {
    let server = TestMcpHttpServer::spawn([]);
    let workflow_source = workflow_source! {
        provider openai from openai {
            endpoint: "https://api.openai.com/v1"
            api_key: "test-api-key"
        }

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        mcp local {
            endpoint: "__ENDPOINT__"
        }

        input {
            project_id: number
        }

        tool fetch_answers from mcp.local.tool.fetch_qualitative_question_answers {
            bindings {
                task_types: ["open_written"]
            }

            output {
                answers: [{
                    answer: variant task_type {
                        open_written {
                            text: string
                        }
                    }
                    participant_id: number
                    participant_name: string
                    task_id: number
                    task_title: string
                    task_type: string
                }]
            }
        }

        dynamic {
            data: call tool.fetch_answers {
                input {
                    project_id: input.project_id
                }
            }
        }

        output {
            greeting: dynamic.data
        }
    }
    .replace("__ENDPOINT__", &server.endpoint());

    let service = ExecutorService::new(TrackingModelProvider::new(Vec::new()));
    let output = service
        .execute(request_with_input(&workflow_source, json!({ "project_id": 31 })))
        .await
        .expect("execution should validate projected MCP output against declared output schema")
        .output;

    assert_eq!(
        output,
        json!({
            "greeting": {
                "answers": [
                    {
                        "answer": {
                            "task_type": "open_written",
                            "text": "hello world"
                        },
                        "participant_id": 1,
                        "participant_name": "jon",
                        "task_id": 1,
                        "task_title": "Example",
                        "task_type": "open_written"
                    }
                ]
            }
        })
    );
    assert_eq!(server.method_count(TestMcpMethod::ToolsCall), 1);
}

#[tokio::test]
async fn mcp_batch_import_alias_to_same_tool_executes_each_local_tool_with_own_bindings() {
    let server = TestMcpHttpServer::spawn([]);
    let workflow_source = workflow_source! {
        provider openai from openai {
            endpoint: "https://api.openai.com/v1"
            api_key: "test-api-key"
        }

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        mcp local {
            endpoint: "__ENDPOINT__"
        }

        from mcp.local {
            bindings {
                project_id: 14
            }

            tool fetch_qualitative_question_answers {
                bindings {
                    task_types: ["video_recording", "open_written"]
                }
            }

            tool fetch_qualitative_question_answers as video_recording_answers {
                bindings {
                    task_types: ["video_recording"]
                }
            }
        }

        dynamic {
            all_qualitative_questions: call tool.fetch_qualitative_question_answers
            video_recording_answers: call tool.video_recording_answers
        }

        output {
            test: dynamic.video_recording_answers
        }
    }
    .replace("__ENDPOINT__", &server.endpoint());

    let service = ExecutorService::new(TrackingModelProvider::new(Vec::new()));
    let mut receiver = service.execute_stream(request(&workflow_source));
    let mut started_calls = Vec::new();

    while let Some(event) = receiver.recv().await {
        if event.kind == ExecutorEventKind::McpCallStarted {
            started_calls.push(event.data.expect("MCP call started event should have data"));
        }
    }

    let tool_calls = started_calls
        .iter()
        .filter(|event_data| event_data["item_name"] == "fetch_qualitative_question_answers")
        .collect::<Vec<_>>();

    assert_eq!(tool_calls.len(), 2);
    assert!(tool_calls.iter().any(|event_data| {
        event_data["target_name"] == "fetch_qualitative_question_answers"
            && event_data["params"]["project_id"] == 14
            && event_data["params"]["task_types"] == json!(["video_recording", "open_written"])
    }));
    assert!(tool_calls.iter().any(|event_data| {
        event_data["target_name"] == "video_recording_answers"
            && event_data["params"]["project_id"] == 14
            && event_data["params"]["task_types"] == json!(["video_recording"])
    }));
}

#[tokio::test]
async fn mcp_endpoint_from_secrets_applies_omitted_tool_schema_before_model_request() {
    let server = TestMcpHttpServer::spawn([("authorization".to_string(), "Bearer secret-token".to_string())]);
    let workflow_source = workflow_source! {
        provider openai from openai {
            endpoint: "https://api.openai.com/v1"
            api_key: "test-api-key"
        }

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        secrets {
            mcp_endpoint: string
            mcp_token: string
        }

        mcp local {
            endpoint: secrets.mcp_endpoint
            headers {
                Authorization: secrets.mcp_token
            }
        }

        input {
            user_id: number
        }

        tool local_update_user from mcp.local.tool.update_user_name {
            bindings {
                user_id: input.user_id
            }
        }

        agent updater {
            model: model.openai_model
            uses: [tool.local_update_user]
            instruction: "Rename the user"
            output {
                value: string
            }
        }

        output {
            value: agent.updater
        }
    };

    let secrets = json!({
        "mcp_endpoint": server.endpoint(),
        "mcp_token": "Bearer secret-token",
    });

    let model_provider = TrackingModelProvider::new(vec![json!({ "value": "done" })]);
    let service = ExecutorService::new(model_provider.clone());
    let mut request = request_with_input(workflow_source, json!({ "user_id": 123 }));

    request.secrets = secrets;

    service
        .execute(request)
        .await
        .expect("execution should resolve MCP endpoint from secrets and apply tool schemas");

    let recorded_requests = model_provider
        .recorded_requests
        .lock()
        .expect("tracking lock should not be poisoned");
    let request = recorded_requests.first().expect("model request should be recorded");
    let tool_definition = request.tools.first().expect("tool definition should be present");

    assert_eq!(tool_definition.bindings, json!({ "user_id": 123 }));
    let input_schema = tool_definition.input_schema.json_value();
    let output_schema = tool_definition.output_schema.json_value();

    assert_eq!(input_schema["required"], json!(["user_name"]));
    assert_eq!(input_schema.pointer("/properties/user_id"), None);
    assert_eq!(input_schema.pointer("/properties/user_name/type"), Some(&json!("string")));
    assert_eq!(input_schema.pointer("/properties/user_name/enum"), Some(&json!(["Ada", "Grace"])));
    assert_eq!(output_schema["required"], json!(["success"]));
}

#[tokio::test]
async fn mcp_nullable_array_input_schema_is_preserved_for_model_validation() {
    let server = TestMcpHttpServer::spawn([]);
    let workflow_source = workflow_source! {
        provider openai from openai {
            endpoint: "https://api.openai.com/v1"
            api_key: "test-api-key"
        }

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        mcp local {
            endpoint: "__ENDPOINT__"
        }

        input {
            project_id: number
        }

        tool edit_project from mcp.local.tool.edit_project_for_workspace {
            bindings {
                project_id: input.project_id
            }
        }

        agent project_editor {
            model: model.openai_model
            uses: [tool.edit_project]
            instruction: "Edit project"
            output {
                value: string
            }
        }

        output {
            value: agent.project_editor
        }
    }
    .replace("__ENDPOINT__", &server.endpoint());

    let model_provider = TrackingModelProvider::new(vec![json!({ "value": "done" })]);
    let service = ExecutorService::new(model_provider.clone());

    service
        .execute(request_with_input(&workflow_source, json!({ "project_id": 31 })))
        .await
        .expect("execution should preserve nullable array schema for model validation");

    let recorded_requests = model_provider
        .recorded_requests
        .lock()
        .expect("tracking lock should not be poisoned");
    let request = recorded_requests.first().expect("model request should be recorded");
    let tool_definition = request.tools.first().expect("tool definition should be present");
    let input_schema = tool_definition.input_schema.json_value();

    assert_eq!(input_schema.pointer("/properties/name/oneOf/0/type"), Some(&json!("array")));
    assert_eq!(input_schema.pointer("/properties/name/oneOf/1/type"), Some(&json!("null")));
    assert_eq!(input_schema.pointer("/properties/languages/oneOf/0/type"), Some(&json!("array")));
    assert_eq!(input_schema.pointer("/properties/languages/oneOf/1/type"), Some(&json!("null")));
}

#[tokio::test]
async fn mcp_tool_batch_imports_apply_shared_bindings_to_all_tools() {
    let server = TestMcpHttpServer::spawn([]);
    let workflow_source = fixture_with_mcp_endpoint(fixtures::MCP_TOOL_BATCH_IMPORTS, &server.endpoint());
    let model_provider = TrackingModelProvider::new(vec![json!({ "value": "done" })]);
    let service = ExecutorService::new(model_provider.clone());

    service
        .execute(request_with_input(&workflow_source, json!({ "project_id": 31, "task_id": 42 })))
        .await
        .expect("execution should expand MCP tool batch imports before agent execution");

    let recorded_requests = model_provider
        .recorded_requests
        .lock()
        .expect("tracking lock should not be poisoned");
    let request = recorded_requests.first().expect("model request should be recorded");

    let workflow_tools = request
        .tools
        .iter()
        .filter(|tool_definition| tool_definition.name != "finalize")
        .collect::<Vec<_>>();

    assert_eq!(workflow_tools.len(), 3);

    let create_sorting_task = request
        .tools
        .iter()
        .find(|tool_definition| tool_definition.name == "create_sorting_task")
        .expect("aliased create tool should be imported");
    let update_task_status = request
        .tools
        .iter()
        .find(|tool_definition| tool_definition.name == "update_task_status")
        .expect("aliased update tool should be imported");
    let assign_task = request
        .tools
        .iter()
        .find(|tool_definition| tool_definition.name == "assign_task")
        .expect("non-aliased tool should infer local name");

    assert_eq!(create_sorting_task.bindings, json!({ "project_id": 31, "task_id": 42 }));
    assert_eq!(update_task_status.bindings, json!({ "project_id": 31, "task_id": 42 }));
    assert_eq!(assign_task.bindings, json!({ "project_id": 31, "task_id": 42 }));

    let create_sorting_task_input_schema = create_sorting_task.input_schema.json_value();
    let update_task_status_input_schema = update_task_status.input_schema.json_value();
    let assign_task_input_schema = assign_task.input_schema.json_value();

    assert_eq!(create_sorting_task_input_schema["required"], json!(["title"]));
    assert_eq!(update_task_status_input_schema["required"], json!(["status"]));
    assert_eq!(assign_task_input_schema["required"], json!(["user_id"]));
    assert_eq!(create_sorting_task_input_schema.pointer("/properties/project_id"), None);
    assert_eq!(update_task_status_input_schema.pointer("/properties/task_id"), None);

    assert_eq!(
        create_sorting_task.source,
        ModelToolSource::Mcp {
            server_name: Some("local".to_string()),
            tool_name: "create-sorting-task".to_string(),
            endpoint: server.endpoint(),
            headers: BTreeMap::new(),
        }
    );
    assert_eq!(
        assign_task.source,
        ModelToolSource::Mcp {
            server_name: Some("local".to_string()),
            tool_name: "assign-task".to_string(),
            endpoint: server.endpoint(),
            headers: BTreeMap::new(),
        }
    );
}
