use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::{collections::HashMap, ffi::OsString};

use clap::{Args, Subcommand};
use schemars::Schema;
use serde_json::{Map, Value};
use superwire_agent::ToolDefinition;
use superwire_core::Tool;

use crate::diagnostics::CommandError;

const GENERATED_TOOL_CARGO_MANIFEST_TEMPLATE: &str = include_str!("../../templates/cargo.toml.template");
const GENERATED_TOOL_COMPONENT_SOURCE_TEMPLATE: &str = include_str!("../../templates/lib.rs.template");
const EMBEDDED_TOOL_WIT_SOURCE: &str = include_str!("../../../../crates/core/wit/runtime/superwire-tool.wit");
const EMBEDDED_WASM_TOOL_SDK_SOURCE: &str = include_str!("../../../../crates/wasm-tool-sdk/src/lib.rs");
const TOOL_SOURCES_MANIFEST_TEMPLATE: &str = r#"[package]
name = "tool-sources"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"

[dependencies]
"#;
const TOOL_SOURCES_LIB_TEMPLATE: &str = "pub mod {{tool_name}};\n";
const TOOL_SOURCE_TEMPLATE: &str = r#"use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use superwire_wasm_tool_sdk::{Tool, ToolExecutionError, ToolMetadata};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct {{tool_type_name}}Input {
    pub query: String,
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct {{tool_type_name}}BoundInput {
    pub workspace_id: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct {{tool_type_name}}Output {
    pub message: String,
    pub workspace_id: Option<String>,
}

pub struct {{tool_type_name}};

impl Tool for {{tool_type_name}} {
    type AgentInput = {{tool_type_name}}Input;
    type BoundInput = {{tool_type_name}}BoundInput;
    type Output = {{tool_type_name}}Output;

    fn metadata() -> ToolMetadata {
        ToolMetadata::new("{{tool_name}}", "Describe what this tool does")
    }

    async fn execute(agent_input: Self::AgentInput, bound_input: Self::BoundInput) -> Result<Self::Output, ToolExecutionError> {
        let resolved_limit = agent_input.limit.unwrap_or(10);
        let message = format!("query={}, limit={resolved_limit}", agent_input.query);

        Ok({{tool_type_name}}Output {
            message,
            workspace_id: bound_input.workspace_id,
        })
    }
}
"#;

fn embedded_wasm_tool_sdk_source() -> String {
    EMBEDDED_WASM_TOOL_SDK_SOURCE
        .replace("$crate::", "crate::superwire_wasm_tool_sdk::")
        .replace("crate::superwire_wasm_tool_sdk::php_proxy_tool!", "crate::php_proxy_tool!")
}

#[derive(Debug, Args)]
pub struct ToolsCommand {
    #[command(subcommand)]
    command: ToolsSubcommand,
}

impl ToolsCommand {
    pub fn execute(self) -> Result<(), CommandError> {
        match self.command {
            ToolsSubcommand::Init(init_tools_command) => init_tools_command.execute(),
            ToolsSubcommand::Build(build_tools_command) => build_tools_command.execute(),
            ToolsSubcommand::Inspect(inspect_tools_command) => inspect_tools_command.execute(),
        }
    }
}

#[derive(Debug, Subcommand)]
enum ToolsSubcommand {
    Init(InitToolsCommand),
    Build(BuildToolsCommand),
    Inspect(InspectToolsCommand),
}

#[derive(Debug, Args)]
struct InitToolsCommand {
    #[arg(value_name = "DIRECTORY", default_value = ".")]
    directory: PathBuf,

    #[arg(long, value_name = "TOOL_NAME", default_value = "example_tool")]
    tool_name: String,
}

impl InitToolsCommand {
    fn execute(self) -> Result<(), CommandError> {
        self.validate_tool_name()?;

        let output_directory = if self.directory.is_absolute() {
            self.directory.clone()
        } else {
            Path::new(".").join(&self.directory)
        };

        fs::create_dir_all(&output_directory).map_err(|error| {
            CommandError::internal(format!("failed to create output directory {}: {error}", output_directory.display()))
        })?;

        let tool_sources_directory = output_directory.join("tool-sources");
        let tool_sources_source_directory = tool_sources_directory.join("src");
        let tool_sources_manifest_path = tool_sources_directory.join("Cargo.toml");
        let tool_sources_lib_path = tool_sources_source_directory.join("lib.rs");
        let tool_source_path = tool_sources_source_directory.join(format!("{}.rs", self.tool_name));

        Self::ensure_paths_do_not_exist([
            tool_sources_manifest_path.as_path(),
            tool_sources_lib_path.as_path(),
            tool_source_path.as_path(),
        ])?;

        fs::create_dir_all(&tool_sources_source_directory).map_err(|error| {
            CommandError::internal(format!(
                "failed to create tool source directory {}: {error}",
                tool_sources_source_directory.display()
            ))
        })?;

        fs::write(&tool_sources_manifest_path, TOOL_SOURCES_MANIFEST_TEMPLATE).map_err(|error| {
            CommandError::internal(format!(
                "failed to write tool sources manifest {}: {error}",
                tool_sources_manifest_path.display()
            ))
        })?;

        let tool_sources_lib_source = TOOL_SOURCES_LIB_TEMPLATE.replace("{{tool_name}}", &self.tool_name);
        fs::write(&tool_sources_lib_path, tool_sources_lib_source)
            .map_err(|error| CommandError::internal(format!("failed to write source file {}: {error}", tool_sources_lib_path.display())))?;

        let tool_type_name = Self::tool_type_name(&self.tool_name);
        let tool_source = TOOL_SOURCE_TEMPLATE
            .replace("{{tool_name}}", &self.tool_name)
            .replace("{{tool_type_name}}", &tool_type_name);

        fs::write(&tool_source_path, tool_source)
            .map_err(|error| CommandError::internal(format!("failed to write source file {}: {error}", tool_source_path.display())))?;

        println!("initialized {}", tool_sources_manifest_path.display());
        println!("initialized {}", tool_sources_lib_path.display());
        println!("initialized {}", tool_source_path.display());
        println!("next: superwire-cli tools build {}", output_directory.display());

        Ok(())
    }

    fn validate_tool_name(&self) -> Result<(), CommandError> {
        if self.tool_name.is_empty() {
            return Err(CommandError::invalid_input("tool name cannot be empty"));
        }

        let first_character = self.tool_name.chars().next().unwrap_or_default();

        if !first_character.is_ascii_lowercase() {
            return Err(CommandError::invalid_input("tool name must start with a lowercase ASCII letter"));
        }

        if !self
            .tool_name
            .chars()
            .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_')
        {
            return Err(CommandError::invalid_input(
                "tool name can only contain lowercase ASCII letters, digits, and underscores",
            ));
        }

        Ok(())
    }

    fn ensure_paths_do_not_exist<'path>(paths: impl IntoIterator<Item = &'path Path>) -> Result<(), CommandError> {
        for path in paths {
            if path.exists() {
                return Err(CommandError::invalid_input(format!("file already exists: {}", path.display())));
            }
        }

        Ok(())
    }

    fn tool_type_name(tool_name: &str) -> String {
        let mut converted_name = String::new();

        for tool_name_segment in tool_name.split('_') {
            let mut segment_characters = tool_name_segment.chars();

            let Some(first_character) = segment_characters.next() else {
                continue;
            };

            converted_name.push(first_character.to_ascii_uppercase());

            for character in segment_characters {
                converted_name.push(character);
            }
        }

        converted_name
    }
}

