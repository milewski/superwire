use crate::event::ExecutorEvent;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use superwire_core::mcp::McpClientPool;
use superwire_core::semantic::support::provider::ProviderConfig;
use superwire_core::semantic::support::types::{WorkflowSchemaCache, WorkflowType};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct ModelRequest {
    pub agent_name: String,
    pub provider_config: ProviderConfig,
    pub model_name: String,
    pub inference: HashMap<String, Value>,
    pub prompt: String,
    pub output_schema: ModelSchema,
    pub tools: Vec<ModelToolDefinition>,
    pub event_sender: Option<mpsc::Sender<ExecutorEvent>>,
    pub mcp_pool: McpClientPool,
    pub tool_call_tracker: ToolCallTracker,
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

#[derive(Debug, Clone)]
pub struct ModelResponse {
    pub output: Value,
    pub context: Value,
}
