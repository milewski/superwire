use crate::runtime::error::WorkflowRuntimeError;
use schemars::Schema;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::fs;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use superwire_agent::{DynamicTool, ToolDefinition, ToolError};
use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store};

mod contract {
    wasmtime::component::bindgen!({
        path: "wit",
        world: "superwire-tool",
    });
}

#[derive(Debug, Clone)]
pub struct WasmToolRuntimeLoader {
    workflow_directory: PathBuf,
}

impl WasmToolRuntimeLoader {
    #[must_use]
    pub fn from_workflow_directory(workflow_directory: impl Into<PathBuf>) -> Self {
        Self {
            workflow_directory: workflow_directory.into(),
        }
    }

    pub fn discover_runtime_tools(&self) -> Result<Vec<DynamicTool>, WorkflowRuntimeError> {
        let mut discovered_runtime_tools = Vec::new();
        let mut discovered_tool_names = HashSet::<String>::new();

        for component_path in self.wasm_component_paths()? {
            let wasm_tool_component = WasmToolComponent::from_file(component_path)?;
            let dynamic_tool = wasm_tool_component.as_dynamic_tool();
            let tool_name = dynamic_tool.tool_definition().name.clone();

            if !discovered_tool_names.insert(tool_name.clone()) {
                return Err(WorkflowRuntimeError::Other {
                    message: format!(
                        "duplicate wasm tool name `{tool_name}` discovered in `{}`",
                        self.tools_directory().display()
                    ),
                });
            }

            discovered_runtime_tools.push(dynamic_tool);
        }

        Ok(discovered_runtime_tools)
    }

    fn tools_directory(&self) -> PathBuf {
        self.workflow_directory.join("tools")
    }

    fn wasm_component_paths(&self) -> Result<Vec<PathBuf>, WorkflowRuntimeError> {
        let tools_directory = self.tools_directory();

        if !tools_directory.exists() {
            return Ok(Vec::new());
        }

        if !tools_directory.is_dir() {
            return Err(WorkflowRuntimeError::Other {
                message: format!(
                    "expected tools directory at `{}`, but found a non-directory path",
                    tools_directory.display()
                ),
            });
        }

        let mut wasm_component_paths = Vec::new();

        let directory_entries = fs::read_dir(&tools_directory).map_err(|error| WorkflowRuntimeError::Other {
            message: format!("failed to read tools directory `{}`: {error}", tools_directory.display()),
        })?;

        for directory_entry in directory_entries {
            let directory_entry = directory_entry.map_err(|error| WorkflowRuntimeError::Other {
                message: format!("failed to read directory entry in `{}`: {error}", tools_directory.display()),
            })?;

            let component_path = directory_entry.path();

            if !component_path.is_file() {
                continue;
            }

            if component_path.extension().and_then(std::ffi::OsStr::to_str) != Some("wasm") {
                continue;
            }

            wasm_component_paths.push(component_path);
        }

        wasm_component_paths.sort();

        Ok(wasm_component_paths)
    }
}

#[derive(Clone)]
struct WasmToolComponent {
    component_path: PathBuf,
    execution_engine: Arc<Engine>,
    compiled_component: Arc<Component>,
    tool_definition: ToolDefinition,
}

impl WasmToolComponent {
    fn from_file(component_path: PathBuf) -> Result<Self, WorkflowRuntimeError> {
        let execution_engine = Arc::new(Self::component_engine()?);

        let compiled_component =
            Arc::new(
                Component::from_file(&execution_engine, &component_path).map_err(|error| WorkflowRuntimeError::Other {
                    message: format!("failed to compile wasm tool component `{}`: {error}", component_path.display()),
                })?,
            );

        let tool_definition = Self::read_tool_definition(&execution_engine, &compiled_component, &component_path)?;

        Ok(Self {
            component_path,
            execution_engine,
            compiled_component,
            tool_definition,
        })
    }

    #[must_use]
    fn as_dynamic_tool(&self) -> DynamicTool {
        let tool_definition = self.tool_definition.clone();
        let wasm_tool_component = self.clone();

        DynamicTool::new_with_bound_arguments(tool_definition, move |agent_input, bound_input| {
            let wasm_tool_component = wasm_tool_component.clone();

            async move { wasm_tool_component.execute(agent_input, bound_input) }
        })
    }