#[derive(Debug, Args)]
struct InspectToolsCommand {
    #[arg(value_name = "WASM_PATH")]
    wasm_path: PathBuf,
}

impl InspectToolsCommand {
    fn execute(self) -> Result<(), CommandError> {
        let resolved_wasm_path = self.resolve_wasm_path()?;
        let tool = Tool::<Value, Value, Map<String, Value>>::from_file(&resolved_wasm_path)
            .map_err(|error| CommandError::invalid_input(format!("failed to load wasm tool {}: {error}", resolved_wasm_path.display())))?;
        let rendered_tool_inspection = ToolInspectionReport::from_tool_definition(tool.definition())?.render();

        println!("{rendered_tool_inspection}");

        Ok(())
    }

    fn resolve_wasm_path(&self) -> Result<PathBuf, CommandError> {
        let resolved_wasm_path = fs::canonicalize(&self.wasm_path)
            .map_err(|_| CommandError::invalid_input(format!("path does not exist: {}", self.wasm_path.display())))?;

        if !resolved_wasm_path.is_file() {
            return Err(CommandError::invalid_input(format!(
                "expected a .wasm file path, got non-file path: {}",
                self.wasm_path.display()
            )));
        }

        if resolved_wasm_path.extension().and_then(|extension| extension.to_str()) != Some("wasm") {
            return Err(CommandError::invalid_input(format!(
                "expected a .wasm file path, got: {}",
                self.wasm_path.display()
            )));
        }

        Ok(resolved_wasm_path)
    }
}

struct ToolInspectionReport {
    lines: Vec<String>,
}

impl ToolInspectionReport {
    fn from_tool_definition(tool_definition: &ToolDefinition) -> Result<Self, CommandError> {
        let output_styler = OutputStyler::for_stdout();
        let mut report = Self { lines: Vec::new() };

        report.lines.push(format!(
            "{}: {}",
            output_styler.label("tool"),
            output_styler.tool_name(&tool_definition.name)
        ));
        report.lines.push(format!(
            "{}: {}",
            output_styler.label("description"),
            output_styler.description(&tool_definition.description)
        ));
        report.lines.push(String::new());

        report.push_schema_section("input schema", &tool_definition.parameters_schema, &output_styler)?;

        if let Some(bound_input_schema) = &tool_definition.bound_parameters_schema {
            report.lines.push(String::new());
            report.push_schema_section("bound input schema", bound_input_schema, &output_styler)?;
        }

        if let Some(output_schema) = &tool_definition.output_schema {
            report.lines.push(String::new());
            report.push_schema_section("output schema", output_schema, &output_styler)?;
        }

        Ok(report)
    }

    fn push_schema_section(&mut self, section_name: &str, schema: &Schema, output_styler: &OutputStyler) -> Result<(), CommandError> {
        let schema_value =
            serde_json::to_value(schema).map_err(|error| CommandError::internal(format!("failed to serialize {section_name}: {error}")))?;
        let mut schema_renderer = JsonSchemaRenderer::from_root_schema(&schema_value, section_name.to_string(), output_styler);

        self.lines.push(format!("{}:", output_styler.section(section_name)));
        schema_renderer.render_into(&mut self.lines);

        Ok(())
    }

    fn render(self) -> String {
        self.lines.join("\n")
    }
}

struct OutputStyler {
    colors_enabled: bool,
}

impl OutputStyler {
    fn for_stdout() -> Self {
        let terminal_supports_colors = std::io::stdout().is_terminal();
        let no_color_requested = std::env::var_os("NO_COLOR").is_some();
        let clicolor_value = std::env::var("CLICOLOR").ok();
        let clicolor_disables_colors = clicolor_value.as_deref() == Some("0");

        Self {
            colors_enabled: terminal_supports_colors && !no_color_requested && !clicolor_disables_colors,
        }
    }

    #[cfg(test)]
    fn without_colors() -> Self {
        Self { colors_enabled: false }
    }

    fn section(&self, value: &str) -> String {
        self.paint(value, "1;36")
    }

    fn label(&self, value: &str) -> String {
        self.paint(value, "1;33")
    }

    fn tool_name(&self, value: &str) -> String {
        self.paint(value, "1;32")
    }

    fn value_type(&self, value: &str) -> String {
        self.paint(value, "32")
    }

