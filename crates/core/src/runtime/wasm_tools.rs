use crate::runtime::error::WorkflowRuntimeError;
use schemars::Schema;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use superwire_agent::{DynamicTool, ToolDefinition, ToolError};
use wasmtime::{Caller, Engine, Extern, Instance, Linker, Memory, Module, Store, TypedFunc};

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

        for module_path in self.wasm_module_paths()? {
            let wasm_tool_module = WasmToolModule::from_file(module_path)?;
            let dynamic_tool = wasm_tool_module.as_dynamic_tool();
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

    fn wasm_module_paths(&self) -> Result<Vec<PathBuf>, WorkflowRuntimeError> {
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

        let mut wasm_module_paths = Vec::new();

        let directory_entries = fs::read_dir(&tools_directory).map_err(|error| WorkflowRuntimeError::Other {
            message: format!("failed to read tools directory `{}`: {error}", tools_directory.display()),
        })?;

        for directory_entry in directory_entries {
            let directory_entry = directory_entry.map_err(|error| WorkflowRuntimeError::Other {
                message: format!("failed to read directory entry in `{}`: {error}", tools_directory.display()),
            })?;

            let module_path = directory_entry.path();

            if !module_path.is_file() {
                continue;
            }

            if module_path.extension().and_then(std::ffi::OsStr::to_str) != Some("wasm") {
                continue;
            }

            wasm_module_paths.push(module_path);
        }

        wasm_module_paths.sort();

        Ok(wasm_module_paths)
    }
}

#[derive(Debug, Clone)]
struct WasmToolModule {
    module_path: PathBuf,
    execution_engine: Arc<Engine>,
    compiled_module: Arc<Module>,
    tool_definition: ToolDefinition,
}

impl WasmToolModule {
    fn from_file(module_path: PathBuf) -> Result<Self, WorkflowRuntimeError> {
        let execution_engine = Arc::new(Engine::default());
        let compiled_module =
            Arc::new(
                Module::from_file(&execution_engine, &module_path).map_err(|error| WorkflowRuntimeError::Other {
                    message: format!("failed to compile wasm tool module `{}`: {error}", module_path.display()),
                })?,
            );

        let tool_definition = Self::read_tool_definition(&execution_engine, &compiled_module, &module_path)?;

        Ok(Self {
            module_path,
            execution_engine,
            compiled_module,
            tool_definition,
        })
    }

    #[must_use]
    fn as_dynamic_tool(&self) -> DynamicTool {
        let tool_definition = self.tool_definition.clone();
        let wasm_tool_module = self.clone();

        DynamicTool::new(tool_definition, move |tool_input| {
            let wasm_tool_module = wasm_tool_module.clone();

            async move { wasm_tool_module.execute(tool_input) }
        })
    }

    fn execute(&self, tool_input: Value) -> Result<Value, ToolError> {
        let mut instance_handle =
            WasmToolInstance::new(&self.execution_engine, &self.compiled_module, &self.module_path).map_err(ToolError::new)?;

        instance_handle.execute_tool(tool_input)
    }

    fn read_tool_definition(
        execution_engine: &Engine,
        compiled_module: &Module,
        module_path: &Path,
    ) -> Result<ToolDefinition, WorkflowRuntimeError> {
        let mut instance_handle = WasmToolInstance::new(execution_engine, compiled_module, module_path)
            .map_err(|error| WorkflowRuntimeError::Other { message: error })?;

        let definition_json = instance_handle
            .read_exported_json_string(WasmToolExportName::Definition)
            .map_err(|error| WorkflowRuntimeError::Other { message: error })?;

        let definition_payload =
            serde_json::from_str::<WasmToolDefinitionPayload>(&definition_json).map_err(|error| WorkflowRuntimeError::Other {
                message: format!(
                    "failed to parse `tool_definition` payload from wasm module `{}`: {error}",
                    module_path.display()
                ),
            })?;

        Ok(definition_payload.into_tool_definition())
    }
}

struct WasmToolInstance {
    store: Store<()>,
    instance: Instance,
    module_path: PathBuf,
}