    fn execute(&self, agent_input: Value, bound_input: Map<String, Value>) -> Result<Value, ToolError> {
        let mut component_instance = WasmToolComponentInstance::new(&self.execution_engine, &self.compiled_component, &self.component_path)
            .map_err(ToolError::new)?;

        component_instance.execute_tool(agent_input, bound_input)
    }

    fn component_engine() -> Result<Engine, WorkflowRuntimeError> {
        let mut engine_config = Config::new();
        engine_config.wasm_component_model(true);

        Engine::new(&engine_config).map_err(|error| WorkflowRuntimeError::Other {
            message: format!("failed to create wasm component engine: {error}"),
        })
    }

    fn read_tool_definition(
        execution_engine: &Engine,
        compiled_component: &Component,
        component_path: &Path,
    ) -> Result<ToolDefinition, WorkflowRuntimeError> {
        let mut component_instance = WasmToolComponentInstance::new(execution_engine, compiled_component, component_path)
            .map_err(|error| WorkflowRuntimeError::Other { message: error })?;

        let definition_result = component_instance
            .bindings
            .superwire_tool_tool()
            .call_definition(&mut component_instance.store)
            .map_err(|error| WorkflowRuntimeError::Other {
                message: format!(
                    "failed to call `definition` on wasm tool component `{}`: {error}",
                    component_path.display()
                ),
            })?;

        let component_tool_definition = definition_result.map_err(|error_message| WorkflowRuntimeError::Other {
            message: format!(
                "wasm tool component `{}` returned definition error: {error_message}",
                component_path.display()
            ),
        })?;

        let parameters_schema = serde_json::from_str::<Schema>(&component_tool_definition.parameters_schema_json).map_err(|error| {
            WorkflowRuntimeError::Other {
                message: format!(
                    "failed to parse `parameters_schema_json` from wasm tool component `{}`: {error}",
                    component_path.display()
                ),
            }
        })?;

        let bound_parameters_schema =
            serde_json::from_str::<Schema>(&component_tool_definition.bound_parameters_schema_json).map_err(|error| {
                WorkflowRuntimeError::Other {
                    message: format!(
                        "failed to parse `bound_parameters_schema_json` from wasm tool component `{}`: {error}",
                        component_path.display()
                    ),
                }
            })?;

        let output_schema =
            serde_json::from_str::<Schema>(&component_tool_definition.output_schema_json).map_err(|error| WorkflowRuntimeError::Other {
                message: format!(
                    "failed to parse `output_schema_json` from wasm tool component `{}`: {error}",
                    component_path.display()
                ),
            })?;

        Ok(ToolDefinition {
            name: component_tool_definition.name,
            description: component_tool_definition.description,
            parameters_schema,
            bound_parameters_schema: Some(bound_parameters_schema),
            output_schema: Some(output_schema),
        })
    }
}

struct WasmToolComponentStoreData {
    component_path: PathBuf,
}

impl contract::superwire::tool::host::Host for WasmToolComponentStoreData {
    fn http_get(&mut self, request_url: String) -> Result<String, String> {
        perform_http_get_request(&request_url).map_err(|error_message| {
            format!(
                "host-http-get failed for component `{}` with url `{request_url}`: {error_message}",
                self.component_path.display()
            )
        })
    }

    fn http_post_json(&mut self, request_url: String, request_body_json: String, internal_token: Option<String>) -> Result<String, String> {
        perform_http_post_json_request(&request_url, &request_body_json, internal_token.as_deref()).map_err(|error_message| {
            format!(
                "host-http-post-json failed for component `{}` with url `{request_url}`: {error_message}",
                self.component_path.display()
            )
        })
    }
}

struct WasmToolComponentInstance {
    store: Store<WasmToolComponentStoreData>,
    bindings: contract::SuperwireTool,
}