    fn description(&self, value: &str) -> String {
        self.paint(value, "2;37")
    }

    fn required_marker(&self) -> String {
        self.paint("[required]", "1;31")
    }

    fn paint(&self, value: &str, color_code: &str) -> String {
        if self.colors_enabled {
            format!("\x1b[{color_code}m{value}\x1b[0m")
        } else {
            value.to_string()
        }
    }
}

struct JsonSchemaRenderer<'schema, 'style> {
    root_schema: &'schema Value,
    schema_label: String,
    active_references: Vec<String>,
    output_styler: &'style OutputStyler,
}

impl<'schema, 'style> JsonSchemaRenderer<'schema, 'style> {
    fn from_root_schema(root_schema: &'schema Value, schema_label: String, output_styler: &'style OutputStyler) -> Self {
        Self {
            root_schema,
            schema_label,
            active_references: Vec::new(),
            output_styler,
        }
    }

    fn render_into(&mut self, output_lines: &mut Vec<String>) {
        let schema_label = self.schema_label.clone();

        self.render_schema_node(self.root_schema, output_lines, 1, Some(schema_label.as_str()), false);
    }

    fn render_schema_node(
        &mut self,
        schema_node: &Value,
        output_lines: &mut Vec<String>,
        depth: usize,
        node_name: Option<&str>,
        required: bool,
    ) {
        let indentation = self.indentation(depth);

        let required_marker = if required {
            format!(" {}", self.output_styler.required_marker())
        } else {
            String::new()
        };

        let node_label = self.output_styler.label(node_name.unwrap_or("value"));
        let value_type = self.describe_value_type(schema_node);
        let description = self.schema_description(schema_node);
        let type_segment = self.output_styler.value_type(&value_type);

        if let Some(description) = description {
            output_lines.push(format!(
                "{indentation}- {node_label}{required_marker} -> {type_segment}; {}",
                self.output_styler.description(description)
            ));
        } else {
            output_lines.push(format!("{indentation}- {node_label}{required_marker} -> {type_segment}"));
        }

        if let Some(reference_target) = self.reference_target(schema_node) {
            if self
                .active_references
                .iter()
                .any(|active_reference| active_reference == reference_target)
            {
                let recursive_indentation = self.indentation(depth + 1);
                output_lines.push(format!("{recursive_indentation}- recursive reference: {reference_target}"));

                return;
            }

            if let Some(referenced_schema) = self.resolve_reference(reference_target).cloned() {
                self.active_references.push(reference_target.to_string());
                self.render_schema_node(&referenced_schema, output_lines, depth + 1, Some("referenced schema"), false);
                self.active_references.pop();
            }

            return;
        }

        self.render_union_variants(schema_node, output_lines, depth);
        self.render_object_properties(schema_node, output_lines, depth);
        self.render_array_items(schema_node, output_lines, depth);
    }

    fn render_union_variants(&mut self, schema_node: &Value, output_lines: &mut Vec<String>, depth: usize) {
        for (keyword, branch_label) in [("oneOf", "oneOf option"), ("anyOf", "anyOf option"), ("allOf", "allOf item")] {
            let Some(union_values) = schema_node.get(keyword).and_then(Value::as_array) else {
                continue;
            };

            for (union_index, union_value) in union_values.iter().enumerate() {
                let union_label = format!("{branch_label} {}", union_index + 1);

                self.render_schema_node(union_value, output_lines, depth + 1, Some(union_label.as_str()), false);
            }
        }
    }

    fn render_object_properties(&mut self, schema_node: &Value, output_lines: &mut Vec<String>, depth: usize) {
        let Some(properties_object) = schema_node.get("properties").and_then(Value::as_object) else {
            return;
        };

        let required_property_names = self.required_property_names(schema_node);
        let mut sorted_property_names = properties_object.keys().cloned().collect::<Vec<_>>();

        sorted_property_names.sort();

        for property_name in sorted_property_names {
            let Some(property_schema) = properties_object.get(&property_name) else {
                continue;
            };

            let property_is_required = required_property_names.iter().any(|required_name| required_name == &property_name);

            self.render_schema_node(
                property_schema,
                output_lines,
                depth + 1,
                Some(property_name.as_str()),
                property_is_required,
            );
        }
    }

    fn render_array_items(&mut self, schema_node: &Value, output_lines: &mut Vec<String>, depth: usize) {
        if let Some(tuple_item_schemas) = schema_node.get("prefixItems").and_then(Value::as_array) {
            for (tuple_item_index, tuple_item_schema) in tuple_item_schemas.iter().enumerate() {
                let tuple_item_label = format!("item {}", tuple_item_index + 1);

                self.render_schema_node(tuple_item_schema, output_lines, depth + 1, Some(tuple_item_label.as_str()), false);
            }
        }

        if let Some(item_schema) = schema_node.get("items") {
            if let Some(tuple_item_schemas) = item_schema.as_array() {
                for (tuple_item_index, tuple_item_schema) in tuple_item_schemas.iter().enumerate() {
                    let tuple_item_label = format!("item {}", tuple_item_index + 1);

                    self.render_schema_node(tuple_item_schema, output_lines, depth + 1, Some(tuple_item_label.as_str()), false);
                }

                return;
            }

            self.render_schema_node(item_schema, output_lines, depth + 1, Some("items"), false);
        }

        if let Some(additional_item_schema) = schema_node.get("additionalItems") {
            self.render_schema_node(additional_item_schema, output_lines, depth + 1, Some("additional items"), false);
        }
    }

