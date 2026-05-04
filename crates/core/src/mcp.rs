use crate::dsl::{
    Declaration, Expression, McpServerDeclaration, McpServerPropertyName, SourcePosition, SourceSpan, ToolSource, TypeExpression,
    TypedField, Workflow,
};
use crate::semantic::support::expression::{evaluate_expression, EvaluationContext};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct McpLock {
    pub servers: BTreeMap<String, McpServerLock>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct McpServerLock {
    pub tools: BTreeMap<String, McpToolLock>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpToolLock {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,
    pub output_schema: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServerConfig {
    pub name: String,
    pub endpoint: String,
    pub headers: BTreeMap<String, String>,
}

#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("MCP declaration `{server_name}` requires string property `endpoint`")]
    MissingEndpoint { server_name: String },

    #[error("MCP declaration `{server_name}` property `{property_name}` must be {expected}")]
    InvalidProperty {
        server_name: String,
        property_name: String,
        expected: &'static str,
    },

    #[error("MCP server `{server_name}` HTTP request for `{method}` failed: {message}")]
    Http {
        server_name: String,
        method: String,
        message: String,
    },

    #[error("MCP server `{server_name}` returned an error for `{method}`: {message}")]
    Rpc {
        server_name: String,
        method: String,
        message: String,
    },

    #[error("MCP server `{server_name}` response for `{method}` did not include a result")]
    MissingResult { server_name: String, method: String },

    #[error("failed to read MCP lock `{path}`: {source}")]
    ReadLock { path: String, source: std::io::Error },

    #[error("failed to parse MCP lock `{path}`: {source}")]
    ParseLock { path: String, source: serde_json::Error },

    #[error("failed to write MCP lock `{path}`: {source}")]
    WriteLock { path: String, source: std::io::Error },

    #[error("failed to serialize MCP lock `{path}`: {source}")]
    SerializeLock { path: String, source: serde_json::Error },
}

impl McpLock {
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn discover_from_workflow(workflow: &Workflow) -> Result<Self, McpError> {
        let mut lock = Self::empty();

        for server_config in McpServerConfig::from_workflow(workflow)? {
            log::debug!("discovering MCP tools from literal server config: {}", server_config.name);
            let server_lock = McpClient::new(server_config.clone()).list_tools()?;
            lock.servers.insert(server_config.name, server_lock);
        }

        Ok(lock)
    }

    pub fn discover_from_workflow_with_context(workflow: &Workflow, evaluation_context: &EvaluationContext) -> Result<Self, McpError> {
        let mut lock = Self::empty();

        for declaration in workflow.declarations() {
            let Declaration::McpServer(mcp_server_declaration) = declaration else {
                continue;
            };
            let server_config = McpServerConfig::resolve_from_declaration(mcp_server_declaration, evaluation_context)?;
            log::debug!("discovering MCP tools from runtime server config: {}", server_config.name);
            let server_lock = McpClient::new(server_config.clone()).list_tools()?;

            lock.servers.insert(server_config.name, server_lock);
        }

        Ok(lock)
    }

    pub fn read_from_path(lock_path: &Path) -> Result<Self, McpError> {
        let lock_text = std::fs::read_to_string(lock_path).map_err(|source| McpError::ReadLock {
            path: lock_path.display().to_string(),
            source,
        })?;

        serde_json::from_str(&lock_text).map_err(|source| McpError::ParseLock {
            path: lock_path.display().to_string(),
            source,
        })
    }

    pub fn write_to_path(&self, lock_path: &Path) -> Result<(), McpError> {
        let lock_text = serde_json::to_string_pretty(self).map_err(|source| McpError::SerializeLock {
            path: lock_path.display().to_string(),
            source,
        })?;

        std::fs::write(lock_path, format!("{lock_text}\n")).map_err(|source| McpError::WriteLock {
            path: lock_path.display().to_string(),
            source,
        })
    }

    #[must_use]
    pub fn find_tool(&self, source: &ToolSource) -> Option<&McpToolLock> {
        let ToolSource::Mcp(mcp_tool_source) = source;

        if let Some(server_name) = &mcp_tool_source.server_name {
            return self
                .servers
                .get(server_name)
                .and_then(|server_lock| server_lock.tools.get(&mcp_tool_source.tool_name));
        }

        self.servers
            .values()
            .find_map(|server_lock| server_lock.tools.get(&mcp_tool_source.tool_name))
    }

    pub fn apply_to_workflow(&self, workflow: &mut Workflow) {
        for declaration in &mut workflow.declarations {
            let Declaration::Tool(tool_declaration) = declaration else {
                continue;
            };
            let Some(mcp_tool) = self.find_tool_for_tool_declaration(tool_declaration) else {
                continue;
            };

            tool_declaration.apply_mcp_schema(mcp_tool);
        }
    }
}

impl crate::dsl::ToolDeclaration {
    fn apply_mcp_schema(&mut self, mcp_tool: &McpToolLock) {
        if self.description.is_none() {
            self.description.clone_from(&mcp_tool.description);
        }

        if self.input_fields.is_empty() {
            let fixed_binding_names = self
                .fixed_binding_fields
                .iter()
                .map(|fixed_binding_field| fixed_binding_field.name.as_str())
                .collect::<Vec<_>>();

            self.input_fields = typed_fields_from_json_schema_except(&mcp_tool.input_schema, &fixed_binding_names);
        }

        if self.output_fields.is_empty() {
            if let Some(output_schema) = &mcp_tool.output_schema {
                self.output_fields = typed_fields_from_json_schema(output_schema);
            }
        }
    }
}

impl McpLock {
    #[must_use]
    fn find_tool_for_tool_declaration(&self, tool_declaration: &crate::dsl::ToolDeclaration) -> Option<&McpToolLock> {
        let Some(tool_source) = &tool_declaration.source else {
            return None;
        };

        let ToolSource::Mcp(mcp_tool_source) = tool_source;

        if mcp_tool_source.server_name.is_none() {
            if let Some(server_lock) = self.servers.get(&mcp_tool_source.tool_name) {
                if let Some(mcp_tool) = server_lock.tools.get(&tool_declaration.name) {
                    return Some(mcp_tool);
                }
            }
        }

        self.find_tool(tool_source)
    }
}

impl McpServerConfig {
    pub fn from_workflow(workflow: &Workflow) -> Result<Vec<Self>, McpError> {
        Ok(workflow
            .declarations()
            .iter()
            .filter_map(|declaration| match declaration {
                Declaration::McpServer(mcp_server_declaration) => Self::from_declaration(mcp_server_declaration),
                Declaration::Provider(_)
                | Declaration::Secrets(_)
                | Declaration::Input(_)
                | Declaration::Schema(_)
                | Declaration::Tool(_)
                | Declaration::Dynamic(_)
                | Declaration::Agent(_)
                | Declaration::Output(_) => None,
            })
            .collect())
    }

    #[must_use]
    pub fn from_declaration(mcp_server_declaration: &McpServerDeclaration) -> Option<Self> {
        let server_name = mcp_server_declaration.name.clone();
        let mut endpoint = None;
        let mut headers = BTreeMap::new();

        for property in &mcp_server_declaration.properties {
            match McpServerPropertyName::from_identifier(&property.name) {
                Some(McpServerPropertyName::Endpoint) => {
                    let Expression::StringLiteral(value) = &property.value else {
                        return None;
                    };
                    endpoint = Some(value.clone());
                }
                Some(McpServerPropertyName::Headers) => {
                    headers = Self::parse_literal_headers(&property.value)?;
                }
                None => {}
            }
        }

        let endpoint = endpoint?;

        Some(Self {
            name: server_name,
            endpoint,
            headers,
        })
    }

    pub fn resolve_from_declaration(
        mcp_server_declaration: &McpServerDeclaration,
        evaluation_context: &EvaluationContext,
    ) -> Result<Self, McpError> {
        let server_name = mcp_server_declaration.name.clone();
        let mut endpoint = None;
        let mut headers = BTreeMap::new();

        for property in &mcp_server_declaration.properties {
            match McpServerPropertyName::from_identifier(&property.name) {
                Some(McpServerPropertyName::Endpoint) => {
                    let value = evaluate_expression(
                        &property.value,
                        evaluation_context,
                        &format!("MCP server `{server_name}` property `endpoint`"),
                    )
                    .map_err(|_error| McpError::InvalidProperty {
                        server_name: server_name.clone(),
                        property_name: "endpoint".to_string(),
                        expected: "a string or reference that resolves to a string",
                    })?;
                    let string_value = value.as_str().ok_or_else(|| McpError::InvalidProperty {
                        server_name: server_name.clone(),
                        property_name: "endpoint".to_string(),
                        expected: "a string value",
                    })?;
                    endpoint = Some(string_value.to_string());
                }
                Some(McpServerPropertyName::Headers) => {
                    headers = Self::resolve_headers(&property.value, &server_name, evaluation_context)?;
                }
                None => {}
            }
        }

        let endpoint = endpoint.ok_or_else(|| McpError::MissingEndpoint {
            server_name: server_name.clone(),
        })?;

        Ok(Self {
            name: server_name,
            endpoint,
            headers,
        })
    }

    fn parse_literal_headers(expression: &Expression) -> Option<BTreeMap<String, String>> {
        let Expression::ObjectLiteral(header_fields) = expression else {
            return None;
        };

        let mut headers = BTreeMap::new();

        for header_field in header_fields {
            let Expression::StringLiteral(value) = &header_field.value else {
                return None;
            };
            headers.insert(header_field.name.clone(), value.clone());
        }

        Some(headers)
    }

    fn resolve_headers(
        expression: &Expression,
        server_name: &str,
        evaluation_context: &EvaluationContext,
    ) -> Result<BTreeMap<String, String>, McpError> {
        let Expression::ObjectLiteral(header_fields) = expression else {
            return Err(McpError::InvalidProperty {
                server_name: server_name.to_string(),
                property_name: "headers".to_string(),
                expected: "an object with string values",
            });
        };

        let mut headers = BTreeMap::new();

        for header_field in header_fields {
            let value = evaluate_expression(
                &header_field.value,
                evaluation_context,
                &format!("MCP server `{server_name}` header `{}`", header_field.name),
            )
            .map_err(|_error| McpError::InvalidProperty {
                server_name: server_name.to_string(),
                property_name: format!("headers.{}", header_field.name),
                expected: "a string or reference that resolves to a string",
            })?;
            let string_value = value.as_str().ok_or_else(|| McpError::InvalidProperty {
                server_name: server_name.to_string(),
                property_name: format!("headers.{}", header_field.name),
                expected: "a string value",
            })?;
            headers.insert(header_field.name.clone(), string_value.to_string());
        }

        Ok(headers)
    }
}

#[derive(Debug)]
pub struct McpClient {
    server_config: McpServerConfig,
}

impl McpClient {
    #[must_use]
    pub fn new(server_config: McpServerConfig) -> Self {
        Self { server_config }
    }

    pub fn list_tools(&self) -> Result<McpServerLock, McpError> {
        log::debug!("initializing MCP tools/list: server={}", self.server_config.name);
        self.request(
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "superwire",
                    "version": env!("CARGO_PKG_VERSION"),
                },
            }),
            1,
        )?;
        self.notify("notifications/initialized", json!({}))?;
        let result = self.request("tools/list", json!({}), 2)?;
        let server_lock = McpServerLock::from_tools_list_result(&result);

        log::info!(
            "MCP tools/list completed: server={}, tools={}",
            self.server_config.name,
            server_lock.tools.len()
        );

        Ok(server_lock)
    }

    pub fn call_tool(&self, tool_name: &str, arguments: Value) -> Result<Value, McpError> {
        log::debug!("initializing MCP tools/call: server={}, tool={tool_name}", self.server_config.name);
        self.request(
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "superwire",
                    "version": env!("CARGO_PKG_VERSION"),
                },
            }),
            1,
        )?;
        self.notify("notifications/initialized", json!({}))?;

        let result = self.request(
            "tools/call",
            json!({
                "name": tool_name,
                "arguments": arguments,
            }),
            2,
        )?;

        log::info!("MCP tools/call completed: server={}, tool={tool_name}", self.server_config.name);

        Ok(result)
    }

    fn notify(&self, method: &str, params: Value) -> Result<(), McpError> {
        let notification = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        let mut request = ureq::post(&self.server_config.endpoint).header("content-type", "application/json");

        for (header_name, header_value) in &self.server_config.headers {
            request = request.header(header_name, header_value);
        }

        request.send_json(&notification).map_err(|error| McpError::Http {
            server_name: self.server_config.name.clone(),
            method: method.to_string(),
            message: error.to_string(),
        })?;

        Ok(())
    }

    fn request(&self, method: &str, params: Value, request_id: u64) -> Result<Value, McpError> {
        let response = self.post(
            method,
            json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "method": method,
                "params": params,
            }),
        )?;

        if let Some(error) = response.get("error") {
            return Err(McpError::Rpc {
                server_name: self.server_config.name.clone(),
                method: method.to_string(),
                message: error.to_string(),
            });
        }

        response.get("result").cloned().ok_or_else(|| McpError::MissingResult {
            server_name: self.server_config.name.clone(),
            method: method.to_string(),
        })
    }

    fn post(&self, method: &str, body: Value) -> Result<Value, McpError> {
        let mut request = ureq::post(&self.server_config.endpoint).header("content-type", "application/json");

        for (header_name, header_value) in &self.server_config.headers {
            request = request.header(header_name, header_value);
        }

        let mut response = request.send_json(&body).map_err(|error| McpError::Http {
            server_name: self.server_config.name.clone(),
            method: method.to_string(),
            message: error.to_string(),
        })?;

        response.body_mut().read_json::<Value>().map_err(|error| McpError::Http {
            server_name: self.server_config.name.clone(),
            method: method.to_string(),
            message: error.to_string(),
        })
    }
}

