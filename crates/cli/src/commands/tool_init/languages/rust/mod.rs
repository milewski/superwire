use std::fs;
use std::path::{Path, PathBuf};

use crate::commands::tool_init::ToolLanguageScaffolder;
use crate::diagnostics::CommandError;

const TOOL_PROJECT_MANIFEST_TEMPLATE: &str = r#"[package]
name = "tool-sources"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"

[dependencies]
wit-bindgen = "0.55.0"

[package.metadata.component]

[package.metadata.component.target]
path = "wit"
world = "superwire-tool"

[package.metadata.component.dependencies]
"superwire:tool" = { path = "wit/deps" }
"#;
const TOOLS_LIB_TEMPLATE: &str = r"pub mod example_tool;
";
const EXAMPLE_TOOL_SOURCE_TEMPLATE: &str = r#"mod bindings {
    wit_bindgen::generate!({
        path: "wit",
        world: "superwire-tool",
        with: {
            "superwire:tool/types@0.1.0": generate,
            "superwire:tool/marker@0.1.0": generate,
        },
    });

    use super::*;

    export!(ExampleTool);
}

use bindings::exports::example::tool::tool::{BoundedInput, Guest, Input, Output, ToolError};

pub struct ExampleTool;

impl Guest for ExampleTool {
    fn execute(input: Input, bounded_input: Option<BoundedInput>) -> Result<Output, ToolError> {
        let greeting_name = input.name;

        let greeting_prefix = bounded_input
            .and_then(|context| context.prefix)
            .unwrap_or_else(|| "hello".to_string());

        let greeting_message = format!("{greeting_prefix} {greeting_name}");

        Ok(Output {
            message: greeting_message,
        })
    }
}
"#;
const SUPERWIRE_SHARED_TYPES_WIT_TEMPLATE: &str = r"package superwire:tool@0.1.0;

interface types {
    record tool-error {
        code: string,
        message: string,
    }
}

interface marker {}
";
const EXAMPLE_TOOL_WORLD_WIT_TEMPLATE: &str = r"package example:tool@0.1.0;

interface types {
    record input {
        name: string,
    }

    record bounded-input {
        prefix: option<string>,
    }

    record output {
        message: string,
    }
}

interface tool {
    use types.{input, bounded-input, output};
    use superwire:tool/types@0.1.0.{tool-error};

    execute: func(input: input, bounded-input: option<bounded-input>) -> result<output, tool-error>;
}

world superwire-tool {
    export tool;
    export superwire:tool/marker@0.1.0;
}
";

#[derive(Default)]
pub struct RustToolLanguageScaffolder;

impl RustToolLanguageScaffolder {
    pub fn new() -> Self {
        Self
    }

    fn workspace_manifest_path(&self, project_directory: &Path) -> PathBuf {
        project_directory.join("Cargo.toml")
    }

    fn tools_lib_path(&self, project_directory: &Path) -> PathBuf {
        project_directory.join("src/lib.rs")
    }

    fn example_tool_source_path(&self, project_directory: &Path) -> PathBuf {
        project_directory.join("src/example_tool.rs")
    }

    fn superwire_types_wit_path(&self, project_directory: &Path) -> PathBuf {
        project_directory.join("wit/deps/tool.wit")
    }

    fn example_tool_world_wit_path(&self, project_directory: &Path) -> PathBuf {
        project_directory.join("wit/world.wit")
    }

    fn write_file(&self, file_path: &Path, content: &str) -> Result<(), CommandError> {
        let parent_directory = file_path.parent().unwrap_or_else(|| Path::new("."));

        fs::create_dir_all(parent_directory).map_err(|error| {
            CommandError::internal(format!(
                "failed to create scaffold directory {}: {error}",
                parent_directory.display()
            ))
        })?;

        fs::write(file_path, content)
            .map_err(|error| CommandError::internal(format!("failed to write scaffold file {}: {error}", file_path.display())))
    }

    fn ensure_paths_are_available(&self, paths: &[PathBuf]) -> Result<(), CommandError> {
        for path in paths {
            if path.exists() {
                return Err(CommandError::invalid_input(format!("path already exists: {}", path.display())));
            }
        }

        Ok(())
    }
}

impl ToolLanguageScaffolder for RustToolLanguageScaffolder {
    fn scaffold(&self, project_directory: &Path) -> Result<Vec<PathBuf>, CommandError> {
        let workspace_manifest_path = self.workspace_manifest_path(project_directory);
        let tools_lib_path = self.tools_lib_path(project_directory);
        let example_tool_source_path = self.example_tool_source_path(project_directory);
        let superwire_types_wit_path = self.superwire_types_wit_path(project_directory);
        let example_tool_world_wit_path = self.example_tool_world_wit_path(project_directory);
        let created_paths = vec![
            workspace_manifest_path.clone(),
            tools_lib_path.clone(),
            example_tool_source_path.clone(),
            superwire_types_wit_path.clone(),
            example_tool_world_wit_path.clone(),
        ];

        self.ensure_paths_are_available(&created_paths)?;

        self.write_file(&workspace_manifest_path, TOOL_PROJECT_MANIFEST_TEMPLATE)?;
        self.write_file(&tools_lib_path, TOOLS_LIB_TEMPLATE)?;
        self.write_file(&example_tool_source_path, EXAMPLE_TOOL_SOURCE_TEMPLATE)?;
        self.write_file(&superwire_types_wit_path, SUPERWIRE_SHARED_TYPES_WIT_TEMPLATE)?;
        self.write_file(&example_tool_world_wit_path, EXAMPLE_TOOL_WORLD_WIT_TEMPLATE)?;

        Ok(created_paths)
    }
}
