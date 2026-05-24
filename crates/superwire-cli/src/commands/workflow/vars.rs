use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use superwire_core::dsl::{parse_workflow, TypedField, Workflow};
use superwire_core::mcp::McpLockResolutionContext;

use super::paths::WorkflowPathTargets;
use crate::diagnostics::CommandError;

#[derive(Debug, Args)]
pub(super) struct VarsWorkflowCommand {
    #[arg(value_name = "WORKFLOW_PATH_OR_DIRECTORY", required = true)]
    workflow_targets: Vec<PathBuf>,

    #[arg(short = 'o', long = "output", value_name = "VARS_PATH", default_value = ".wire.vars")]
    output_path: PathBuf,
}

impl VarsWorkflowCommand {
    pub(super) fn execute(self) -> Result<(), CommandError> {
        let workflow_paths = self.collect_workflow_paths()?;
        let mut generated_vars_file = WorkflowVarsFile::default();
        let mut generation_errors = Vec::new();

        for workflow_path in &workflow_paths {
            let workflow_source = match fs::read_to_string(workflow_path) {
                Ok(workflow_source) => workflow_source,
                Err(read_error) => {
                    generation_errors.push(format!("failed to read workflow file {}: {read_error}", workflow_path.display()));
                    continue;
                }
            };

            let parsed_workflow = match parse_workflow(&workflow_source) {
                Ok(parsed_workflow) => parsed_workflow,
                Err(parse_error) => {
                    generation_errors.push(parse_error.render_for_output_target(&workflow_source, &workflow_path.display().to_string()));
                    continue;
                }
            };

            Self::merge_generated_fields_from_workflow(&mut generated_vars_file, &parsed_workflow);
        }

        self.write_vars_file(&generated_vars_file)?;

        if generation_errors.is_empty() {
            println!("wrote {}", self.output_path.display());

            return Ok(());
        }

        Err(CommandError::invalid_input(format!(
            "generated {} with partial values, but found workflow errors:\n{}",
            self.output_path.display(),
            generation_errors.join("\n")
        )))
    }

    fn collect_workflow_paths(&self) -> Result<Vec<PathBuf>, CommandError> {
        WorkflowPathTargets::new(&self.workflow_targets).collect()
    }

    fn merge_generated_fields_from_workflow(generated_vars_file: &mut WorkflowVarsFile, parsed_workflow: &Workflow) {
        if let Some(input_declaration) = parsed_workflow.find_input() {
            Self::merge_typed_fields_into_values(&input_declaration.fields, parsed_workflow, &mut generated_vars_file.input);
        }

        if let Some(secrets_declaration) = parsed_workflow.find_secrets() {
            Self::merge_typed_fields_into_values(&secrets_declaration.fields, parsed_workflow, &mut generated_vars_file.secrets);
        }
    }

    fn merge_typed_fields_into_values(typed_fields: &[TypedField], parsed_workflow: &Workflow, values: &mut BTreeMap<String, Value>) {
        for typed_field in typed_fields {
            if values.contains_key(&typed_field.name) {
                continue;
            }

            let generated_value = typed_field.field_type.sample_json_value(parsed_workflow);
            values.insert(typed_field.name.clone(), generated_value);
        }
    }

    fn write_vars_file(&self, vars_file: &WorkflowVarsFile) -> Result<(), CommandError> {
        let vars_json = serde_json::to_string_pretty(vars_file)
            .map_err(|serialize_error| CommandError::internal(format!("failed to serialize vars file: {serialize_error}")))?;
        let vars_file_contents = format!("{vars_json}\n");

        if let Some(parent_directory) = self.output_path.parent().filter(|parent_path| !parent_path.as_os_str().is_empty()) {
            fs::create_dir_all(parent_directory).map_err(|create_error| {
                CommandError::internal(format!(
                    "failed to create vars file directory {}: {create_error}",
                    parent_directory.display()
                ))
            })?;
        }

        fs::write(&self.output_path, vars_file_contents).map_err(|write_error| {
            CommandError::internal(format!("failed to write vars file {}: {write_error}", self.output_path.display()))
        })
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub(super) struct WorkflowVarsFile {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    input: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    secrets: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    dynamic: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    agent_outputs: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    agent_contexts: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    overrides: BTreeMap<String, McpLockResolutionContext>,
}

impl WorkflowVarsFile {
    pub(super) fn root_context(&self) -> McpLockResolutionContext {
        McpLockResolutionContext {
            input: self.input.clone(),
            secrets: self.secrets.clone(),
            dynamic: self.dynamic.clone(),
            agent_outputs: self.agent_outputs.clone(),
            agent_contexts: self.agent_contexts.clone(),
        }
    }

    pub(super) fn override_context(&self, lock_root: &Path, workflow_path: &Path) -> Option<&McpLockResolutionContext> {
        self.override_path_candidates(lock_root, workflow_path)
            .iter()
            .find_map(|path_candidate| self.overrides.get(path_candidate))
    }

    fn override_path_candidates(&self, lock_root: &Path, workflow_path: &Path) -> Vec<String> {
        let mut candidates = Vec::new();
        Self::push_path_candidate(&mut candidates, workflow_path);

        if let Ok(canonical_workflow_path) = workflow_path.canonicalize() {
            Self::push_path_candidate(&mut candidates, &canonical_workflow_path);
        }

        if let Ok(relative_workflow_path) = workflow_path.strip_prefix(lock_root) {
            Self::push_path_candidate(&mut candidates, relative_workflow_path);
        }

        let normalized_lock_root = lock_root.canonicalize().unwrap_or_else(|_error| lock_root.to_path_buf());
        let normalized_workflow_path = workflow_path.canonicalize().unwrap_or_else(|_error| workflow_path.to_path_buf());

        if let Ok(relative_workflow_path) = normalized_workflow_path.strip_prefix(&normalized_lock_root) {
            Self::push_path_candidate(&mut candidates, relative_workflow_path);
        }

        candidates
    }

    fn push_path_candidate(candidates: &mut Vec<String>, path: &Path) {
        let path_candidate = path.to_string_lossy().replace('\\', "/");

        if !candidates.contains(&path_candidate) {
            candidates.push(path_candidate);
        }
    }
}