    fn required_property_names(&self, schema_node: &Value) -> Vec<String> {
        schema_node
            .get("required")
            .and_then(Value::as_array)
            .map(|required_values| {
                required_values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    }

    fn describe_value_type(&self, schema_node: &Value) -> String {
        if let Some(enum_values) = schema_node.get("enum").and_then(Value::as_array) {
            return format!("enum ({})", Self::join_display_values(enum_values));
        }

        if let Some(const_value) = schema_node.get("const") {
            return format!("const ({})", Self::display_value(const_value));
        }

        if let Some(type_value) = schema_node.get("type") {
            let mut rendered_type = Self::display_value_type_keyword(type_value);

            if rendered_type == "array" {
                if let Some(tuple_item_schemas) = schema_node.get("prefixItems").and_then(Value::as_array) {
                    rendered_type = format!("tuple[{}]", tuple_item_schemas.len());
                }

                if let Some(tuple_item_schemas) = schema_node.get("items").and_then(Value::as_array) {
                    rendered_type = format!("tuple[{}]", tuple_item_schemas.len());
                }
            }

            return rendered_type;
        }

        if schema_node.get("properties").is_some() {
            return "object".to_string();
        }

        if schema_node.get("items").is_some() {
            return "array".to_string();
        }

        if let Some(tuple_item_schemas) = schema_node.get("prefixItems").and_then(Value::as_array) {
            return format!("tuple[{}]", tuple_item_schemas.len());
        }

        if let Some(one_of_values) = schema_node.get("oneOf").and_then(Value::as_array) {
            return format!("oneOf ({})", self.union_member_types(one_of_values));
        }

        if let Some(any_of_values) = schema_node.get("anyOf").and_then(Value::as_array) {
            return format!("anyOf ({})", self.union_member_types(any_of_values));
        }

        if let Some(all_of_values) = schema_node.get("allOf").and_then(Value::as_array) {
            return format!("allOf ({})", self.union_member_types(all_of_values));
        }

        if let Some(reference_target) = self.reference_target(schema_node) {
            return format!("reference ({reference_target})");
        }

        "unknown".to_string()
    }

    fn union_member_types(&self, union_values: &[Value]) -> String {
        let mut rendered_member_types = union_values
            .iter()
            .map(|union_value| self.describe_value_type(union_value))
            .collect::<Vec<_>>();

        rendered_member_types.sort();
        rendered_member_types.dedup();

        rendered_member_types.join(" | ")
    }

    fn schema_description<'value>(&self, schema_node: &'value Value) -> Option<&'value str> {
        schema_node.get("description").and_then(Value::as_str)
    }

    fn reference_target<'value>(&self, schema_node: &'value Value) -> Option<&'value str> {
        schema_node.get("$ref").and_then(Value::as_str)
    }

    fn resolve_reference(&self, reference_target: &str) -> Option<&Value> {
        if !reference_target.starts_with("#/") {
            return None;
        }

        let reference_segments = reference_target
            .trim_start_matches("#/")
            .split('/')
            .map(Self::decode_reference_segment)
            .collect::<Vec<_>>();

        let mut current_schema = self.root_schema;

        for reference_segment in reference_segments {
            current_schema = current_schema.get(reference_segment.as_str())?;
        }

        Some(current_schema)
    }

    fn decode_reference_segment(reference_segment: &str) -> String {
        reference_segment.replace("~1", "/").replace("~0", "~")
    }

    fn display_value_type_keyword(type_value: &Value) -> String {
        match type_value {
            Value::String(single_type_name) => single_type_name.clone(),
            Value::Array(type_values) => {
                let type_names = type_values.iter().filter_map(Value::as_str).collect::<Vec<_>>();

                if type_names.is_empty() {
                    "unknown".to_string()
                } else {
                    type_names.join(" | ")
                }
            }
            _ => "unknown".to_string(),
        }
    }

    fn display_value(value: &Value) -> String {
        match value {
            Value::String(string_value) => format!("\"{string_value}\""),
            _ => value.to_string(),
        }
    }

    fn join_display_values(values: &[Value]) -> String {
        values.iter().map(Self::display_value).collect::<Vec<_>>().join(", ")
    }

    fn indentation(&self, depth: usize) -> String {
        "  ".repeat(depth)
    }
}

#[derive(Debug, Args)]
struct BuildToolsCommand {
    #[arg(value_name = "PATH", default_value = ".")]
    path: PathBuf,

    #[arg(long, value_name = "OUTPUT")]
    output: Option<PathBuf>,

    #[arg(long, action = clap::ArgAction::SetTrue)]
    wat: bool,

    #[arg(long, value_name = "TARGET", default_value = "wasm32-unknown-unknown")]
    target: String,
}

impl BuildToolsCommand {
    fn execute(self) -> Result<(), CommandError> {
        self.ensure_cargo_component_installed()?;

        let build_layout = self.resolve_build_layout()?;

        let tool_output_directory = build_layout.workflow_directory.join("tools");
        let generated_tools_directory = build_layout.workflow_directory.join("target/tool-build");
        let shared_target_directory = build_layout.workflow_directory.join("target/tool-target");
        let additional_dependency_entries = self.additional_dependency_entries(&build_layout.workflow_directory)?;

        fs::create_dir_all(&tool_output_directory).map_err(|error| {
            CommandError::internal(format!(
                "failed to create tools output directory {}: {error}",
                tool_output_directory.display()
            ))
        })?;

        fs::create_dir_all(&generated_tools_directory).map_err(|error| {
            CommandError::internal(format!(
                "failed to create generated tools directory {}: {error}",
                generated_tools_directory.display()
            ))
        })?;

        let output_paths_by_tool_source =
            self.output_paths_by_tool_source(&build_layout.tool_source_paths, &build_layout.workflow_directory.join("tools"))?;

        let wat_output_paths_by_tool_source = self.wat_output_paths_by_tool_source(&output_paths_by_tool_source);
        let tool_build_context = ToolBuildContext {
            generated_tools_directory: &generated_tools_directory,
            shared_target_directory: &shared_target_directory,
            additional_dependency_entries: &additional_dependency_entries,
        };

        for tool_source_path in &build_layout.tool_source_paths {
            let destination_component_path = output_paths_by_tool_source.get(tool_source_path).ok_or_else(|| {
                CommandError::internal(format!("missing destination path for tool source {}", tool_source_path.display()))
            })?;

            self.build_single_tool(
                tool_source_path,
                destination_component_path,
                wat_output_paths_by_tool_source
                    .get(tool_source_path)
                    .map(std::path::PathBuf::as_path),
                &tool_build_context,
            )?;
        }

        Ok(())
    }