impl McpServerLock {
    #[must_use]
    fn from_tools_list_result(result: &Value) -> Self {
        let mut server_lock = Self::default();
        let Some(tools) = result.get("tools").and_then(Value::as_array) else {
            return server_lock;
        };

        for tool in tools {
            let Some(tool_name) = tool.get("name").and_then(Value::as_str) else {
                continue;
            };
            let input_schema = tool
                .get("inputSchema")
                .or_else(|| tool.get("input_schema"))
                .cloned()
                .unwrap_or_else(|| json!({ "type": "object", "properties": {}, "required": [] }));
            let output_schema = tool.get("outputSchema").or_else(|| tool.get("output_schema")).cloned();

            server_lock.tools.insert(
                tool_name.to_string(),
                McpToolLock {
                    name: tool_name.to_string(),
                    description: tool.get("description").and_then(Value::as_str).map(str::to_string),
                    input_schema,
                    output_schema,
                },
            );
        }

        server_lock
    }
}

fn typed_fields_from_json_schema(schema: &Value) -> Vec<TypedField> {
    typed_fields_from_json_schema_except(schema, &[])
}

fn typed_fields_from_json_schema_except(schema: &Value, excluded_field_names: &[&str]) -> Vec<TypedField> {
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        return Vec::new();
    };
    let required_fields = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|required| required.iter().filter_map(Value::as_str).collect::<Vec<_>>())
        .unwrap_or_default();
    let include_all_fields = required_fields.is_empty();
    let mut typed_fields = Vec::new();

    for (field_name, field_schema) in properties {
        if excluded_field_names.contains(&field_name.as_str()) {
            continue;
        }

        if !include_all_fields && !required_fields.contains(&field_name.as_str()) {
            continue;
        }

        typed_fields.push(TypedField {
            name: field_name.clone(),
            field_type: type_expression_from_json_schema(field_schema),
            description: field_schema.get("description").and_then(Value::as_str).map(str::to_string),
            span: generated_span(),
        });
    }

    typed_fields
}

