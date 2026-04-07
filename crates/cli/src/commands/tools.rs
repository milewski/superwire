use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use clap::{Args, Subcommand};

use crate::diagnostics::CommandError;

const GENERATED_TOOL_CARGO_MANIFEST_TEMPLATE: &str = include_str!("../../templates/cargo.toml.template");
const GENERATED_TOOL_COMPONENT_SOURCE_TEMPLATE: &str = include_str!("../../templates/lib.rs.template");

#[derive(Debug, Args)]
pub struct ToolsCommand {
    #[command(subcommand)]
    command: ToolsSubcommand,
}

impl ToolsCommand {
    pub fn execute(self) -> Result<(), CommandError> {
        match self.command {
            ToolsSubcommand::Build(build_tools_command) => build_tools_command.execute(),
        }
    }
}

#[derive(Debug, Subcommand)]
enum ToolsSubcommand {
    Build(BuildToolsCommand),
}

#[derive(Debug, Args)]
struct BuildToolsCommand {
    #[arg(value_name = "WORKFLOW_DIR", default_value = ".")]
    workflow_directory: PathBuf,

    #[arg(long, value_name = "TARGET", default_value = "wasm32-unknown-unknown")]
    target: String,
}

impl BuildToolsCommand {
    fn execute(self) -> Result<(), CommandError> {
        self.ensure_cargo_component_installed()?;

        let workflow_directory = self.resolve_workflow_directory()?;
        let tool_source_paths = self.tool_source_paths(&workflow_directory)?;
        let workspace_root = Self::workspace_root();

        let wit_source_directory = workspace_root.join("crates/core/wit");
        let tool_sdk_crate_directory = workspace_root.join("crates/wasm-tool-sdk");

        if !wit_source_directory.is_dir() {
            return Err(CommandError::internal(format!(
                "wit source directory not found: {}",
                wit_source_directory.display()
            )));
        }

        if !tool_sdk_crate_directory.is_dir() {
            return Err(CommandError::internal(format!(
                "tool sdk crate directory not found: {}",
                tool_sdk_crate_directory.display()
            )));
        }

        let tool_output_directory = workflow_directory.join("tools");
        let generated_tools_directory = workflow_directory.join("target/tool-build");
        let shared_target_directory = workflow_directory.join("target/tool-target");

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

        for tool_source_path in tool_source_paths {
            self.build_single_tool(
                &tool_source_path,
                &generated_tools_directory,
                &shared_target_directory,
                &tool_output_directory,
                &wit_source_directory,
                &tool_sdk_crate_directory,
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

    fn resolve_workflow_directory(&self) -> Result<PathBuf, CommandError> {
        if self.workflow_directory.is_dir() {
            return Ok(self.workflow_directory.clone());
        }

        Err(CommandError::invalid_input(format!(
            "workflow directory does not exist: {}",
            self.workflow_directory.display()
        )))
    }

    fn tool_source_paths(&self, workflow_directory: &Path) -> Result<Vec<PathBuf>, CommandError> {
        let tool_sources_directory = workflow_directory.join("tool-sources/src");

        if !tool_sources_directory.is_dir() {
            return Err(CommandError::invalid_input(format!(
                "tool source directory not found: {}",
                tool_sources_directory.display()
            )));
        }

        let mut tool_source_paths = Vec::new();

        for directory_entry_result in fs::read_dir(&tool_sources_directory).map_err(|error| {
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

            tool_source_paths.push(entry_path);
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

    fn build_single_tool(
        &self,
        tool_source_path: &Path,
        generated_tools_directory: &Path,
        shared_target_directory: &Path,
        tool_output_directory: &Path,
        wit_source_directory: &Path,
        tool_sdk_crate_directory: &Path,
    ) -> Result<(), CommandError> {
        let tool_name = tool_source_path
            .file_stem()
            .and_then(|name| name.to_str())
            .ok_or_else(|| CommandError::internal(format!("failed to resolve tool name from {}", tool_source_path.display())))?
            .to_string();

        let tool_type_name = self.tool_type_name(&tool_name);
        let generated_tool_crate_directory = generated_tools_directory.join(&tool_name);
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

        copy_directory_recursively(wit_source_directory, &generated_tool_wit_directory)?;

        let generated_cargo_manifest = self.generated_tool_cargo_manifest(&tool_name, tool_sdk_crate_directory);
        let generated_source = self.generated_tool_component_source(tool_source_path, &tool_type_name);

        fs::write(generated_tool_crate_directory.join("Cargo.toml"), generated_cargo_manifest)
            .map_err(|error| CommandError::internal(format!("failed to write generated Cargo.toml for tool `{tool_name}`: {error}")))?;

        fs::write(generated_tool_source_directory.join("lib.rs"), generated_source)
            .map_err(|error| CommandError::internal(format!("failed to write generated source for tool `{tool_name}`: {error}")))?;

        let build_status = Command::new("cargo")
            .arg("component")
            .arg("build")
            .arg("--manifest-path")
            .arg(generated_tool_crate_directory.join("Cargo.toml"))
            .arg("--release")
            .arg("--target")
            .arg(&self.target)
            .env("CARGO_TARGET_DIR", shared_target_directory)
            .status()
            .map_err(|error| CommandError::internal(format!("failed to run cargo component build for `{tool_name}`: {error}")))?;

        if !build_status.success() {
            return Err(CommandError::internal(format!(
                "cargo component build failed for tool `{tool_name}`"
            )));
        }

        let compiled_component_path = shared_target_directory.join(&self.target).join("release/tool_component.wasm");
        let destination_component_path = tool_output_directory.join(format!("{tool_name}.wasm"));

        fs::copy(&compiled_component_path, &destination_component_path).map_err(|error| {
            CommandError::internal(format!(
                "failed to copy component output from {} to {}: {error}",
                compiled_component_path.display(),
                destination_component_path.display()
            ))
        })?;

        println!("built {}", destination_component_path.display());

        Ok(())
    }

    fn generated_tool_cargo_manifest(&self, tool_name: &str, tool_sdk_crate_directory: &Path) -> String {
        GENERATED_TOOL_CARGO_MANIFEST_TEMPLATE
            .replace("{{tool_name}}", tool_name)
            .replace("{{tool_sdk_crate_path}}", &tool_sdk_crate_directory.display().to_string())
    }

    fn generated_tool_component_source(&self, tool_source_path: &Path, tool_type_name: &str) -> String {
        GENERATED_TOOL_COMPONENT_SOURCE_TEMPLATE
            .replace("{{tool_source_path}}", &tool_source_path.display().to_string())
            .replace("{{tool_type_name}}", tool_type_name)
    }

    fn tool_type_name(&self, tool_name: &str) -> String {
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

    fn workspace_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }
}

fn copy_directory_recursively(source_directory: &Path, destination_directory: &Path) -> Result<(), CommandError> {
    fs::create_dir_all(destination_directory).map_err(|error| {
        CommandError::internal(format!(
            "failed to create destination directory {}: {error}",
            destination_directory.display()
        ))
    })?;

    for directory_entry_result in fs::read_dir(source_directory)
        .map_err(|error| CommandError::internal(format!("failed to read source directory {}: {error}", source_directory.display())))?
    {
        let directory_entry =
            directory_entry_result.map_err(|error| CommandError::internal(format!("failed to read directory entry: {error}")))?;

        let entry_source_path = directory_entry.path();
        let entry_destination_path = destination_directory.join(directory_entry.file_name());

        if entry_source_path.is_dir() {
            copy_directory_recursively(&entry_source_path, &entry_destination_path)?;

            continue;
        }

        fs::copy(&entry_source_path, &entry_destination_path).map_err(|error| {
            CommandError::internal(format!(
                "failed to copy file from {} to {}: {error}",
                entry_source_path.display(),
                entry_destination_path.display()
            ))
        })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::BuildToolsCommand;
    use std::path::PathBuf;

    #[test]
    fn converts_snake_case_tool_name_to_pascal_case_type_name() {
        let build_tools_command = BuildToolsCommand {
            workflow_directory: PathBuf::from("."),
            target: String::from("wasm32-unknown-unknown"),
        };

        assert_eq!(build_tools_command.tool_type_name("weather"), "Weather");
        assert_eq!(build_tools_command.tool_type_name("knowledge_base_search"), "KnowledgeBaseSearch");
    }
}
