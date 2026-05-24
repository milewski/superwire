use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use clap::Args;
use serde_json::{Map, Value};
use superwire_dsl::{parse_workflow, Declaration, Workflow};
use superwire_mcp::{McpClientFactory, McpLock, McpLockResolutionContext, McpServerConfig, ProjectMcpLock, PROJECT_MCP_LOCK_FILE_NAME};

use super::json::WorkflowPayloadSources;
use super::paths::WorkflowPathTargets;
use super::prompt::WorkflowLockPrompts;
use super::vars::WorkflowVarsFile;
use crate::diagnostics::CommandError;

#[derive(Debug, Args)]
pub(super) struct LockWorkflowCommand {
    #[arg(value_name = "WORKFLOW_PATH_OR_DIRECTORY", required = true)]
    workflow_targets: Vec<PathBuf>,

    #[arg(short = 'o', long = "output", value_name = "LOCK_PATH", default_value = PROJECT_MCP_LOCK_FILE_NAME)]
    output_path: PathBuf,

    #[arg(long, value_name = "VARS_JSON_FILE", default_value = ".wire.vars")]
    vars_file: PathBuf,

    #[arg(long, value_name = "JSON")]
    input_json: Option<String>,

    #[arg(long, value_name = "INPUT_JSON_FILE")]
    input_file: Option<PathBuf>,

    #[arg(long, value_name = "JSON")]
    secrets_json: Option<String>,

    #[arg(long, value_name = "SECRETS_JSON_FILE")]
    secrets_file: Option<PathBuf>,

    #[arg(long = "set", value_name = "KEY=VALUE", number_of_values = 1)]
    set: Option<Vec<String>>,
}