fn type_expression_from_json_schema(schema: &Value) -> TypeExpression {
    if let Some(enum_values) = schema.get("enum").and_then(Value::as_array) {
        let mut string_enum_values = enum_values
            .iter()
            .filter_map(Value::as_str)
            .map(|enum_value| TypeExpression::StringEnum(enum_value.to_string()))
            .collect::<Vec<_>>();

        if string_enum_values.len() == 1 {
            return string_enum_values.remove(0);
        }

        if !string_enum_values.is_empty() {
            return TypeExpression::Union(string_enum_values);
        }
    }

    if let Some(one_of) = schema.get("oneOf").and_then(Value::as_array) {
        return TypeExpression::Union(one_of.iter().map(type_expression_from_json_schema).collect());
    }

    if let Some(any_of) = schema.get("anyOf").and_then(Value::as_array) {
        return TypeExpression::Union(any_of.iter().map(type_expression_from_json_schema).collect());
    }

    match schema.get("type").and_then(Value::as_str) {
        Some("string") => TypeExpression::String,
        Some("integer" | "number") => TypeExpression::Number,
        Some("boolean") => TypeExpression::Boolean,
        Some("null") => TypeExpression::Null,
        Some("array") => TypeExpression::Array {
            item_type: Box::new(schema.get("items").map_or(TypeExpression::String, type_expression_from_json_schema)),
            fixed_length: None,
        },
        Some("object") => TypeExpression::Object(typed_fields_from_json_schema(schema)),
        _ => TypeExpression::String,
    }
}

