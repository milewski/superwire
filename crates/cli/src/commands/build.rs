use std::collections::hash_map::DefaultHasher;
use std::ffi::OsString;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Args;

use crate::diagnostics::CommandError;
use crate::execution::compile_dynamic_workflow_from_source;

const GENERATED_PROJECT_NAME: &str = "engine-ai-generated-workflow";

#[derive(Debug, Args)]
pub struct BuildCommand {
    #[arg(value_name = "WORKFLOW")]
    workflow_path: PathBuf,

    #[arg(long = "output", value_name = "OUTPUT")]
    output_path: PathBuf,
}

impl BuildCommand {
    pub fn execute(self) -> Result<(), CommandError> {
        let workflow_source = self.read_workflow_source()?;

        compile_dynamic_workflow_from_source(&workflow_source)?;

        self.ensure_output_parent_directory_exists()?;

        let generated_project_directory = self.resolve_generated_project_directory(&workflow_source);
        self.materialize_generated_project(&generated_project_directory, &workflow_source)?;

        let compiled_binary_path = self.build_generated_project(&generated_project_directory)?;

        fs::copy(&compiled_binary_path, &self.output_path).map_err(|error| {
            CommandError::internal(format!(
                "failed to copy generated executable from {} to {}: {error}",
                compiled_binary_path.display(),
                self.output_path.display()
            ))
        })?;

        println!("built executable: {}", self.output_path.display());

        Ok(())
    }

    fn read_workflow_source(&self) -> Result<String, CommandError> {
        if !self.workflow_path.exists() {
            return Err(CommandError::invalid_workflow(format!(
                "workflow file does not exist: {}",
                self.workflow_path.display()
            )));
        }

        fs::read_to_string(&self.workflow_path).map_err(|io_error| {
            CommandError::internal(format!("failed to read workflow file {}: {io_error}", self.workflow_path.display()))
        })
    }

    fn ensure_output_parent_directory_exists(&self) -> Result<(), CommandError> {
        if let Some(output_parent_directory) = self.output_path.parent() {
            if !output_parent_directory.as_os_str().is_empty() && !output_parent_directory.exists() {
                return Err(CommandError::invalid_workflow(format!(
                    "output directory does not exist: {}",
                    output_parent_directory.display()
                )));
            }
        }

        Ok(())
    }

    fn resolve_generated_project_directory(&self, workflow_source: &str) -> PathBuf {
        let mut hasher = DefaultHasher::new();
        workflow_source.hash(&mut hasher);

        let workflow_hash = format!("{:016x}", hasher.finish());
        let generated_root_directory = workspace_target_directory().join("engine-ai-cli");

        generated_root_directory.join(workflow_hash)
    }

    fn materialize_generated_project(&self, generated_project_directory: &Path, workflow_source: &str) -> Result<(), CommandError> {
        let generated_source_directory = generated_project_directory.join("src");

        fs::create_dir_all(&generated_source_directory).map_err(|error| {
            CommandError::internal(format!(
                "failed to create generated build directory {}: {error}",
                generated_source_directory.display()
            ))
        })?;

        let cargo_manifest_path = generated_project_directory.join("Cargo.toml");
        let launcher_source_path = generated_source_directory.join("main.rs");

        fs::write(&cargo_manifest_path, self.render_generated_cargo_manifest()).map_err(|error| {
            CommandError::internal(format!(
                "failed to write generated cargo manifest {}: {error}",
                cargo_manifest_path.display()
            ))
        })?;

        fs::write(&launcher_source_path, self.render_generated_launcher_source(workflow_source)).map_err(|error| {
            CommandError::internal(format!(
                "failed to write generated launcher source {}: {error}",
                launcher_source_path.display()
            ))
        })?;

        Ok(())
    }

    fn render_generated_cargo_manifest(&self) -> String {
        let cli_crate_path = Path::new(env!("CARGO_MANIFEST_DIR"));
        let escaped_cli_crate_path = escape_toml_string(&cli_crate_path.to_string_lossy());

        format!(
            "[package]\nname = \"{GENERATED_PROJECT_NAME}\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n\n[dependencies]\nengine-ai-cli = {{ path = \"{escaped_cli_crate_path}\" }}\n\n[workspace]\n"
        )
    }

    fn render_generated_launcher_source(&self, workflow_source: &str) -> String {
        let workflow_source_literal = render_raw_string_literal(workflow_source);

        format!(
            "const WORKFLOW_SOURCE: &str = {workflow_source_literal};\n\nfn main() {{\n    if let Err(command_error) = engine_ai_cli::launcher::run_generated_workflow(WORKFLOW_SOURCE) {{\n        eprintln!(\"{{command_error}}\");\n        std::process::exit(command_error.exit_status_code());\n    }}\n}}\n"
        )
    }

    fn build_generated_project(&self, generated_project_directory: &Path) -> Result<PathBuf, CommandError> {
        let cargo_manifest_path = generated_project_directory.join("Cargo.toml");
        let cargo_target_directory = generated_project_directory.join("cargo-target");
        let command_output = Command::new("cargo")
            .arg("build")
            .arg("--release")
            .arg("--manifest-path")
            .arg(&cargo_manifest_path)
            .arg("--target-dir")
            .arg(&cargo_target_directory)
            .output()
            .map_err(|error| CommandError::internal(format!("failed to execute cargo build for generated executable: {error}")))?;

        if !command_output.status.success() {
            let stderr_output = String::from_utf8_lossy(&command_output.stderr);

            return Err(CommandError::internal(format!(
                "failed to compile generated executable:\n{stderr_output}"
            )));
        }

        let binary_name = executable_binary_name();
        let compiled_binary_path = cargo_target_directory.join("release").join(binary_name);

        if !compiled_binary_path.exists() {
            return Err(CommandError::internal(format!(
                "generated executable missing after build: {}",
                compiled_binary_path.display()
            )));
        }

        Ok(compiled_binary_path)
    }
}

fn workspace_target_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("target")
}

fn executable_binary_name() -> OsString {
    let mut binary_name = OsString::from(GENERATED_PROJECT_NAME);
    binary_name.push(std::env::consts::EXE_SUFFIX);

    binary_name
}

fn escape_toml_string(raw_string: &str) -> String {
    raw_string.replace('\\', "\\\\").replace('"', "\\\"")
}

fn render_raw_string_literal(raw_value: &str) -> String {
    let mut hash_count = 0;

    loop {
        let hashes = "#".repeat(hash_count);
        let terminator = format!("\"{hashes}");

        if !raw_value.contains(&terminator) {
            return format!("r{hashes}\"{raw_value}\"{hashes}");
        }

        hash_count += 1;
    }
}
