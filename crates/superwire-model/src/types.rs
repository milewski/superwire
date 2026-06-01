use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use superwire_mcp::McpClientPool;
use superwire_protocol::event::ExecutorEvent;
use superwire_semantic::support::provider::ProviderConfig;
use superwire_semantic::support::types::{WorkflowSchemaCache, WorkflowType};
use superwire_types::{ModelAssetKind, ModelWireApi};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct ModelRequest {
    pub agent_name: String,
    pub provider_config: ProviderConfig,
    pub model_name: String,
    pub wire_api: ModelWireApi,
    pub inference: HashMap<String, Value>,
    pub context: Option<Value>,
    pub prompt: String,
    pub prompt_content: Vec<ModelPromptContent>,
    pub file_attachments: Vec<ModelFileAttachment>,
    pub output_schema: ModelSchema,
    pub tools: Vec<ModelToolDefinition>,
    pub event_sender: Option<mpsc::Sender<ExecutorEvent>>,
    pub mcp_pool: McpClientPool,
    pub tool_call_tracker: ToolCallTracker,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelFileAttachment {
    pub name: String,
    pub content: String,
    pub purpose: String,
}

impl ModelFileAttachment {
    #[must_use]
    pub fn fingerprint_value(&self) -> Value {
        serde_json::json!({
            "name": self.name,
            "content": self.content,
            "purpose": self.purpose,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelPromptContent {
    Text(String),
    Asset(ModelAsset),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelAsset {
    pub kind: ModelAssetKind,
    pub source: ModelAssetSource,
    pub media_type: Option<String>,
    pub title: Option<String>,
    pub context: Option<String>,
    pub citations: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelAssetSource {
    Url(String),
    Base64(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelAssetValueField {
    Marker,
    Kind,
    SourceType,
    Url,
    Data,
    MediaType,
    Title,
    Context,
    Citations,
}

impl ModelAssetValueField {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Marker => "__superwire_asset",
            Self::Kind => "kind",
            Self::SourceType => "source_type",
            Self::Url => "url",
            Self::Data => "data",
            Self::MediaType => "media_type",
            Self::Title => "title",
            Self::Context => "context",
            Self::Citations => "citations",
        }
    }
}

impl ModelAsset {
    #[must_use]
    pub fn all_from_value(value: &Value) -> Option<Vec<Self>> {
        if let Some(asset) = Self::from_value(value) {
            return Some(vec![asset]);
        }

        let asset_values = value.as_array()?;
        let mut assets = Vec::with_capacity(asset_values.len());

        for asset_value in asset_values {
            assets.push(Self::from_value(asset_value)?);
        }

        Some(assets)
    }

    #[must_use]
    pub fn non_empty_all_from_value(value: &Value) -> Option<Vec<Self>> {
        let assets = Self::all_from_value(value)?;

        if assets.is_empty() {
            return None;
        }

        Some(assets)
    }

    pub fn from_value(value: &Value) -> Option<Self> {
        if value.get(ModelAssetValueField::Marker.as_str()).and_then(Value::as_bool) != Some(true) {
            return None;
        }

        let kind = value
            .get(ModelAssetValueField::Kind.as_str())
            .and_then(Value::as_str)
            .and_then(ModelAssetKind::from_identifier)?;
        let source = match value.get(ModelAssetValueField::SourceType.as_str()).and_then(Value::as_str) {
            Some("base64") => ModelAssetSource::Base64(value.get(ModelAssetValueField::Data.as_str()).and_then(Value::as_str)?.to_string()),
            Some("url") => ModelAssetSource::Url(value.get(ModelAssetValueField::Url.as_str()).and_then(Value::as_str)?.to_string()),
            _ => return None,
        };

        Some(Self {
            kind,
            source,
            media_type: value
                .get(ModelAssetValueField::MediaType.as_str())
                .and_then(Value::as_str)
                .map(str::to_string),
            title: value
                .get(ModelAssetValueField::Title.as_str())
                .and_then(Value::as_str)
                .map(str::to_string),
            context: value
                .get(ModelAssetValueField::Context.as_str())
                .and_then(Value::as_str)
                .map(str::to_string),
            citations: value.get(ModelAssetValueField::Citations.as_str()).and_then(Value::as_bool),
        })
    }

    #[must_use]
    pub fn fingerprint_value(&self) -> Value {
        serde_json::json!({
            "kind": self.kind.as_str(),
            "source": match &self.source {
                ModelAssetSource::Url(url) => serde_json::json!({ "type": "url", "url": url }),
                ModelAssetSource::Base64(data) => serde_json::json!({ "type": "base64", "data": data }),
            },
            "media_type": self.media_type,
            "title": self.title,
            "context": self.context,
            "citations": self.citations,
        })
    }
}

impl ModelPromptContent {
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text(text.into())
    }

    #[must_use]
    pub fn fingerprint_value(&self) -> Value {
        match self {
            Self::Text(text) => serde_json::json!({ "type": "text", "text": text }),
            Self::Asset(asset) => {
                let mut value = asset.fingerprint_value();
                value["type"] = Value::String("asset".to_string());

                value
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelToolDefinition {
    pub name: String,
    pub description: Option<String>,
    pub source: ModelToolSource,
    pub input_schema: ModelSchema,
    pub output_schema: ModelSchema,
    pub bindings: Value,
    pub max_calls: Option<u64>,
    pub max_calls_scope: ToolCallLimitScope,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ModelSchema {
    Workflow(WorkflowType),
    ModelToolInput { input_type: WorkflowType, bindings: Value },
    FinalizeInput { output_schema: Box<ModelSchema> },
    OpenObject,
    Json(Value),
}

#[derive(Debug, Clone)]
pub struct ModelSchemaCache {
    capacity: usize,
    schemas: HashMap<String, Value>,
    insertion_order: VecDeque<String>,
    workflow_schema_cache: WorkflowSchemaCache,
}

impl ModelSchema {
    const DEFAULT_SCHEMA_CACHE_CAPACITY: usize = 256;

    #[must_use]
    pub fn workflow(workflow_type: WorkflowType) -> Self {
        Self::Workflow(workflow_type)
    }

    #[must_use]
    pub fn model_tool_input(input_type: WorkflowType, bindings: Value) -> Self {
        Self::ModelToolInput { input_type, bindings }
    }

    #[must_use]
    pub fn json(schema: Value) -> Self {
        Self::Json(schema)
    }

    #[must_use]
    pub fn json_value(&self) -> Value {
        let mut schema_cache = ModelSchemaCache::disabled();

        self.json_value_with_cache(&mut schema_cache)
    }

    #[must_use]
    pub fn cache_fingerprint_value(&self, schema_cache: &mut ModelSchemaCache) -> Value {
        self.json_value_with_cache(schema_cache)
    }

    #[must_use]
    pub fn json_value_with_cache(&self, schema_cache: &mut ModelSchemaCache) -> Value {
        let Some(schema_cache_key) = self.schema_cache_key() else {
            return self.uncached_json_value_with_cache(schema_cache);
        };

        if let Some(schema) = schema_cache.schema(&schema_cache_key) {
            return schema.clone();
        }

        let schema = self.uncached_json_value_with_cache(schema_cache);
        schema_cache.insert(schema_cache_key, schema.clone());

        schema
    }

    pub fn json_string_with_cache(&self, schema_cache: &mut ModelSchemaCache) -> Result<String, serde_json::Error> {
        serde_json::to_string(&self.json_value_with_cache(schema_cache))
    }

    #[must_use]
    pub fn schema_type_name_with_cache(&self, schema_cache: &mut ModelSchemaCache) -> Option<String> {
        self.json_value_with_cache(schema_cache)
            .get("type")
            .and_then(Value::as_str)
            .map(str::to_string)
    }

    fn uncached_json_value_with_cache(&self, schema_cache: &mut ModelSchemaCache) -> Value {
        match self {
            Self::Workflow(workflow_type) => workflow_type.json_schema_value_with_cache(&mut schema_cache.workflow_schema_cache),
            Self::ModelToolInput { input_type, bindings } => Self::model_tool_input_json_value(input_type, bindings, schema_cache),
            Self::FinalizeInput { output_schema } => Self::finalize_input_json_value(output_schema.json_value_with_cache(schema_cache)),
            Self::OpenObject => Self::open_object_json_value(),
            Self::Json(schema) => schema.clone(),
        }
    }

    pub fn json_string(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(&self.json_value())
    }

    #[must_use]
    pub fn schema_type_name(&self) -> Option<String> {
        self.json_value().get("type").and_then(Value::as_str).map(str::to_string)
    }

    #[must_use]
    pub fn project_json_value(&self, value: &Value) -> Value {
        match self {
            Self::Workflow(workflow_type) => workflow_type.project_json_value(value),
            Self::ModelToolInput {
                input_type: _,
                bindings: _,
            }
            | Self::FinalizeInput { output_schema: _ }
            | Self::OpenObject
            | Self::Json(_) => value.clone(),
        }
    }

    fn schema_cache_key(&self) -> Option<String> {
        match self {
            Self::Workflow(workflow_type) => Some(format!("workflow:{}", workflow_type.schema_cache_key())),
            Self::ModelToolInput { input_type, bindings } => {
                let mut binding_names = bindings
                    .as_object()
                    .map(|binding_object| binding_object.keys().cloned().collect::<Vec<_>>())
                    .unwrap_or_default();
                binding_names.sort();

                Some(format!(
                    "model_tool_input:{}:{}",
                    input_type.schema_cache_key(),
                    binding_names.join(",")
                ))
            }
            Self::FinalizeInput { output_schema } => output_schema
                .schema_cache_key()
                .map(|output_schema_cache_key| format!("finalize:{output_schema_cache_key}")),
            Self::OpenObject => Some("open_object".to_string()),
            Self::Json(_) => None,
        }
    }

    fn model_tool_input_json_value(input_type: &WorkflowType, bindings: &Value, schema_cache: &mut ModelSchemaCache) -> Value {
        let mut input_schema =
            input_type.json_schema_value_with_nullable_fields_optional_with_cache(&mut schema_cache.workflow_schema_cache);
        let Some(binding_object) = bindings.as_object() else {
            return input_schema;
        };
        let binding_names = binding_object.keys().cloned().collect::<HashSet<_>>();

        if let Some(properties) = input_schema.get_mut("properties").and_then(Value::as_object_mut) {
            for binding_name in &binding_names {
                properties.remove(binding_name);
            }
        }

        let mut remove_required = false;

        if let Some(required_fields) = input_schema.get_mut("required").and_then(Value::as_array_mut) {
            required_fields.retain(|required_field| {
                required_field
                    .as_str()
                    .is_none_or(|required_field_name| !binding_names.contains(required_field_name))
            });
            remove_required = required_fields.is_empty();
        }

        if remove_required {
            if let Some(schema_object) = input_schema.as_object_mut() {
                schema_object.remove("required");
            }
        }

        input_schema
    }

    fn finalize_input_json_value(output_schema: Value) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "type": {
                    "type": "string",
                    "enum": [FinalizeCallKind::Success.as_str(), FinalizeCallKind::Fail.as_str()],
                },
                "output": output_schema,
                "reason": {
                    "type": "string",
                    "minLength": 1,
                },
            },
            "required": ["type"],
            "additionalProperties": false,
            "oneOf": [
                {
                    "properties": {
                        "type": { "const": FinalizeCallKind::Success.as_str() },
                    },
                    "required": ["type", "output"],
                    "not": { "required": ["reason"] },
                },
                {
                    "properties": {
                        "type": { "const": FinalizeCallKind::Fail.as_str() },
                    },
                    "required": ["type", "reason"],
                    "not": { "required": ["output"] },
                },
            ],
        })
    }

    fn open_object_json_value() -> Value {
        serde_json::json!({
            "type": "object",
            "additionalProperties": true,
        })
    }
}

impl Default for ModelSchemaCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelSchemaCache {
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(ModelSchema::DEFAULT_SCHEMA_CACHE_CAPACITY)
    }

    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            schemas: HashMap::new(),
            insertion_order: VecDeque::new(),
            workflow_schema_cache: WorkflowSchemaCache::with_capacity(capacity),
        }
    }

    #[must_use]
    pub fn disabled() -> Self {
        Self::with_capacity(0)
    }

    fn schema(&self, schema_cache_key: &str) -> Option<&Value> {
        self.schemas.get(schema_cache_key)
    }

    fn insert(&mut self, schema_cache_key: String, schema: Value) {
        if self.capacity == 0 {
            return;
        }

        if self.schemas.insert(schema_cache_key.clone(), schema).is_none() {
            self.insertion_order.push_back(schema_cache_key);
        }

        while self.schemas.len() > self.capacity {
            let Some(expired_schema_cache_key) = self.insertion_order.pop_front() else {
                break;
            };

            self.schemas.remove(&expired_schema_cache_key);
        }
    }
}

impl ModelToolDefinition {
    const FINALIZE_NAME: &'static str = "finalize";
    const INTERNAL_FINALIZE_DISPLAY_NAME: &'static str = "internal:finalize";

    #[must_use]
    pub fn finalize(output_schema: ModelSchema) -> Self {
        Self {
            name: Self::FINALIZE_NAME.to_string(),
            description: Some(
                "Finish the agent run. Use success with output matching the schema, or fail with a clear reason when the request cannot be fulfilled."
                    .to_string(),
            ),
            source: ModelToolSource::Finalize,
            input_schema: ModelSchema::FinalizeInput {
                output_schema: Box::new(output_schema),
            },
            output_schema: ModelSchema::workflow(WorkflowType::AnyObject),
            bindings: Value::Null,
            max_calls: None,
            max_calls_scope: ToolCallLimitScope::Workflow,
        }
    }

    #[must_use]
    pub fn event_display_name(&self) -> String {
        match &self.source {
            ModelToolSource::Finalize => Self::INTERNAL_FINALIZE_DISPLAY_NAME.to_string(),
            _ => self.name.clone(),
        }
    }

    #[must_use]
    pub fn cache_fingerprint_value(&self, schema_cache: &mut ModelSchemaCache) -> Value {
        serde_json::json!({
            "name": self.name,
            "description": self.description,
            "source": self.source.cache_fingerprint_value(),
            "input_schema": self.input_schema.cache_fingerprint_value(schema_cache),
            "output_schema": self.output_schema.cache_fingerprint_value(schema_cache),
            "bindings": self.bindings,
            "max_calls": self.max_calls,
            "max_calls_scope": self.max_calls_scope.cache_fingerprint_value(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinalizeCallKind {
    Success,
    Fail,
}

impl FinalizeCallKind {
    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "success" => Some(Self::Success),
            "fail" => Some(Self::Fail),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Fail => "fail",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCallLimitScope {
    Workflow,
    Agent { agent_name: String },
}

impl ToolCallLimitScope {
    #[must_use]
    pub fn key(&self, tool_name: &str) -> String {
        match self {
            Self::Workflow => tool_name.to_string(),
            Self::Agent { agent_name } => format!("{agent_name}::{tool_name}"),
        }
    }

    #[must_use]
    pub fn cache_fingerprint_value(&self) -> Value {
        match self {
            Self::Workflow => serde_json::json!({ "kind": "workflow" }),
            Self::Agent { agent_name } => {
                serde_json::json!({
                    "kind": "agent",
                    "agent_name": agent_name,
                })
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ToolCallTracker {
    counts_by_tool: Arc<Mutex<HashMap<String, u64>>>,
}

impl ToolCallTracker {
    pub fn register_call(&self, tool_name: &str, max_calls: Option<u64>, scope: &ToolCallLimitScope) -> Result<(), String> {
        let Some(max_calls) = max_calls else {
            return Ok(());
        };

        let mut counts_by_tool = self
            .counts_by_tool
            .lock()
            .map_err(|error| format!("failed to acquire tool call tracker lock: {error}"))?;
        let scope_key = scope.key(tool_name);
        let current_count = counts_by_tool.entry(scope_key).or_insert(0);

        if *current_count >= max_calls {
            return Err(format!("tool `{tool_name}` cannot be called more than {max_calls} times"));
        }

        *current_count += 1;

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelToolSource {
    Finalize,
    Local,
    Mcp {
        server_name: Option<String>,
        tool_name: String,
        endpoint: String,
        headers: BTreeMap<String, String>,
    },
    McpPrompt {
        server_name: String,
        prompt_name: String,
        endpoint: String,
        headers: BTreeMap<String, String>,
    },
    McpResource {
        server_name: String,
        resource_name: String,
        endpoint: String,
        headers: BTreeMap<String, String>,
    },
}

impl ModelToolSource {
    #[must_use]
    pub fn cache_fingerprint_value(&self) -> Value {
        match self {
            Self::Finalize => serde_json::json!({
                "kind": "finalize",
            }),
            Self::Local => serde_json::json!({
                "kind": "local",
            }),
            Self::Mcp {
                server_name,
                tool_name,
                endpoint,
                headers,
            } => serde_json::json!({
                "kind": "mcp_tool",
                "server_name": server_name,
                "tool_name": tool_name,
                "endpoint": endpoint,
                "headers": headers,
            }),
            Self::McpPrompt {
                server_name,
                prompt_name,
                endpoint,
                headers,
            } => serde_json::json!({
                "kind": "mcp_prompt",
                "server_name": server_name,
                "prompt_name": prompt_name,
                "endpoint": endpoint,
                "headers": headers,
            }),
            Self::McpResource {
                server_name,
                resource_name,
                endpoint,
                headers,
            } => serde_json::json!({
                "kind": "mcp_resource",
                "server_name": server_name,
                "resource_name": resource_name,
                "endpoint": endpoint,
                "headers": headers,
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModelResponse {
    pub output: Value,
    pub context: Value,
}