    fn ensure_cargo_component_installed(&self) -> Result<(), CommandError> {
        let cargo_component_status = Command::new("cargo")
            .arg("component")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|error| CommandError::internal(format!("failed to check cargo-component installation: {error}")))?;

        if cargo_component_status.success() {
            return Ok(());
        }

        Err(CommandError::invalid_input(
            "cargo-component is required. Install with: cargo install cargo-component",
        ))
    }

    fn resolve_build_layout(&self) -> Result<BuildLayout, CommandError> {
        let canonical_path = fs::canonicalize(&self.path)
            .map_err(|_| CommandError::invalid_input(format!("path does not exist: {}", self.path.display())))?;

        if canonical_path.join("tool-sources/src").is_dir() {
            let tool_sources_directory = canonical_path.join("tool-sources/src");

            return Ok(BuildLayout {
                workflow_directory: canonical_path.clone(),
                tool_source_paths: self.tool_source_paths(&tool_sources_directory)?,
            });
        }

        if canonical_path.join("src").is_dir() {
            let tool_sources_directory = canonical_path.join("src");

            return Ok(BuildLayout {
                workflow_directory: canonical_path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf(),
                tool_source_paths: self.tool_source_paths(&tool_sources_directory)?,
            });
        }

        if canonical_path.file_name().and_then(|name| name.to_str()) == Some("src") {
            let workflow_directory = canonical_path
                .parent()
                .and_then(Path::parent)
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf();

            return Ok(BuildLayout {
                workflow_directory,
                tool_source_paths: self.tool_source_paths(&canonical_path)?,
            });
        }

        if canonical_path.is_file() {
            if canonical_path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                return Err(CommandError::invalid_input(format!(
                    "expected a .rs tool file, got: {}",
                    canonical_path.display()
                )));
            }

            let workflow_directory = canonical_path
                .parent()
                .and_then(Path::parent)
                .and_then(Path::parent)
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf();

            return Ok(BuildLayout {
                workflow_directory,
                tool_source_paths: vec![canonical_path],
            });
        }

        Err(CommandError::invalid_input(format!(
            "expected a workflow directory (containing tool-sources/src), a tool-sources directory, or a tool-sources/src directory. got: {}",
            self.path.display()
        )))
    }

    fn tool_source_paths(&self, tool_sources_directory: &Path) -> Result<Vec<PathBuf>, CommandError> {
        let mut tool_source_paths = Vec::new();

        for directory_entry_result in fs::read_dir(tool_sources_directory).map_err(|error| {
            CommandError::internal(format!(
                "failed to read tool source directory {}: {error}",
                tool_sources_directory.display()
            ))
        })? {
            let directory_entry = directory_entry_result.map_err(|error| {
                CommandError::internal(format!(
                    "failed to read entry in tool source directory {}: {error}",
                    tool_sources_directory.display()
                ))
            })?;

            let entry_path = directory_entry.path();

            if !entry_path.is_file() {
                continue;
            }

            if entry_path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                continue;
            }

            if entry_path.file_name().and_then(|name| name.to_str()) == Some("lib.rs") {
                continue;
            }

            let canonical_tool_source_path = fs::canonicalize(&entry_path).map_err(|error| {
                CommandError::internal(format!("failed to canonicalize tool source path {}: {error}", entry_path.display()))
            })?;

            tool_source_paths.push(canonical_tool_source_path);
        }

        tool_source_paths.sort();

        if tool_source_paths.is_empty() {
            return Err(CommandError::invalid_input(format!(
                "no tool source files found in {}",
                tool_sources_directory.display()
            )));
        }

        Ok(tool_source_paths)
    }

    fn output_paths_by_tool_source(
        &self,
        tool_source_paths: &[PathBuf],
        default_tool_output_directory: &Path,
    ) -> Result<HashMap<PathBuf, PathBuf>, CommandError> {
        let mut output_paths = HashMap::new();

        if let Some(output_path) = &self.output {
            if tool_source_paths.len() == 1 {
                let single_tool_source_path = tool_source_paths[0].clone();
                let resolved_output_path = if output_path.is_absolute() {
                    output_path.clone()
                } else {
                    Path::new(".").join(output_path)
                };

                output_paths.insert(single_tool_source_path, resolved_output_path);

                return Ok(output_paths);
            }

            let output_directory = if output_path.extension().and_then(|extension| extension.to_str()) == Some("wasm") {
                return Err(CommandError::invalid_input(
                    "--output points to a single .wasm file, but multiple tools were discovered. pass a directory path or build one tool file",
                ));
            } else if output_path.is_absolute() {
                output_path.clone()
            } else {
                Path::new(".").join(output_path)
            };

            fs::create_dir_all(&output_directory).map_err(|error| {
                CommandError::internal(format!("failed to create output directory {}: {error}", output_directory.display()))
            })?;

            for tool_source_path in tool_source_paths {
                let tool_name = tool_source_path
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| {
                        CommandError::internal(format!("failed to resolve tool name from source {}", tool_source_path.display()))
                    })?
                    .to_string();

                output_paths.insert(tool_source_path.clone(), output_directory.join(format!("{tool_name}.wasm")));
            }

            return Ok(output_paths);
        }

        for tool_source_path in tool_source_paths {
            let tool_name = tool_source_path
                .file_stem()
                .and_then(|name| name.to_str())
                .ok_or_else(|| CommandError::internal(format!("failed to resolve tool name from source {}", tool_source_path.display())))?
                .to_string();

            output_paths.insert(
                tool_source_path.clone(),
                default_tool_output_directory.join(format!("{tool_name}.wasm")),
            );
        }

        Ok(output_paths)
    }

    fn wat_output_paths_by_tool_source(&self, output_paths_by_tool_source: &HashMap<PathBuf, PathBuf>) -> HashMap<PathBuf, PathBuf> {
        let mut wat_output_paths_by_tool_source = HashMap::new();

        if !self.wat {
            return wat_output_paths_by_tool_source;
        }

        for (tool_source_path, wasm_output_path) in output_paths_by_tool_source {
            let mut wat_file_name = wasm_output_path
                .file_stem()
                .map_or_else(|| OsString::from("tool"), std::ffi::OsStr::to_os_string);

            wat_file_name.push(".wat");

            let wat_output_path = wasm_output_path.parent().unwrap_or_else(|| Path::new(".")).join(wat_file_name);

            wat_output_paths_by_tool_source.insert(tool_source_path.clone(), wat_output_path);
        }

        wat_output_paths_by_tool_source
    }

    fn build_single_tool(
        &self,
        tool_source_path: &Path,
        destination_component_path: &Path,
        wat_output_path: Option<&Path>,
        tool_build_context: &ToolBuildContext<'_>,
    ) -> Result<(), CommandError> {
        let tool_name = tool_source_path
            .file_stem()
            .and_then(|name| name.to_str())
            .ok_or_else(|| CommandError::internal(format!("failed to resolve tool name from {}", tool_source_path.display())))?
            .to_string();

        let tool_type_name = self.tool_type_name(&tool_name);
        let generated_tool_crate_directory = tool_build_context.generated_tools_directory.join(&tool_name);
        let generated_tool_source_directory = generated_tool_crate_directory.join("src");
        let generated_tool_wit_directory = generated_tool_crate_directory.join("wit");

        if generated_tool_crate_directory.exists() {
            fs::remove_dir_all(&generated_tool_crate_directory).map_err(|error| {
                CommandError::internal(format!(
                    "failed to clean generated tool directory {}: {error}",
                    generated_tool_crate_directory.display()
                ))
            })?;
        }

        fs::create_dir_all(&generated_tool_source_directory).map_err(|error| {
            CommandError::internal(format!(
                "failed to create generated tool source directory {}: {error}",
                generated_tool_source_directory.display()
            ))
        })?;

        write_embedded_wit_package(&generated_tool_wit_directory)?;

        let generated_cargo_manifest = self.generated_tool_cargo_manifest(&tool_name, tool_build_context.additional_dependency_entries);
        let generated_source = self.generated_tool_component_source(tool_source_path, &tool_type_name);

        fs::write(generated_tool_crate_directory.join("Cargo.toml"), generated_cargo_manifest)
            .map_err(|error| CommandError::internal(format!("failed to write generated Cargo.toml for tool `{tool_name}`: {error}")))?;

        fs::write(generated_tool_source_directory.join("lib.rs"), generated_source)
            .map_err(|error| CommandError::internal(format!("failed to write generated source for tool `{tool_name}`: {error}")))?;

        fs::write(
            generated_tool_source_directory.join("superwire_wasm_tool_sdk.rs"),
            embedded_wasm_tool_sdk_source(),
        )
        .map_err(|error| CommandError::internal(format!("failed to write embedded wasm sdk source for tool `{tool_name}`: {error}")))?;

        let build_status = Command::new("cargo")
            .arg("component")
            .arg("build")
            .arg("--manifest-path")
            .arg(generated_tool_crate_directory.join("Cargo.toml"))
            .arg("--release")
            .arg("--target")
            .arg(&self.target)
            .env("CARGO_TARGET_DIR", tool_build_context.shared_target_directory)
            .status()
            .map_err(|error| CommandError::internal(format!("failed to run cargo component build for `{tool_name}`: {error}")))?;

        if !build_status.success() {
            return Err(CommandError::internal(format!(
                "cargo component build failed for tool `{tool_name}`"
            )));
        }

        let compiled_component_path = tool_build_context
            .shared_target_directory
            .join(&self.target)
            .join("release/tool_component.wasm");

        let destination_directory = destination_component_path.parent().unwrap_or_else(|| Path::new("."));

        fs::create_dir_all(destination_directory).map_err(|error| {
            CommandError::internal(format!(
                "failed to create destination directory {}: {error}",
                destination_directory.display()
            ))
        })?;

        fs::copy(&compiled_component_path, destination_component_path).map_err(|error| {
            CommandError::internal(format!(
                "failed to copy component output from {} to {}: {error}",
                compiled_component_path.display(),
                destination_component_path.display()
            ))
        })?;

        if let Some(wat_output_path) = wat_output_path {
            let wat_output_directory = wat_output_path.parent().unwrap_or_else(|| Path::new("."));
            let wasm_binary = fs::read(destination_component_path).map_err(|error| {
                CommandError::internal(format!(
                    "failed to read built wasm binary {}: {error}",
                    destination_component_path.display()
                ))
            })?;

            let wat_source = wasmprinter::print_bytes(&wasm_binary).map_err(|error| {
                CommandError::internal(format!(
                    "failed to convert wasm binary {} to wat: {error}",
                    destination_component_path.display()
                ))
            })?;

            fs::create_dir_all(wat_output_directory).map_err(|error| {
                CommandError::internal(format!(
                    "failed to create WAT destination directory {}: {error}",
                    wat_output_directory.display()
                ))
            })?;

            fs::write(wat_output_path, wat_source)
                .map_err(|error| CommandError::internal(format!("failed to write WAT file {}: {error}", wat_output_path.display())))?;

            println!("built {}", wat_output_path.display());
        }

        println!("built {}", destination_component_path.display());

        Ok(())
    }

    fn generated_tool_cargo_manifest(&self, tool_name: &str, additional_dependency_entries: &str) -> String {
        GENERATED_TOOL_CARGO_MANIFEST_TEMPLATE
            .replace("{{tool_name}}", tool_name)
            .replace("{{extra_dependencies}}", additional_dependency_entries)
    }

    fn additional_dependency_entries(&self, workflow_directory: &Path) -> Result<String, CommandError> {
        let tool_sources_manifest_path = workflow_directory.join("tool-sources/Cargo.toml");

        if !tool_sources_manifest_path.is_file() {
            return Ok(String::new());
        }

        let tool_sources_manifest = fs::read_to_string(&tool_sources_manifest_path).map_err(|error| {
            CommandError::internal(format!(
                "failed to read tool sources manifest {}: {error}",
                tool_sources_manifest_path.display()
            ))
        })?;

        let tool_sources_manifest_directory = tool_sources_manifest_path.parent().unwrap_or_else(|| Path::new("."));

        Self::extract_dependencies_section(&tool_sources_manifest, tool_sources_manifest_directory)
    }

    fn extract_dependencies_section(tool_sources_manifest: &str, manifest_directory: &Path) -> Result<String, CommandError> {
        let mut is_inside_dependencies_section = false;
        let mut dependency_lines = Vec::new();

        for manifest_line in tool_sources_manifest.lines() {
            let trimmed_manifest_line = manifest_line.trim();

            if trimmed_manifest_line == "[dependencies]" {
                is_inside_dependencies_section = true;

                continue;
            }

            if is_inside_dependencies_section
                && trimmed_manifest_line.starts_with('[')
                && !trimmed_manifest_line.starts_with("[dependencies.")
            {
                break;
            }

            if is_inside_dependencies_section {
                dependency_lines.push(Self::normalize_dependency_line(manifest_line, manifest_directory)?);
            }
        }

        if dependency_lines.iter().all(|line| line.trim().is_empty()) {
            return Ok(String::new());
        }

        Ok(format!("\n{}", dependency_lines.join("\n")))
    }

    fn normalize_dependency_line(dependency_line: &str, manifest_directory: &Path) -> Result<String, CommandError> {
        let trimmed_dependency_line = dependency_line.trim_start();

        if trimmed_dependency_line.starts_with("superwire-wasm-tool-sdk")
            || trimmed_dependency_line.starts_with("serde_json")
            || trimmed_dependency_line.starts_with("serde")
            || trimmed_dependency_line.starts_with("schemars")
            || trimmed_dependency_line.starts_with("pollster")
        {
            return Ok(String::new());
        }

        let Some(path_assignment_index) = dependency_line.find("path = \"") else {
            return Ok(dependency_line.to_string());
        };

        let path_value_start_index = path_assignment_index + "path = \"".len();
        let path_value_end_offset = dependency_line[path_value_start_index..]
            .find('"')
            .ok_or_else(|| CommandError::invalid_input(format!("invalid dependency path entry: {dependency_line}")))?;

        let path_value_end_index = path_value_start_index + path_value_end_offset;
        let dependency_path = &dependency_line[path_value_start_index..path_value_end_index];

        let resolved_dependency_path = manifest_directory.join(dependency_path);
        let canonical_dependency_path = fs::canonicalize(&resolved_dependency_path).map_err(|error| {
            CommandError::invalid_input(format!(
                "failed to resolve dependency path `{dependency_path}` from {}: {error}",
                manifest_directory.display()
            ))
        })?;

        let mut normalized_dependency_line = dependency_line.to_string();
        normalized_dependency_line.replace_range(
            path_value_start_index..path_value_end_index,
            &canonical_dependency_path.display().to_string(),
        );

        Ok(normalized_dependency_line)
    }

    fn generated_tool_component_source(&self, tool_source_path: &Path, tool_type_name: &str) -> String {
        GENERATED_TOOL_COMPONENT_SOURCE_TEMPLATE
            .replace("{{tool_source_path}}", &tool_source_path.display().to_string())
            .replace("{{tool_type_name}}", tool_type_name)
    }

    fn tool_type_name(&self, tool_name: &str) -> String {
        InitToolsCommand::tool_type_name(tool_name)
    }
}