impl WasmToolComponentInstance {
    fn new(execution_engine: &Engine, compiled_component: &Component, component_path: &Path) -> Result<Self, String> {
        let mut component_linker = Linker::new(execution_engine);

        contract::superwire::tool::host::add_to_linker::<
            WasmToolComponentStoreData,
            wasmtime::component::HasSelf<WasmToolComponentStoreData>,
        >(&mut component_linker, |store_data| store_data)
        .map_err(|error| {
            format!(
                "failed to register host imports for component `{}`: {error}",
                component_path.display()
            )
        })?;

        let mut component_store = Store::new(
            execution_engine,
            WasmToolComponentStoreData {
                component_path: component_path.to_path_buf(),
            },
        );

        let component_bindings = contract::SuperwireTool::instantiate(&mut component_store, compiled_component, &component_linker)
            .map_err(|error| format!("failed to instantiate wasm tool component `{}`: {error}", component_path.display()))?;

        Ok(Self {
            store: component_store,
            bindings: component_bindings,
        })
    }

    fn execute_tool(&mut self, agent_input: Value, bound_input: Map<String, Value>) -> Result<Value, ToolError> {
        let serialized_agent_input = serde_json::to_string(&agent_input)
            .map_err(|error| ToolError::new(format!("failed to serialize agent tool input for wasm component: {error}")))?;

        let serialized_bound_input = serde_json::to_string(&Value::Object(bound_input))
            .map_err(|error| ToolError::new(format!("failed to serialize bound tool input for wasm component: {error}")))?;

        let execution_result = self
            .bindings
            .superwire_tool_tool()
            .call_execute(&mut self.store, &serialized_agent_input, &serialized_bound_input)
            .map_err(|error| ToolError::new(format!("wasm tool `execute` call failed: {error}")))?;

        let output_json = match execution_result {
            Ok(output_json) => output_json,
            Err(tool_error) => {
                return Err(ToolError::new(format!(
                    "wasm tool execution failed [{}]: {}",
                    tool_error.code, tool_error.message
                )));
            }
        };

        serde_json::from_str::<Value>(&output_json).map_err(|error| {
            ToolError::new(format!("wasm tool returned invalid JSON output: {error}"))
                .with_context("raw_output", Value::String(output_json))
        })
    }
}

fn perform_http_get_request(request_url: &str) -> Result<String, String> {
    let mut http_response = ureq::get(request_url)
        .header("accept", "application/json")
        .call()
        .map_err(|error| format!("http request to `{request_url}` failed: {error}"))?;

    http_response
        .body_mut()
        .read_to_string()
        .map_err(|error| format!("failed to read http response body from `{request_url}`: {error}"))
}

fn perform_http_post_json_request(request_url: &str, request_body_json: &str, internal_token: Option<&str>) -> Result<String, String> {
    let mut http_request = ureq::post(request_url)
        .header("accept", "application/json")
        .header("content-type", "application/json");

    let internal_token = if internal_token.is_some() {
        internal_token.map(str::to_string)
    } else {
        std::env::var("SUPERWIRE_INTERNAL_TOKEN").ok()
    };

    if let Some(internal_token) = internal_token {
        http_request = http_request.header("x-superwire-internal-token", &internal_token);
    }

    let mut http_response = http_request
        .send(request_body_json)
        .map_err(|error| format!("http post request to `{request_url}` failed: {error}"))?;

    http_response
        .body_mut()
        .read_to_string()
        .map_err(|error| format!("failed to read http post response body from `{request_url}`: {error}"))
}

#[derive(Clone)]
pub struct Tool<AgentInputType = Value, OutputType = Value, BoundInputType = Map<String, Value>> {
    component: WasmToolComponent,
    phantom: PhantomData<(AgentInputType, OutputType, BoundInputType)>,
}