impl LockWorkflowCommand {
    pub(super) fn execute_with_mcp_client_factory(self, mcp_client_factory: &dyn McpClientFactory) -> Result<(), CommandError> {
        self.payload_sources().validate()?;

        let lock_root = self
            .output_path
            .parent()
            .filter(|parent_path| !parent_path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let variables_context = self.vars_context()?;
        let command_context = self.command_context()?;
        let mut prompted_value_was_captured = false;
        let mut prompted_lock_context = None;
        let workflow_paths = self.collect_workflow_paths()?;
        let mut project_lock = self.read_existing_project_lock()?;

        for workflow_path in &workflow_paths {
            let workflow_variables_context = self.workflow_vars_context(variables_context.as_ref(), lock_root, workflow_path);
            let mut lock_context = Self::merge_contexts(workflow_variables_context, command_context.clone()).unwrap_or_default();
            let workflow_source = fs::read_to_string(workflow_path).map_err(|read_error| {
                CommandError::invalid_input(format!("failed to read workflow file {}: {read_error}", workflow_path.display()))
            })?;

            let parsed_workflow = parse_workflow(&workflow_source).map_err(|parse_error| {
                CommandError::invalid_input(parse_error.render_for_output_target(&workflow_source, &workflow_path.display().to_string()))
            })?;
            let workflow_lock_context = WorkflowLockPrompts::resolve_lock_context(&parsed_workflow, &mut lock_context)?;

            if workflow_lock_context.prompted_value_was_captured {
                prompted_value_was_captured = true;
                prompted_lock_context = Some(workflow_lock_context.lock_context.clone());
            }

            let workflow_lock = match Self::discover_workflow_lock(&parsed_workflow, workflow_lock_context.as_ref(), mcp_client_factory) {
                Ok(workflow_lock) => workflow_lock,
                Err(discover_error) => {
                    if let Some(lock_context_to_persist) = prompted_lock_context.as_ref() {
                        self.persist_prompted_lock_context_if_needed(lock_context_to_persist, prompted_value_was_captured)?;
                    }

                    return Err(discover_error);
                }
            };

            project_lock.insert_workflow_lock_with_source(lock_root, workflow_path, workflow_lock, &workflow_source);
        }

        self.persist_prompted_lock_context_if_needed(&prompted_lock_context.unwrap_or_default(), prompted_value_was_captured)?;

        project_lock.write_to_path(&self.output_path).map_err(|mcp_error| {
            CommandError::internal(format!(
                "failed to write MCP project lock {}: {mcp_error}",
                self.output_path.display()
            ))
        })?;

        println!("wrote {}", self.output_path.display());

        Ok(())
    }

    fn read_existing_project_lock(&self) -> Result<ProjectMcpLock, CommandError> {
        if !self.output_path.exists() {
            return Ok(ProjectMcpLock::empty());
        }

        ProjectMcpLock::read_from_path(&self.output_path).map_err(|read_error| {
            CommandError::invalid_input(format!(
                "failed to read existing lock file {}: {read_error}",
                self.output_path.display()
            ))
        })
    }

    fn collect_workflow_paths(&self) -> Result<Vec<PathBuf>, CommandError> {
        WorkflowPathTargets::new(&self.workflow_targets).collect()
    }

    fn vars_context(&self) -> Result<Option<WorkflowVarsFile>, CommandError> {
        let vars_file = self.effective_vars_file();

        if !vars_file.exists() {
            return Ok(None);
        }

        let vars_text = fs::read_to_string(&vars_file)
            .map_err(|read_error| CommandError::invalid_input(format!("failed to read vars file {}: {read_error}", vars_file.display())))?;
        let vars_context = serde_json::from_str::<WorkflowVarsFile>(&vars_text)
            .map_err(|parse_error| CommandError::invalid_input(format!("vars file must be valid json: {parse_error}")))?;

        Ok(Some(vars_context))
    }

    fn workflow_vars_context(
        &self,
        variables_context: Option<&WorkflowVarsFile>,
        lock_root: &Path,
        workflow_path: &Path,
    ) -> Option<McpLockResolutionContext> {
        let variables_context = variables_context?;
        let mut workflow_context = variables_context.root_context();

        if let Some(override_context) = variables_context.override_context(lock_root, workflow_path) {
            Self::merge_context_into(&mut workflow_context, override_context);
        }

        Some(workflow_context)
    }

    fn command_context(&self) -> Result<Option<McpLockResolutionContext>, CommandError> {
        let input = self.payload_sources().input_value()?;
        let secrets = self.payload_sources().secrets_value()?;

        if input.is_empty() && secrets.is_empty() {
            return Ok(None);
        }

        Ok(Some(McpLockResolutionContext {
            input: input.into_iter().collect(),
            secrets: secrets.into_iter().collect(),
            dynamic: BTreeMap::new(),
            agent_outputs: BTreeMap::new(),
            agent_contexts: BTreeMap::new(),
        }))
    }

    fn merge_contexts(
        variables_context: Option<McpLockResolutionContext>,
        command_context: Option<McpLockResolutionContext>,
    ) -> Option<McpLockResolutionContext> {
        let Some(command_context) = command_context else {
            return variables_context;
        };
        let mut merged_context = variables_context.unwrap_or_default();

        Self::merge_context_into(&mut merged_context, &command_context);

        Some(merged_context)
    }

    fn merge_context_into(base_context: &mut McpLockResolutionContext, override_context: &McpLockResolutionContext) {
        Self::merge_value_maps(&mut base_context.input, &override_context.input);
        Self::merge_value_maps(&mut base_context.secrets, &override_context.secrets);
        Self::merge_value_maps(&mut base_context.dynamic, &override_context.dynamic);
        Self::merge_value_maps(&mut base_context.agent_outputs, &override_context.agent_outputs);
        Self::merge_value_maps(&mut base_context.agent_contexts, &override_context.agent_contexts);
    }

    fn merge_value_maps(base_values: &mut BTreeMap<String, Value>, override_values: &BTreeMap<String, Value>) {
        for (field_name, override_value) in override_values {
            match (base_values.get_mut(field_name), override_value) {
                (Some(Value::Object(base_object)), Value::Object(override_object)) => {
                    Self::merge_json_objects(base_object, override_object);
                }
                _ => {
                    base_values.insert(field_name.clone(), override_value.clone());
                }
            }
        }
    }

    fn merge_json_objects(base_object: &mut Map<String, Value>, override_object: &Map<String, Value>) {
        for (field_name, override_value) in override_object {
            match (base_object.get_mut(field_name), override_value) {
                (Some(Value::Object(base_child_object)), Value::Object(override_child_object)) => {
                    Self::merge_json_objects(base_child_object, override_child_object);
                }
                _ => {
                    base_object.insert(field_name.clone(), override_value.clone());
                }
            }
        }
    }

    fn persist_prompted_lock_context_if_needed(
        &self,
        lock_context: &McpLockResolutionContext,
        prompted_value_was_captured: bool,
    ) -> Result<(), CommandError> {
        if !prompted_value_was_captured {
            return Ok(());
        }

        let vars_context_json = serde_json::to_string_pretty(lock_context)
            .map_err(|serialize_error| CommandError::internal(format!("failed to serialize vars context: {serialize_error}")))?;
        let vars_file_contents = format!("{vars_context_json}\n");
        let vars_file = self.effective_vars_file();

        if let Some(vars_file_parent) = vars_file.parent().filter(|parent_path| !parent_path.as_os_str().is_empty()) {
            fs::create_dir_all(vars_file_parent).map_err(|create_error| {
                CommandError::internal(format!(
                    "failed to create vars file directory {}: {create_error}",
                    vars_file_parent.display()
                ))
            })?;
        }

        fs::write(&vars_file, vars_file_contents).map_err(|write_error| {
            CommandError::internal(format!(
                "failed to persist prompted values to vars file {}: {write_error}",
                vars_file.display()
            ))
        })?;

        println!("updated {}", vars_file.display());

        Ok(())
    }

    fn effective_vars_file(&self) -> PathBuf {
        if self.vars_file != Path::new(".wire.vars") {
            return self.vars_file.clone();
        }

        self.output_path
            .parent()
            .filter(|parent_path| !parent_path.as_os_str().is_empty())
            .map_or_else(|| self.vars_file.clone(), |parent_path| parent_path.join(".wire.vars"))
    }

    fn payload_sources(&self) -> WorkflowPayloadSources<'_> {
        WorkflowPayloadSources::new(
            self.input_json.as_deref(),
            self.input_file.as_deref(),
            self.secrets_json.as_deref(),
            self.secrets_file.as_deref(),
            self.set.as_deref(),
        )
    }