fn generated_span() -> SourceSpan {
    SourceSpan {
        start: SourcePosition { line: 1, column: 1 },
        end: SourcePosition { line: 1, column: 1 },
    }
}

impl Display for McpServerConfig {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{} ({})", self.name, self.endpoint)
    }
}

#[cfg(test)]
mod tests {
    use super::McpLock;
    use crate::dsl::{parse_workflow, validate_workflow, ToolSource};
    use crate::workflow_source;
    use serde_json::{json, Value};
    use std::collections::BTreeMap;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    #[test]
    fn discovers_mcp_tools_and_applies_missing_tool_schema() {
        let server = TestMcpHttpServer::spawn([
            ("accept".to_string(), "application/json".to_string()),
            ("x-api-key".to_string(), "test-api-key".to_string()),
        ]);
        let workflow_source = workflow_source! {
            mcp local {
                endpoint: "__ENDPOINT__"
                headers: {
                    Accept: "application/json"
                    "X-API-Key": "test-api-key"
                }
            }

            tool update_user_name {
                using: mcp.local.update-user-name
            }
        }
        .replace("__ENDPOINT__", &server.endpoint());
        let mut workflow = parse_workflow(&workflow_source).expect("workflow should parse");
        let mcp_lock = McpLock::discover_from_workflow(&workflow).expect("MCP discovery should succeed");

        mcp_lock.apply_to_workflow(&mut workflow);

        let tool_declaration = workflow.find_tool("update_user_name").expect("tool declaration should exist");

        assert!(matches!(
            &tool_declaration.source,
            Some(ToolSource::Mcp(mcp_tool_source))
                if mcp_tool_source.server_name.as_deref() == Some("local")
                    && mcp_tool_source.tool_name == "update-user-name"
        ));
        assert_eq!(tool_declaration.input_fields[0].name, "user_name");
        assert_eq!(tool_declaration.output_fields[0].name, "success");
    }

