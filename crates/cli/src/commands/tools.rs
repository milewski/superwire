use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::{collections::HashMap, ffi::OsString};

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
        let workspace_root = Self::workspace_root();

        let wit_source_directory = workspace_root.join("crates/core/wit");

        if !wit_source_directory.is_dir() {
            return Err(CommandError::internal(format!(
                "wit source directory not found: {}",
                wit_source_directory.display()
            )));
        }

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

        for tool_source_path in &build_layout.tool_source_paths {
            let destination_component_path = output_paths_by_tool_source.get(tool_source_path).ok_or_else(|| {
                CommandError::internal(format!("missing destination path for tool source {}", tool_source_path.display()))
            })?;

            self.build_single_tool(
                tool_source_path,
                &generated_tools_directory,
                &shared_target_directory,
                destination_component_path,
                wat_output_paths_by_tool_source.get(tool_source_path).map(|path| path.as_path()),
                &wit_source_directory,
                &additional_dependency_entries,
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
                .map(std::ffi::OsStr::to_os_string)
                .unwrap_or_else(|| OsString::from("tool"));

            wat_file_name.push(".wat");

            let wat_output_path = wasm_output_path.parent().unwrap_or_else(|| Path::new(".")).join(wat_file_name);

            wat_output_paths_by_tool_source.insert(tool_source_path.clone(), wat_output_path);
        }

        wat_output_paths_by_tool_source
    }

    fn build_single_tool(
        &self,
        tool_source_path: &Path,
        generated_tools_directory: &Path,
        shared_target_directory: &Path,
        destination_component_path: &Path,
        wat_output_path: Option<&Path>,
        wit_source_directory: &Path,
        additional_dependency_entries: &str,
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

        let generated_cargo_manifest = self.generated_tool_cargo_manifest(&tool_name, additional_dependency_entries);
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

#[derive(Debug, Clone)]
struct BuildLayout {
    workflow_directory: PathBuf,
    tool_source_paths: Vec<PathBuf>,
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