    fn discover_workflow_lock(
        parsed_workflow: &Workflow,
        lock_context: Option<&McpLockResolutionContext>,
        mcp_client_factory: &dyn McpClientFactory,
    ) -> Result<McpLock, CommandError> {
        if lock_context.is_none() {
            let unresolved_server_names = Self::unresolved_mcp_server_names(parsed_workflow);

            if !unresolved_server_names.is_empty() {
                return Err(CommandError::invalid_input(format!(
                    "failed to discover MCP typings: MCP servers require runtime values for endpoint/headers: {}. Provide values in .wire.vars or pass --vars-file, --secrets-json, --input-json, or --set",
                    unresolved_server_names.join(", ")
                )));
            }
        }

        McpLock::discover_from_workflow_with_lock_context_and_client_factory(parsed_workflow, lock_context, mcp_client_factory).map_err(|mcp_error| {
            CommandError::invalid_input(format!(
                "failed to discover MCP typings; provide dynamic values with --vars-file .wire.vars, --input-json, --secrets-json, or --set: {mcp_error}"
            ))
        })
    }

    fn unresolved_mcp_server_names(parsed_workflow: &Workflow) -> Vec<String> {
        let mut unresolved_server_names = Vec::new();

        for declaration in parsed_workflow.declarations() {
            let Declaration::McpServer(mcp_server_declaration) = declaration else {
                continue;
            };

            if McpServerConfig::from_declaration(mcp_server_declaration).is_none() {
                unresolved_server_names.push(mcp_server_declaration.name.clone());
            }
        }

        unresolved_server_names
    }
}