    #[test]
    fn applies_mcp_tool_schema_from_server_only_source() {
        let server = TestMcpHttpServer::spawn([]);
        let workflow_source = workflow_source! {
            mcp local {
                endpoint: "__ENDPOINT__"
            }

            tool list_all_participants_who_has_answered_given_task {
                using: mcp.local
            }
        }
        .replace("__ENDPOINT__", &server.endpoint());
        let mut workflow = parse_workflow(&workflow_source).expect("workflow should parse");
        let mcp_lock = McpLock::discover_from_workflow(&workflow).expect("MCP discovery should succeed");

        mcp_lock.apply_to_workflow(&mut workflow);

        let tool_declaration = workflow
            .find_tool("list_all_participants_who_has_answered_given_task")
            .expect("tool declaration should exist");

        assert_eq!(tool_declaration.input_fields[0].name, "project_id");
        assert_eq!(tool_declaration.input_fields[1].name, "task_id");
        assert_eq!(tool_declaration.output_fields[0].name, "participants");
    }

    #[test]
    fn omits_fixed_binding_fields_from_applied_mcp_input_schema() {
        let server = TestMcpHttpServer::spawn([]);
        let workflow_source = workflow_source! {
            mcp local {
                endpoint: "__ENDPOINT__"
            }

            input {
                project_id: number
                task_id: number
            }

            tool list_all_participants_who_has_answered_given_task {
                using: mcp.local.list_all_participants_who_has_answered_given_task

                bindings {
                    project_id: input.project_id
                    task_id: input.task_id
                }
            }

            dynamic {
                data: call tool.list_all_participants_who_has_answered_given_task
            }

            agent participant_answer_analyzer for participant in dynamic.data.participants {
                model: openai("gpt-4.1-mini")
                prompt: "Summarize {{ participant.id }}"
                output: string
            }

            provider openai {
                driver: "openai"
                endpoint: "https://api.openai.com/v1"
                api_key: "test-api-key"
                models: ["gpt-4.1-mini"]
            }

            output {
                value: agent.participant_answer_analyzer
            }
        }
        .replace("__ENDPOINT__", &server.endpoint());
        let mut workflow = parse_workflow(&workflow_source).expect("workflow should parse");
        let mcp_lock = McpLock::discover_from_workflow(&workflow).expect("MCP discovery should succeed");

        mcp_lock.apply_to_workflow(&mut workflow);

        let tool_declaration = workflow
            .find_tool("list_all_participants_who_has_answered_given_task")
            .expect("tool declaration should exist");
        assert!(tool_declaration.input_fields.is_empty());
        assert_eq!(tool_declaration.fixed_binding_fields.len(), 2);

        let validation_report = validate_workflow(&workflow);

        assert!(validation_report.is_valid(), "unexpected validation issues: {validation_report:?}");
    }