impl WasmToolInstance {
    fn new(execution_engine: &Engine, compiled_module: &Module, module_path: &Path) -> Result<Self, String> {
        let mut store = Store::new(execution_engine, ());

        let mut linker = Linker::new(execution_engine);

        Self::register_host_http_import(&mut linker, module_path)?;

        let instance = linker
            .instantiate(&mut store, compiled_module)
            .map_err(|error| format!("failed to instantiate wasm tool module `{}`: {error}", module_path.display()))?;

        Ok(Self {
            store,
            instance,
            module_path: module_path.to_path_buf(),
        })
    }

    fn execute_tool(&mut self, tool_input: Value) -> Result<Value, ToolError> {
        let serialized_tool_input = serde_json::to_vec(&tool_input)
            .map_err(|error| ToolError::new(format!("failed to serialize tool input for wasm module: {error}")))?;

        let input_length =
            i32::try_from(serialized_tool_input.len()).map_err(|_| ToolError::new("tool input payload exceeds wasm i32 length limits"))?;

        let allocate_function = self.allocate_function().map_err(ToolError::new)?;

        let input_pointer = allocate_function
            .call(&mut self.store, input_length)
            .map_err(|error| ToolError::new(format!("wasm tool `tool_alloc` failed: {error}")))?;

        let input_pointer_offset =
            usize::try_from(input_pointer).map_err(|_| ToolError::new("wasm tool returned a negative pointer from `tool_alloc`"))?;

        let tool_memory = self.memory().map_err(ToolError::new)?;

        tool_memory
            .write(&mut self.store, input_pointer_offset, &serialized_tool_input)
            .map_err(|error| ToolError::new(format!("failed to write input payload into wasm memory: {error}")))?;

        let execute_function = self.execute_function().map_err(ToolError::new)?;

        let output_pointer_and_length = execute_function
            .call(&mut self.store, (input_pointer, input_length))
            .map_err(|error| ToolError::new(format!("wasm tool `tool_execute` failed: {error}")))?;

        let output_json = self
            .read_memory_slice_as_string(output_pointer_and_length)
            .map_err(ToolError::new)?;

        serde_json::from_str::<Value>(&output_json).map_err(|error| {
            ToolError::new(format!(
                "wasm tool `{}` returned invalid JSON output: {error}",
                self.module_path.display()
            ))
            .with_context("raw_output", Value::String(output_json))
        })
    }

    fn read_exported_json_string(&mut self, export_name: WasmToolExportName) -> Result<String, String> {
        let exported_function = self.json_slice_function(export_name)?;

        let pointer_and_length = exported_function.call(&mut self.store, ()).map_err(|error| {
            format!(
                "failed to call export `{}` for wasm module `{}`: {error}",
                export_name.as_str(),
                self.module_path.display()
            )
        })?;

        self.read_memory_slice_as_string(pointer_and_length)
    }