impl<AgentInputType, OutputType, BoundInputType> Tool<AgentInputType, OutputType, BoundInputType>
where
    AgentInputType: Serialize,
    OutputType: DeserializeOwned,
    BoundInputType: Serialize,
{
    pub fn from_file(component_path: impl AsRef<Path>) -> Result<Self, WorkflowRuntimeError> {
        let component = WasmToolComponent::from_file(component_path.as_ref().to_path_buf())?;

        Ok(Self {
            component,
            phantom: PhantomData,
        })
    }

    #[must_use]
    pub fn definition(&self) -> &ToolDefinition {
        &self.component.tool_definition
    }

    #[allow(clippy::unused_async)]
    pub async fn run(&self, agent_input: AgentInputType) -> Result<OutputType, ToolError>
    where
        BoundInputType: Default,
    {
        self.run_with_bound_input(agent_input, BoundInputType::default()).await
    }

    #[allow(clippy::unused_async)]
    pub async fn run_with_bound_input(&self, agent_input: AgentInputType, bound_input: BoundInputType) -> Result<OutputType, ToolError> {
        let serialized_agent_input = self.serialize_agent_input(agent_input)?;
        let serialized_bound_input = self.serialize_bound_input(bound_input)?;
        let output_value = self.component.execute(serialized_agent_input, serialized_bound_input)?;

        self.deserialize_output(output_value)
    }

    #[allow(clippy::unused_async)]
    pub async fn run_without_input(&self) -> Result<OutputType, ToolError>
    where
        AgentInputType: Default,
        BoundInputType: Default,
    {
        self.run_with_bound_input(AgentInputType::default(), BoundInputType::default())
            .await
    }

    fn serialize_agent_input(&self, agent_input: AgentInputType) -> Result<Value, ToolError> {
        serde_json::to_value(agent_input)
            .map_err(|error| ToolError::new(format!("failed to serialize wasm tool agent input payload: {error}")))
    }

    fn serialize_bound_input(&self, bound_input: BoundInputType) -> Result<Map<String, Value>, ToolError> {
        let serialized_bound_input = serde_json::to_value(bound_input)
            .map_err(|error| ToolError::new(format!("failed to serialize wasm tool bound input payload: {error}")))?;

        let Some(bound_input_object) = serialized_bound_input.as_object() else {
            return Err(ToolError::new(
                "failed to serialize wasm tool bound input payload: expected object-compatible bound input",
            ));
        };

        Ok(bound_input_object.clone())
    }

    fn deserialize_output(&self, output_value: Value) -> Result<OutputType, ToolError> {
        serde_json::from_value(output_value)
            .map_err(|error| ToolError::new(format!("failed to deserialize wasm tool output payload: {error}")))
    }
}

#[cfg(test)]
mod tests {
    use super::WasmToolRuntimeLoader;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::{env, fs};

    static TEMPORARY_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn returns_empty_tool_list_when_tools_directory_is_missing() {
        let project_directory = create_temporary_project_directory("wasm-tools-missing");

        let runtime_tools = WasmToolRuntimeLoader::from_workflow_directory(&project_directory)
            .discover_runtime_tools()
            .expect("missing tools directory should not fail discovery");

        assert!(runtime_tools.is_empty());

        fs::remove_dir_all(project_directory).expect("temporary project directory should be removed");
    }

    #[test]
    fn fails_when_tools_path_is_not_a_directory() {
        let project_directory = create_temporary_project_directory("wasm-tools-invalid-directory");
        let tools_path = project_directory.join("tools");

        fs::write(&tools_path, b"not-a-directory").expect("tools path placeholder file should be written");

        let discovery_result = WasmToolRuntimeLoader::from_workflow_directory(&project_directory).discover_runtime_tools();

        assert!(discovery_result.is_err());

        fs::remove_dir_all(project_directory).expect("temporary project directory should be removed");
    }

    #[test]
    fn fails_when_tools_directory_contains_invalid_component_binary() {
        let project_directory = create_temporary_project_directory("wasm-tools-invalid-component");
        let tools_directory = project_directory.join("tools");

        fs::create_dir_all(&tools_directory).expect("tools directory should be created");
        fs::write(tools_directory.join("broken.wasm"), b"not-a-component").expect("invalid component binary should be written");

        let discovery_result = WasmToolRuntimeLoader::from_workflow_directory(&project_directory).discover_runtime_tools();

        assert!(discovery_result.is_err());

        fs::remove_dir_all(project_directory).expect("temporary project directory should be removed");
    }

    fn create_temporary_project_directory(prefix: &str) -> PathBuf {
        let sequence_value = TEMPORARY_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary_directory = env::temp_dir().join(format!("superwire-{prefix}-{}-{sequence_value}", std::process::id()));

        fs::create_dir_all(&temporary_directory).expect("temporary directory should be created");

        temporary_directory
    }
}