    #[test]
    fn resolves_mcp_endpoint_from_secret_reference() {
        let _server = TestMcpHttpServer::spawn([]);
        let workflow_source = workflow_source! {
            secrets {
                mcp_endpoint: string
            }

            mcp local {
                endpoint: secrets.mcp_endpoint
                headers: {
                    Accept: "application/json"
                }
            }

            tool update_user_name {
                using: mcp.local.update-user-name
            }
        };
        let mut workflow = parse_workflow(workflow_source).expect("workflow should parse");
        let mcp_lock = McpLock::discover_from_workflow(&workflow).expect("MCP discovery should succeed");

        mcp_lock.apply_to_workflow(&mut workflow);

        let tool_declaration = workflow.find_tool("update_user_name").expect("tool declaration should exist");

        assert!(matches!(
            &tool_declaration.source,
            Some(ToolSource::Mcp(mcp_tool_source))
                if mcp_tool_source.server_name.as_deref() == Some("local")
                    && mcp_tool_source.tool_name == "update-user-name"
        ));
    }

    #[test]
    fn resolves_mcp_endpoint_from_input_reference() {
        let _server = TestMcpHttpServer::spawn([]);
        let workflow_source = workflow_source! {
            input {
                mcp_url: string
            }

            mcp local {
                endpoint: input.mcp_url
            }

            tool update_user_name {
                using: mcp.local.update-user-name
            }
        };
        let mut workflow = parse_workflow(workflow_source).expect("workflow should parse");
        let mcp_lock = McpLock::discover_from_workflow(&workflow).expect("MCP discovery should succeed");

        mcp_lock.apply_to_workflow(&mut workflow);

        let tool_declaration = workflow.find_tool("update_user_name").expect("tool declaration should exist");

        assert!(matches!(
            &tool_declaration.source,
            Some(ToolSource::Mcp(mcp_tool_source))
                if mcp_tool_source.server_name.as_deref() == Some("local")
        ));
    }

    struct TestMcpHttpServer {
        endpoint: String,
    }

    impl TestMcpHttpServer {
        fn spawn(expected_headers: impl IntoIterator<Item = (String, String)>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("test MCP listener should bind");
            let endpoint = format!("http://{}", listener.local_addr().expect("local address should exist"));
            let expected_headers = expected_headers.into_iter().collect::<BTreeMap<_, _>>();

            thread::spawn(move || {
                for incoming_stream in listener.incoming().take(3) {
                    let stream = incoming_stream.expect("test MCP stream should open");
                    handle_mcp_request(stream, &expected_headers);
                }
            });

            Self { endpoint }
        }

        fn endpoint(&self) -> String {
            self.endpoint.clone()
        }
    }

    fn handle_mcp_request(mut stream: TcpStream, expected_headers: &BTreeMap<String, String>) {
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
            Some("tools/list") => Some(json!({
                "jsonrpc": "2.0",
                "id": 2,
                "result": {
                    "tools": [
                        {
                            "name": "update-user-name",
                            "description": "Update a user name",
                            "inputSchema": {
                                "type": "object",
                                "properties": { "user_name": { "type": "string" } },
                                "required": ["user_name"]
                            },
                            "outputSchema": {
                                "type": "object",
                                "properties": { "success": { "type": "boolean" } },
                                "required": ["success"]
                            }
                        },
                        {
                            "name": "list_all_participants_who_has_answered_given_task",
                            "description": "List participants",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "project_id": { "type": "number" },
                                    "task_id": { "type": "number" }
                                },
                                "required": ["project_id", "task_id"]
                            },
                            "outputSchema": {
                                "type": "object",
                                "properties": {
                                    "participants": {
                                        "type": "array",
                                        "items": {
                                            "type": "object",
                                            "properties": { "id": { "type": "number" } },
                                            "required": ["id"]
                                        }
                                    }
                                },
                                "required": ["participants"]
                            }
                        }
                    ]
                }
            })),
            _ => Some(json!({ "jsonrpc": "2.0", "id": 1, "result": {} })),
        }
    }
}