    fn register_host_http_import(linker: &mut Linker<()>, module_path: &Path) -> Result<(), String> {
        linker
            .func_wrap(
                "superwire",
                "host_http_get",
                move |mut caller: Caller<'_, ()>, url_pointer: i32, url_length: i32| {
                    Self::host_http_get(&mut caller, url_pointer, url_length).unwrap_or(0)
                },
            )
            .map_err(|error| {
                format!(
                    "failed to register host import `superwire.host_http_get` for module `{}`: {error}",
                    module_path.display()
                )
            })
            .map(|_| ())
    }

    fn host_http_get(caller: &mut Caller<'_, ()>, url_pointer: i32, url_length: i32) -> Result<i64, String> {
        let request_url = Self::read_caller_memory_as_string(caller, url_pointer, url_length)?;

        let response_body = Self::perform_http_get_request(&request_url)?;

        Self::write_string_into_caller_memory(caller, &response_body)
    }

    fn perform_http_get_request(request_url: &str) -> Result<String, String> {
        let http_agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(15)).build();

        let http_response = http_agent
            .get(request_url)
            .set("accept", "application/json")
            .call()
            .map_err(|error| format!("http request to `{request_url}` failed: {error}"))?;

        http_response
            .into_string()
            .map_err(|error| format!("failed to read http response body from `{request_url}`: {error}"))
    }

    fn read_caller_memory_as_string(caller: &mut Caller<'_, ()>, pointer: i32, length: i32) -> Result<String, String> {
        if pointer <= 0 {
            return Err("caller memory pointer must be positive".to_string());
        }

        if length < 0 {
            return Err("caller memory length cannot be negative".to_string());
        }

        let byte_offset = usize::try_from(pointer).map_err(|_| "caller memory pointer does not fit usize".to_string())?;
        let byte_length = usize::try_from(length).map_err(|_| "caller memory length does not fit usize".to_string())?;
        let memory_slice = WasmMemorySlice { byte_offset, byte_length };

        let memory = Self::caller_memory(caller)?;
        let memory_bytes = memory.data(&*caller);
        let end_offset = memory_slice.end_offset()?;

        if end_offset > memory_bytes.len() {
            return Err(format!(
                "caller memory slice out of bounds (offset={}, length={}, memory_size={})",
                memory_slice.byte_offset,
                memory_slice.byte_length,
                memory_bytes.len()
            ));
        }

        let slice_bytes = &memory_bytes[memory_slice.byte_offset..end_offset];

        String::from_utf8(slice_bytes.to_vec()).map_err(|error| format!("caller memory payload is not utf-8: {error}"))
    }

    fn write_string_into_caller_memory(caller: &mut Caller<'_, ()>, payload: &str) -> Result<i64, String> {
        let payload_bytes = payload.as_bytes();

        let payload_length = i32::try_from(payload_bytes.len()).map_err(|_| "host payload length exceeds wasm i32 limits".to_string())?;

        let allocation_pointer = Self::allocate_caller_memory(caller, payload_length)?;

        if allocation_pointer <= 0 {
            return Err("guest `tool_alloc` returned a non-positive pointer".to_string());
        }

        let allocation_offset =
            usize::try_from(allocation_pointer).map_err(|_| "guest allocation pointer does not fit usize".to_string())?;

        let memory = Self::caller_memory(caller)?;

        memory
            .write(&mut *caller, allocation_offset, payload_bytes)
            .map_err(|error| format!("failed to write host payload into wasm memory: {error}"))?;

        let memory_slice = WasmMemorySlice {
            byte_offset: allocation_offset,
            byte_length: payload_bytes.len(),
        };

        memory_slice.to_encoded_i64()
    }

    fn allocate_caller_memory(caller: &mut Caller<'_, ()>, allocation_length: i32) -> Result<i32, String> {
        let Some(allocation_export) = caller.get_export(WasmToolExportName::Allocate.as_str()) else {
            return Err("guest module is missing `tool_alloc` while serving host import".to_string());
        };

        let Extern::Func(allocation_function) = allocation_export else {
            return Err("guest `tool_alloc` export is not a function".to_string());
        };

        let typed_allocation_function = allocation_function
            .typed::<i32, i32>(&mut *caller)
            .map_err(|error| format!("guest `tool_alloc` has invalid signature: {error}"))?;

        typed_allocation_function
            .call(&mut *caller, allocation_length)
            .map_err(|error| format!("guest `tool_alloc` call failed: {error}"))
    }

    fn caller_memory(caller: &mut Caller<'_, ()>) -> Result<Memory, String> {
        let Some(memory_export) = caller.get_export(WasmToolExportName::Memory.as_str()) else {
            return Err("guest module is missing exported `memory`".to_string());
        };

        let Extern::Memory(memory) = memory_export else {
            return Err("guest `memory` export is not a memory".to_string());
        };

        Ok(memory)
    }

    fn json_slice_function(&mut self, export_name: WasmToolExportName) -> Result<TypedFunc<(), i64>, String> {
        self.instance
            .get_typed_func::<(), i64>(&mut self.store, export_name.as_str())
            .map_err(|error| {
                format!(
                    "missing or invalid wasm export `{}` in module `{}`: {error}",
                    export_name.as_str(),
                    self.module_path.display()
                )
            })
    }

    fn allocate_function(&mut self) -> Result<TypedFunc<i32, i32>, String> {
        self.instance
            .get_typed_func::<i32, i32>(&mut self.store, WasmToolExportName::Allocate.as_str())
            .map_err(|error| {
                format!(
                    "missing or invalid wasm export `{}` in module `{}`: {error}",
                    WasmToolExportName::Allocate.as_str(),
                    self.module_path.display()
                )
            })
    }

    fn execute_function(&mut self) -> Result<TypedFunc<(i32, i32), i64>, String> {
        self.instance
            .get_typed_func::<(i32, i32), i64>(&mut self.store, WasmToolExportName::Execute.as_str())
            .map_err(|error| {
                format!(
                    "missing or invalid wasm export `{}` in module `{}`: {error}",
                    WasmToolExportName::Execute.as_str(),
                    self.module_path.display()
                )
            })
    }

    fn memory(&mut self) -> Result<Memory, String> {
        self.instance
            .get_memory(&mut self.store, WasmToolExportName::Memory.as_str())
            .ok_or_else(|| format!("missing wasm memory export `memory` in module `{}`", self.module_path.display()))
    }

    fn read_memory_slice_as_string(&mut self, encoded_slice: i64) -> Result<String, String> {
        let memory_slice = WasmMemorySlice::from_encoded_i64(encoded_slice)?;
        let tool_memory = self.memory()?;
        let memory_bytes = tool_memory.data(&self.store);

        let end_offset = memory_slice.end_offset()?;

        if end_offset > memory_bytes.len() {
            return Err(format!(
                "wasm memory slice out of bounds for module `{}` (offset={}, length={}, memory_size={})",
                self.module_path.display(),
                memory_slice.byte_offset,
                memory_slice.byte_length,
                memory_bytes.len()
            ));
        }

        let slice_bytes = &memory_bytes[memory_slice.byte_offset..end_offset];

        String::from_utf8(slice_bytes.to_vec())
            .map_err(|error| format!("wasm module `{}` returned non-utf8 payload: {error}", self.module_path.display()))
    }
}