#[derive(Debug, Clone)]
struct BuildLayout {
    workflow_directory: PathBuf,
    tool_source_paths: Vec<PathBuf>,
}

struct ToolBuildContext<'context> {
    generated_tools_directory: &'context Path,
    shared_target_directory: &'context Path,
    additional_dependency_entries: &'context str,
}

fn write_embedded_wit_package(destination_directory: &Path) -> Result<(), CommandError> {
    fs::create_dir_all(destination_directory).map_err(|error| {
        CommandError::internal(format!(
            "failed to create destination directory {}: {error}",
            destination_directory.display()
        ))
    })?;

    fs::write(destination_directory.join("superwire-tool.wit"), EMBEDDED_TOOL_WIT_SOURCE)
        .map_err(|error| CommandError::internal(format!("failed to write embedded WIT package: {error}")))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{BuildToolsCommand, InitToolsCommand, JsonSchemaRenderer, OutputStyler};
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::{env, process};

    static TEMPORARY_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn converts_snake_case_tool_name_to_pascal_case_type_name() {
        let build_tools_command = BuildToolsCommand {
            path: PathBuf::from("."),
            output: None,
            wat: false,
            target: String::from("wasm32-unknown-unknown"),
        };

        assert_eq!(build_tools_command.tool_type_name("weather"), "Weather");
        assert_eq!(build_tools_command.tool_type_name("knowledge_base_search"), "KnowledgeBaseSearch");
    }

    #[test]
    fn initializes_rust_tool_sources_template() {
        let temporary_directory = create_temporary_workflow_directory();
        let project_directory = temporary_directory.join("scaffold");
        let init_tools_command = InitToolsCommand {
            directory: project_directory.clone(),
            tool_name: "hello_tool".to_string(),
        };

        init_tools_command.execute().expect("init command should write scaffold files");

        let manifest_path = project_directory.join("tool-sources/Cargo.toml");
        let module_path = project_directory.join("tool-sources/src/lib.rs");
        let tool_path = project_directory.join("tool-sources/src/hello_tool.rs");

        assert!(manifest_path.is_file());
        assert!(module_path.is_file());
        assert!(tool_path.is_file());

        let module_source = fs::read_to_string(module_path).expect("module source should be readable");
        let tool_source = fs::read_to_string(tool_path).expect("tool source should be readable");

        assert!(module_source.contains("pub mod hello_tool;"));
        assert!(tool_source.contains("pub struct HelloToolInput"));
        assert!(tool_source.contains("pub struct HelloToolBoundInput"));
        assert!(tool_source.contains("pub struct HelloToolOutput"));
        assert!(tool_source.contains("type AgentInput = HelloToolInput;"));
        assert!(tool_source.contains("type BoundInput = HelloToolBoundInput;"));
        assert!(tool_source.contains("type Output = HelloToolOutput;"));
    }

    #[test]
    fn rejects_invalid_tool_name_when_initializing_template() {
        let temporary_directory = create_temporary_workflow_directory();
        let project_directory = temporary_directory.join("invalid");
        let init_tools_command = InitToolsCommand {
            directory: project_directory,
            tool_name: "HelloTool".to_string(),
        };

        let command_error = init_tools_command
            .execute()
            .expect_err("init command should reject invalid tool names");

        assert!(command_error.to_string().contains("must start with a lowercase ASCII letter"));
    }

    #[test]
    fn resolves_layout_from_tool_sources_directory() {
        let workflow_directory = create_temporary_workflow_directory();
        let tool_sources_directory = workflow_directory.join("tool-sources");
        let tool_sources_src_directory = tool_sources_directory.join("src");

        fs::create_dir_all(&tool_sources_src_directory).expect("tool sources src directory should exist");

        let build_tools_command = BuildToolsCommand {
            path: tool_sources_directory.clone(),
            output: None,
            wat: false,
            target: String::from("wasm32-unknown-unknown"),
        };

        let build_layout = build_tools_command
            .resolve_build_layout()
            .expect("build layout should resolve from tool-sources directory");

        assert_eq!(build_layout.workflow_directory, workflow_directory);
        assert_eq!(build_layout.tool_source_paths.len(), 1);
        assert_eq!(build_layout.tool_source_paths[0], tool_sources_src_directory.join("weather.rs"));
    }

    #[test]
    fn renders_required_properties_from_object_schema() {
        let schema_value = json!({
            "type": "object",
            "description": "Request payload",
            "required": ["city"],
            "properties": {
                "city": {
                    "type": "string",
                    "description": "City name"
                },
                "days": {
                    "type": "integer"
                }
            }
        });

        let mut output_lines = Vec::new();
        let output_styler = OutputStyler::without_colors();
        let mut schema_renderer = JsonSchemaRenderer::from_root_schema(&schema_value, "input schema".to_string(), &output_styler);

        schema_renderer.render_into(&mut output_lines);

        let rendered_output = output_lines.join("\n");

        assert!(rendered_output.contains("input schema -> object; Request payload"));
        assert!(rendered_output.contains("city [required] -> string; City name"));
        assert!(rendered_output.contains("days -> integer"));
    }

    #[test]
    fn renders_schema_references() {
        let schema_value = json!({
            "$defs": {
                "Coordinates": {
                    "type": "object",
                    "properties": {
                        "latitude": { "type": "number" },
                        "longitude": { "type": "number" }
                    },
                    "required": ["latitude", "longitude"]
                }
            },
            "$ref": "#/$defs/Coordinates"
        });
        let mut output_lines = Vec::new();
        let output_styler = OutputStyler::without_colors();
        let mut schema_renderer = JsonSchemaRenderer::from_root_schema(&schema_value, "output schema".to_string(), &output_styler);

        schema_renderer.render_into(&mut output_lines);

        let rendered_output = output_lines.join("\n");

        assert!(rendered_output.contains("output schema -> reference (#/$defs/Coordinates)"));
        assert!(rendered_output.contains("referenced schema -> object"));
        assert!(rendered_output.contains("latitude [required] -> number"));
        assert!(rendered_output.contains("longitude [required] -> number"));
    }

    #[test]
    fn renders_enum_union_and_tuple_types() {
        let schema_value = json!({
            "type": "object",
            "properties": {
                "status": {
                    "enum": ["pending", "done"]
                },
                "payload": {
                    "oneOf": [
                        { "type": "string" },
                        { "type": "integer" }
                    ]
                },
                "coordinates": {
                    "type": "array",
                    "items": [
                        { "type": "number" },
                        { "type": "number" }
                    ]
                }
            }
        });

        let mut output_lines = Vec::new();
        let output_styler = OutputStyler::without_colors();
        let mut schema_renderer = JsonSchemaRenderer::from_root_schema(&schema_value, "input schema".to_string(), &output_styler);

        schema_renderer.render_into(&mut output_lines);

        let rendered_output = output_lines.join("\n");

        assert!(rendered_output.contains("status -> enum (\"pending\", \"done\")"));
        assert!(rendered_output.contains("payload -> oneOf (integer | string)"));
        assert!(rendered_output.contains("coordinates -> tuple[2]"));
        assert!(rendered_output.contains("item 1 -> number"));
        assert!(rendered_output.contains("item 2 -> number"));
    }

    fn create_temporary_workflow_directory() -> PathBuf {
        let sequence_value = TEMPORARY_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary_directory = env::temp_dir().join(format!("superwire-cli-tools-test-{}-{sequence_value}", process::id()));

        fs::create_dir_all(&temporary_directory).expect("temporary directory should be created");
        let tool_sources_src_directory = temporary_directory.join("tool-sources/src");

        fs::create_dir_all(&tool_sources_src_directory).expect("tool sources src directory should be created");
        fs::write(tool_sources_src_directory.join("weather.rs"), "pub struct Weather;").expect("tool source file should be created");

        temporary_directory
    }
}