#[derive(Debug, Clone, Copy)]
struct WasmMemorySlice {
    byte_offset: usize,
    byte_length: usize,
}

impl WasmMemorySlice {
    fn from_encoded_i64(encoded_slice: i64) -> Result<Self, String> {
        let encoded_slice = u64::try_from(encoded_slice).map_err(|_| "wasm tool returned a negative pointer-length payload".to_string())?;

        let byte_offset_u64 = encoded_slice >> 32;
        let byte_length_u64 = encoded_slice & 0xFFFF_FFFF;

        let byte_offset = usize::try_from(byte_offset_u64).map_err(|_| "wasm tool pointer does not fit in host usize".to_string())?;

        let byte_length = usize::try_from(byte_length_u64).map_err(|_| "wasm tool length does not fit in host usize".to_string())?;

        Ok(Self { byte_offset, byte_length })
    }

    fn end_offset(self) -> Result<usize, String> {
        self.byte_offset
            .checked_add(self.byte_length)
            .ok_or_else(|| "wasm tool pointer-length overflowed host usize".to_string())
    }

    fn to_encoded_i64(self) -> Result<i64, String> {
        let byte_offset = u32::try_from(self.byte_offset).map_err(|_| "wasm tool pointer does not fit into 32-bit encoding".to_string())?;

        let byte_length = u32::try_from(self.byte_length).map_err(|_| "wasm tool length does not fit into 32-bit encoding".to_string())?;

        let encoded_slice = (u64::from(byte_offset) << 32) | u64::from(byte_length);

        i64::try_from(encoded_slice).map_err(|_| "encoded wasm memory slice does not fit i64".to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WasmToolExportName {
    Memory,
    Allocate,
    Definition,
    Execute,
}

impl WasmToolExportName {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Allocate => "tool_alloc",
            Self::Definition => "tool_definition",
            Self::Execute => "tool_execute",
        }
    }
}

#[derive(Debug, Deserialize)]
struct WasmToolDefinitionPayload {
    name: String,
    description: String,
    parameters_schema: Schema,
}

impl WasmToolDefinitionPayload {
    fn into_tool_definition(self) -> ToolDefinition {
        ToolDefinition {
            name: self.name,
            description: self.description,
            parameters_schema: self.parameters_schema,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WasmToolRuntimeLoader;
    use serde_json::json;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::{env, fs};
    use superwire_agent::tool::RuntimeTool;

    static TEMPORARY_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    #[tokio::test]
    async fn discovers_and_executes_wasm_tools_from_workflow_tools_directory() {
        let project_directory = create_temporary_project_directory("wasm-tools-discovery");
        let tools_directory = project_directory.join("tools");

        fs::create_dir_all(&tools_directory).expect("tools directory should be created");

        write_test_wasm_tool_module(&tools_directory.join("weather.wasm"));

        let runtime_tools = WasmToolRuntimeLoader::from_workflow_directory(&project_directory)
            .discover_runtime_tools()
            .expect("wasm tools should be discovered");

        assert_eq!(runtime_tools.len(), 1);

        let tool_definition = runtime_tools[0].definition().expect("tool definition should be available");

        assert_eq!(tool_definition.name, "weather");
        assert_eq!(tool_definition.description, "Returns static weather output");

        let output_value = runtime_tools[0]
            .execute(json!({ "city": "Madrid" }))
            .await
            .expect("tool execution should succeed");

        assert_eq!(output_value, json!({ "status": "sunny" }));

        fs::remove_dir_all(project_directory).expect("temporary project directory should be removed");
    }

    #[test]
    fn returns_empty_tool_list_when_tools_directory_is_missing() {
        let project_directory = create_temporary_project_directory("wasm-tools-missing");

        let runtime_tools = WasmToolRuntimeLoader::from_workflow_directory(&project_directory)
            .discover_runtime_tools()
            .expect("missing tools directory should not fail discovery");

        assert!(runtime_tools.is_empty());

        fs::remove_dir_all(project_directory).expect("temporary project directory should be removed");
    }

    fn create_temporary_project_directory(prefix: &str) -> PathBuf {
        let sequence_value = TEMPORARY_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary_directory = env::temp_dir().join(format!("superwire-{prefix}-{}-{sequence_value}", std::process::id()));

        fs::create_dir_all(&temporary_directory).expect("temporary directory should be created");

        temporary_directory
    }

    fn write_test_wasm_tool_module(module_path: &Path) {
        let wasm_module_bytes = wat::parse_str(TEST_WASM_TOOL_MODULE).expect("test wasm module should parse");

        fs::write(module_path, wasm_module_bytes).expect("test wasm module should be written");
    }

    const TEST_WASM_TOOL_MODULE: &str = r#"
        (module
          (memory (export "memory") 1)

          (global $heap_pointer (mut i32) (i32.const 1024))

          (func $pack_slice (param $slice_offset i32) (param $slice_length i32) (result i64)
            (i64.or
              (i64.shl
                (i64.extend_i32_u (local.get $slice_offset))
                (i64.const 32)
              )
              (i64.extend_i32_u (local.get $slice_length))
            )
          )

          (func (export "tool_alloc") (param $allocation_length i32) (result i32)
            (local $allocation_offset i32)

            (local.set $allocation_offset (global.get $heap_pointer))

            (global.set $heap_pointer
              (i32.add (global.get $heap_pointer) (local.get $allocation_length))
            )

            (local.get $allocation_offset)
          )

          (data (i32.const 0) "{\"name\":\"weather\",\"description\":\"Returns static weather output\",\"parameters_schema\":{\"type\":\"object\",\"properties\":{\"city\":{\"type\":\"string\"}},\"required\":[\"city\"]}}")
          (data (i32.const 256) "{\"status\":\"sunny\"}")

          (func (export "tool_definition") (result i64)
            (call $pack_slice (i32.const 0) (i32.const 162))
          )

          (func (export "tool_execute") (param $input_offset i32) (param $input_length i32) (result i64)
            (call $pack_slice (i32.const 256) (i32.const 18))
          )
        )
    "#;
}
