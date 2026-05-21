use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use schemars::Schema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use superwire_core::dsl::{parse_workflow, Declaration, ObjectField, TypeExpression, TypedField, Workflow};
use superwire_core::mcp::{McpLock, McpLockResolutionContext, McpServerConfig, ProjectMcpLock, PROJECT_MCP_LOCK_FILE_NAME};
use superwire_core::semantic::support::type_inference::{infer_expression_type, TypeInferenceContext};
use superwire_core::semantic::support::types::{workflow_type_from_dsl, workflow_type_to_json_schema, WorkflowType};
use superwire_executor::{CerseiModelProvider, ExecutorError, WorkflowExecutor};

use crate::diagnostics::CommandError;

mod paths;

use paths::WorkflowPathTargets;

#[derive(Debug, Args)]
pub struct WorkflowCommand {
    #[command(subcommand)]
    command: WorkflowSubcommand,
}

impl WorkflowCommand {
    pub fn execute(self) -> Result<(), CommandError> {
        match self.command {
            WorkflowSubcommand::Check(check_workflow_command) => check_workflow_command.execute(),
            WorkflowSubcommand::Run(run_workflow_command) => run_workflow_command.execute(),
            WorkflowSubcommand::Lock(lock_workflow_command) => lock_workflow_command.execute(),
            WorkflowSubcommand::Vars(vars_workflow_command) => vars_workflow_command.execute(),
        }
    }
}

#[derive(Debug, Subcommand)]
enum WorkflowSubcommand {
    Check(CheckWorkflowCommand),
    Run(RunWorkflowCommand),
    #[command(
        after_help = "Examples:\n  superwire-cli workflow lock .\n  superwire-cli workflow lock workflows/*.wire --vars-file .wire.vars --output superwire.lock"
    )]
    Lock(LockWorkflowCommand),
    #[command(
        after_help = "Examples:\n  superwire-cli workflow vars .\n  superwire-cli workflow vars app/Services/Agent --output .wire.vars"
    )]
    Vars(VarsWorkflowCommand),
}

#[derive(Debug, Args)]
struct VarsWorkflowCommand {
    #[arg(value_name = "WORKFLOW_PATH_OR_DIRECTORY", required = true)]
    workflow_targets: Vec<PathBuf>,

    #[arg(short = 'o', long = "output", value_name = "VARS_PATH", default_value = ".wire.vars")]
    output_path: PathBuf,
}

impl VarsWorkflowCommand {
    fn execute(self) -> Result<(), CommandError> {
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

            let generated_value = Self::generate_value_from_type_expression(parsed_workflow, &typed_field.field_type);
            values.insert(typed_field.name.clone(), generated_value);
        }
    }

    fn generate_value_from_type_expression(parsed_workflow: &Workflow, type_expression: &TypeExpression) -> Value {
        match type_expression {
            TypeExpression::String | TypeExpression::StringEnum(_) | TypeExpression::StringEnumReference(_) => Value::String(String::new()),
            TypeExpression::Number => Value::Number(0.into()),
            TypeExpression::Float => Value::Number(serde_json::Number::from(0)),
            TypeExpression::Boolean => Value::Bool(false),
            TypeExpression::Null => Value::Null,
            TypeExpression::AnyObject => Value::Object(Map::new()),
            TypeExpression::Object(object_fields) => {
                let mut object_values = Map::new();

                for object_field in object_fields {
                    object_values.insert(
                        object_field.name.clone(),
                        Self::generate_value_from_type_expression(parsed_workflow, &object_field.field_type),
                    );
                }

                Value::Object(object_values)
            }
            TypeExpression::SchemaReference(schema_name) => {
                if let Some(schema_declaration) = parsed_workflow.find_schema(schema_name) {
                    if let Some(root_variant) = &schema_declaration.root_variant {
                        return Self::generate_value_from_type_expression(parsed_workflow, root_variant);
                    }
                }

                let mut object_values = Map::new();

                if let Some(schema_declaration) = parsed_workflow.find_schema(schema_name) {
                    for object_field in &schema_declaration.fields {
                        object_values.insert(
                            object_field.name.clone(),
                            Self::generate_value_from_type_expression(parsed_workflow, &object_field.field_type),
                        );
                    }
                }

                Value::Object(object_values)
            }
            TypeExpression::Union(type_expressions) => {
                if let Some(non_null_type_expression) = type_expressions
                    .iter()
                    .find(|candidate_type_expression| !matches!(candidate_type_expression, TypeExpression::Null))
                {
                    return Self::generate_value_from_type_expression(parsed_workflow, non_null_type_expression);
                }

                Value::Null
            }
            TypeExpression::Variant { discriminator, cases } => {
                let Some(first_case) = cases.first() else {
                    return Value::Object(Map::new());
                };
                let mut object_values = Map::new();
                object_values.insert(discriminator.clone(), Value::String(first_case.name.clone()));

                for object_field in &first_case.fields {
                    object_values.insert(
                        object_field.name.clone(),
                        Self::generate_value_from_type_expression(parsed_workflow, &object_field.field_type),
                    );
                }

                Value::Object(object_values)
            }
            TypeExpression::Array {
                item_type: _,
                fixed_length: _,
            }
            | TypeExpression::Tuple(_) => Value::Array(Vec::new()),
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

#[derive(Debug, Args)]
struct CheckWorkflowCommand {
    #[arg(value_name = "WORKFLOW_PATH")]
    workflow_path: PathBuf,
}

impl CheckWorkflowCommand {
    fn execute(self) -> Result<(), CommandError> {
        let workflow_source = fs::read_to_string(&self.workflow_path).map_err(|read_error| {
            CommandError::invalid_input(format!(
                "failed to read workflow file {}: {read_error}",
                self.workflow_path.display()
            ))
        })?;

        let parsed_workflow = parse_workflow(&workflow_source).map_err(|parse_error| {
            CommandError::invalid_input(parse_error.render_for_output_target(&workflow_source, &self.workflow_path.display().to_string()))
        })?;

        let _runtime_schema_context = CliRuntimeSchemaContext::from_workflow(&parsed_workflow)
            .map_err(|schema_context_error| CommandError::invalid_input(schema_context_error.message().to_string()))?;
        WorkflowExecutor::from_source(&workflow_source).map_err(Self::map_workflow_runtime_error)?;

        println!("workflow is valid");

        Ok(())
    }

    fn map_workflow_runtime_error(runtime_error: ExecutorError) -> CommandError {
        CommandError::invalid_input(runtime_error.to_string())
    }
}

#[derive(Debug, Args)]
struct RunWorkflowCommand {
    #[arg(value_name = "WORKFLOW_PATH")]
    workflow_path: PathBuf,

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

    #[arg(long, action = clap::ArgAction::SetTrue)]
    pretty: bool,
}

impl RunWorkflowCommand {
    fn execute(self) -> Result<(), CommandError> {
        self.validate_payload_arguments()?;

        let input_value = self.input_value()?;
        let secrets_value = self.secrets_value()?;

        let async_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| CommandError::internal(format!("failed to build tokio runtime: {error}")))?;

        let workflow_source = fs::read_to_string(&self.workflow_path)
            .map_err(|error| CommandError::internal(format!("failed to read workflow file {}: {error}", self.workflow_path.display())))?;

        let parsed_workflow = parse_workflow(&workflow_source).map_err(|error| {
            CommandError::internal(error.render_for_output_target(&workflow_source, &self.workflow_path.display().to_string()))
        })?;

        let _runtime_schema_context = CliRuntimeSchemaContext::from_workflow(&parsed_workflow)?;
        let workflow_executor =
            WorkflowExecutor::from_source(&workflow_source).map_err(|error| CommandError::internal(error.to_string()))?;

        let output_value = async_runtime
            .block_on(workflow_executor.execute(
                Value::Object(input_value),
                Value::Object(secrets_value),
                &CerseiModelProvider,
                None,
                10,
            ))
            .map_err(Self::map_workflow_runtime_error)?;

        if self.pretty {
            println!(
                "{}",
                serde_json::to_string_pretty(&output_value)
                    .map_err(|error| CommandError::internal(format!("failed to serialize pretty workflow output: {error}")))?
            );

            return Ok(());
        }

        println!(
            "{}",
            serde_json::to_string(&output_value)
                .map_err(|error| CommandError::internal(format!("failed to serialize workflow output: {error}")))?
        );

        Ok(())
    }

    fn validate_payload_arguments(&self) -> Result<(), CommandError> {
        if self.input_json.is_some() && self.input_file.is_some() {
            return Err(CommandError::invalid_input("use either --input-json or --input-file, not both"));
        }

        if self.input_json.is_some() && self.set.is_some() {
            return Err(CommandError::invalid_input("use either --input-json or --set, not both"));
        }

        if self.input_file.is_some() && self.set.is_some() {
            return Err(CommandError::invalid_input("use either --input-file or --set, not both"));
        }

        if self.secrets_json.is_some() && self.secrets_file.is_some() {
            return Err(CommandError::invalid_input("use either --secrets-json or --secrets-file, not both"));
        }

        Ok(())
    }

    fn input_value(&self) -> Result<Map<String, Value>, CommandError> {
        let base_payload = self.payload_as_object(self.input_json.as_deref(), self.input_file.as_deref(), "input payload")?;
        self.apply_dot_params(base_payload)
    }

    fn secrets_value(&self) -> Result<Map<String, Value>, CommandError> {
        self.payload_as_object(self.secrets_json.as_deref(), self.secrets_file.as_deref(), "secrets payload")
    }

    fn apply_dot_params(&self, mut payload: Map<String, Value>) -> Result<Map<String, Value>, CommandError> {
        let Some(set_args) = &self.set else {
            return Ok(payload);
        };

        for key_value_pair in set_args {
            let Some((key, value)) = key_value_pair.split_once('=') else {
                return Err(CommandError::invalid_input(format!(
                    "invalid --set format: expected KEY=VALUE, got '{key_value_pair}'"
                )));
            };

            let key = key.trim();
            let value = value.trim();

            let mut current = &mut payload;
            let parts: Vec<&str> = key.split('.').collect();

            for (i, part) in parts.iter().enumerate() {
                let is_last = i == parts.len() - 1;

                if is_last {
                    current.insert(part.to_string(), Value::String(value.to_string()));
                } else {
                    if !current.contains_key(*part) {
                        current.insert(part.to_string(), Value::Object(Map::new()));
                    }
                    let Some(obj) = current.get_mut(*part).and_then(|v| v.as_object_mut()) else {
                        return Err(CommandError::invalid_input(format!(
                            "cannot set nested value on non-object path: {key}"
                        )));
                    };
                    current = obj;
                }
            }
        }

        Ok(payload)
    }

    fn payload_as_object(
        &self,
        inline_payload: Option<&str>,
        payload_file_path: Option<&Path>,
        payload_label: &str,
    ) -> Result<Map<String, Value>, CommandError> {
        let payload_json = if let Some(inline_payload) = inline_payload {
            inline_payload.to_string()
        } else if let Some(payload_file_path) = payload_file_path {
            fs::read_to_string(payload_file_path).map_err(|error| {
                CommandError::invalid_input(format!(
                    "failed to read {payload_label} file {}: {error}",
                    payload_file_path.display()
                ))
            })?
        } else {
            "{}".to_string()
        };

        let parsed_payload_value = serde_json::from_str::<Value>(&payload_json)
            .map_err(|error| CommandError::invalid_input(format!("{payload_label} must be valid json: {error}")))?;

        let Some(parsed_payload_object) = parsed_payload_value.as_object() else {
            return Err(CommandError::invalid_input(format!("{payload_label} must be a json object")));
        };

        Ok(parsed_payload_object.clone())
    }
}

impl RunWorkflowCommand {
    fn map_workflow_runtime_error(error: ExecutorError) -> CommandError {
        CommandError::internal_with_details(
            error.to_string(),
            json!({
                "type": "workflow_runtime_error",
                "error": error.to_string(),
            }),
        )
    }
}

#[derive(Debug, Args)]
struct LockWorkflowCommand {
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

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct WorkflowVarsFile {
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
    fn root_context(&self) -> McpLockResolutionContext {
        McpLockResolutionContext {
            input: self.input.clone(),
            secrets: self.secrets.clone(),
            dynamic: self.dynamic.clone(),
            agent_outputs: self.agent_outputs.clone(),
            agent_contexts: self.agent_contexts.clone(),
        }
    }

    fn override_context(&self, lock_root: &Path, workflow_path: &Path) -> Option<&McpLockResolutionContext> {
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

impl LockWorkflowCommand {
    fn execute(self) -> Result<(), CommandError> {
        self.validate_payload_arguments()?;

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
            let workflow_lock_context = self.resolve_lock_context_with_prompts(&parsed_workflow, &mut lock_context)?;

            if workflow_lock_context.prompted_value_was_captured {
                prompted_value_was_captured = true;
                prompted_lock_context = Some(workflow_lock_context.lock_context.clone());
            }

            let workflow_lock = match Self::discover_workflow_lock(&parsed_workflow, workflow_lock_context.as_ref()) {
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

    fn validate_payload_arguments(&self) -> Result<(), CommandError> {
        if self.input_json.is_some() && self.input_file.is_some() {
            return Err(CommandError::invalid_input("use either --input-json or --input-file, not both"));
        }

        if self.input_json.is_some() && self.set.is_some() {
            return Err(CommandError::invalid_input("use either --input-json or --set, not both"));
        }

        if self.input_file.is_some() && self.set.is_some() {
            return Err(CommandError::invalid_input("use either --input-file or --set, not both"));
        }

        if self.secrets_json.is_some() && self.secrets_file.is_some() {
            return Err(CommandError::invalid_input("use either --secrets-json or --secrets-file, not both"));
        }

        Ok(())
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
        let input = self.input_value()?;
        let secrets = self.secrets_value()?;

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

    fn resolve_lock_context_with_prompts(
        &self,
        parsed_workflow: &Workflow,
        lock_context: &mut McpLockResolutionContext,
    ) -> Result<PromptedLockContext, CommandError> {
        let mut prompted_value_was_captured = false;

        if let Some(input_declaration) = parsed_workflow.find_input() {
            if self.prompt_for_missing_fields(parsed_workflow, "input", &input_declaration.fields, &mut lock_context.input)? {
                prompted_value_was_captured = true;
            }
        }

        if let Some(secrets_declaration) = parsed_workflow.find_secrets() {
            if self.prompt_for_missing_fields(parsed_workflow, "secrets", &secrets_declaration.fields, &mut lock_context.secrets)? {
                prompted_value_was_captured = true;
            }
        }

        Ok(PromptedLockContext {
            lock_context: lock_context.clone(),
            prompted_value_was_captured,
        })
    }

    fn prompt_for_missing_fields(
        &self,
        parsed_workflow: &Workflow,
        section_name: &str,
        typed_fields: &[TypedField],
        existing_values: &mut BTreeMap<String, Value>,
    ) -> Result<bool, CommandError> {
        let mut prompted_value_was_captured = false;

        for typed_field in typed_fields {
            let field_path = typed_field.name.clone();
            let existing_value = existing_values.remove(&typed_field.name);
            let (field_value, field_value_was_prompted) =
                self.prompt_for_missing_value(parsed_workflow, section_name, &field_path, &typed_field.field_type, existing_value)?;

            existing_values.insert(typed_field.name.clone(), field_value);

            if field_value_was_prompted {
                prompted_value_was_captured = true;
            }
        }

        Ok(prompted_value_was_captured)
    }

    fn prompt_for_missing_value(
        &self,
        parsed_workflow: &Workflow,
        section_name: &str,
        field_path: &str,
        type_expression: &TypeExpression,
        existing_value: Option<Value>,
    ) -> Result<(Value, bool), CommandError> {
        if let Some(object_fields) = Self::object_fields_for_prompt(parsed_workflow, type_expression) {
            let mut object_value = match existing_value {
                Some(Value::Object(object_value)) => object_value,
                Some(existing_value) => {
                    return Err(CommandError::invalid_input(format!(
                        "invalid value for {section_name}.{field_path}: expected object, got {}",
                        Self::json_value_type_label(&existing_value)
                    )));
                }
                None => Map::new(),
            };
            let mut prompted_value_was_captured = false;

            for object_field in object_fields {
                let child_field_path = format!("{field_path}.{}", object_field.name);
                let existing_child_value = object_value.remove(&object_field.name);
                let (child_value, child_value_was_prompted) = self.prompt_for_missing_value(
                    parsed_workflow,
                    section_name,
                    &child_field_path,
                    &object_field.field_type,
                    existing_child_value,
                )?;

                object_value.insert(object_field.name.clone(), child_value);

                if child_value_was_prompted {
                    prompted_value_was_captured = true;
                }
            }

            return Ok((Value::Object(object_value), prompted_value_was_captured));
        }

        if let Some(existing_value) = existing_value {
            return Ok((existing_value, false));
        }

        let field_value = self.prompt_for_field_value(section_name, field_path, type_expression)?;

        Ok((field_value, true))
    }

    fn object_fields_for_prompt<'workflow>(
        parsed_workflow: &'workflow Workflow,
        type_expression: &'workflow TypeExpression,
    ) -> Option<&'workflow [TypedField]> {
        match type_expression {
            TypeExpression::Object(object_fields) => Some(object_fields),
            TypeExpression::SchemaReference(schema_name) => {
                let schema_declaration = parsed_workflow.find_schema(schema_name)?;

                if schema_declaration.root_variant.is_some() {
                    return None;
                }

                Some(schema_declaration.fields.as_slice())
            }
            TypeExpression::Union(type_expressions) => {
                let mut object_fields = None;

                for type_expression in type_expressions {
                    if matches!(type_expression, TypeExpression::Null) {
                        continue;
                    }

                    if object_fields.is_some() {
                        return None;
                    }

                    object_fields = Self::object_fields_for_prompt(parsed_workflow, type_expression);
                }

                object_fields
            }
            TypeExpression::String
            | TypeExpression::Number
            | TypeExpression::Float
            | TypeExpression::Boolean
            | TypeExpression::Null
            | TypeExpression::AnyObject
            | TypeExpression::StringEnum(_)
            | TypeExpression::StringEnumReference(_)
            | TypeExpression::Array {
                item_type: _,
                fixed_length: _,
            }
            | TypeExpression::Tuple(_)
            | TypeExpression::Variant {
                discriminator: _,
                cases: _,
            } => None,
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

    fn prompt_for_field_value(
        &self,
        section_name: &str,
        field_path: &str,
        type_expression: &TypeExpression,
    ) -> Result<Value, CommandError> {
        if !io::stdin().is_terminal() {
            return Err(CommandError::invalid_input(format!(
                "missing {section_name}.{field_path} and terminal is non-interactive; provide it via .wire.vars, --vars-file, --input-json, --secrets-json, or --set"
            )));
        }

        let type_expression_label = Self::type_expression_label(type_expression);
        let prompt_message = format!("missing {section_name}.{field_path} ({type_expression_label}) - enter value: ");

        print!("{prompt_message}");

        io::stdout()
            .flush()
            .map_err(|flush_error| CommandError::internal(format!("failed to flush prompt output: {flush_error}")))?;

        let mut input_buffer = String::new();

        io::stdin()
            .read_line(&mut input_buffer)
            .map_err(|read_error| CommandError::internal(format!("failed to read prompt input: {read_error}")))?;

        let trimmed_input = input_buffer.trim();

        if trimmed_input.is_empty() {
            return Err(CommandError::invalid_input(format!(
                "missing {section_name}.{field_path}; empty value is not allowed"
            )));
        }

        Self::parse_prompt_value(trimmed_input, type_expression, section_name, field_path)
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

    fn json_value_type_label(value: &Value) -> &'static str {
        match value {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        }
    }

    fn type_expression_label(type_expression: &TypeExpression) -> &'static str {
        match type_expression {
            TypeExpression::String | TypeExpression::StringEnum(_) | TypeExpression::StringEnumReference(_) => "string",
            TypeExpression::Number => "integer",
            TypeExpression::Float => "float",
            TypeExpression::Boolean => "boolean",
            TypeExpression::Null => "null",
            TypeExpression::AnyObject => "json",
            TypeExpression::SchemaReference(_)
            | TypeExpression::Array {
                item_type: _,
                fixed_length: _,
            }
            | TypeExpression::Tuple(_)
            | TypeExpression::Object(_)
            | TypeExpression::Variant {
                discriminator: _,
                cases: _,
            }
            | TypeExpression::Union(_) => "json",
        }
    }

    fn parse_prompt_value(
        input_text: &str,
        type_expression: &TypeExpression,
        section_name: &str,
        field_name: &str,
    ) -> Result<Value, CommandError> {
        match type_expression {
            TypeExpression::String | TypeExpression::StringEnum(_) | TypeExpression::StringEnumReference(_) => {
                Ok(Value::String(input_text.to_string()))
            }
            TypeExpression::Number => {
                let parsed_integer = input_text.parse::<i64>().map_err(|parse_error| {
                    CommandError::invalid_input(format!("invalid integer for {section_name}.{field_name}: {parse_error}"))
                })?;

                Ok(Value::Number(parsed_integer.into()))
            }
            TypeExpression::Float => {
                let parsed_float = input_text.parse::<f64>().map_err(|parse_error| {
                    CommandError::invalid_input(format!("invalid float for {section_name}.{field_name}: {parse_error}"))
                })?;
                let Some(parsed_number) = serde_json::Number::from_f64(parsed_float) else {
                    return Err(CommandError::invalid_input(format!(
                        "invalid float for {section_name}.{field_name}: value must be finite"
                    )));
                };

                Ok(Value::Number(parsed_number))
            }
            TypeExpression::Boolean => {
                let parsed_boolean = input_text.parse::<bool>().map_err(|parse_error| {
                    CommandError::invalid_input(format!("invalid boolean for {section_name}.{field_name}: {parse_error}"))
                })?;

                Ok(Value::Bool(parsed_boolean))
            }
            TypeExpression::Null => {
                if input_text != "null" {
                    return Err(CommandError::invalid_input(format!(
                        "invalid null for {section_name}.{field_name}: expected literal `null`"
                    )));
                }

                Ok(Value::Null)
            }
            TypeExpression::AnyObject
            | TypeExpression::Variant {
                discriminator: _,
                cases: _,
            }
            | TypeExpression::SchemaReference(_)
            | TypeExpression::Array {
                item_type: _,
                fixed_length: _,
            }
            | TypeExpression::Tuple(_)
            | TypeExpression::Object(_)
            | TypeExpression::Union(_) => serde_json::from_str::<Value>(input_text)
                .map_err(|parse_error| CommandError::invalid_input(format!("invalid json for {section_name}.{field_name}: {parse_error}"))),
        }
    }

    fn input_value(&self) -> Result<Map<String, Value>, CommandError> {
        let base_payload = self.payload_as_object(self.input_json.as_deref(), self.input_file.as_deref(), "input payload")?;
        self.apply_dot_params(base_payload)
    }

    fn secrets_value(&self) -> Result<Map<String, Value>, CommandError> {
        self.payload_as_object(self.secrets_json.as_deref(), self.secrets_file.as_deref(), "secrets payload")
    }

    fn apply_dot_params(&self, mut payload: Map<String, Value>) -> Result<Map<String, Value>, CommandError> {
        let Some(set_args) = &self.set else {
            return Ok(payload);
        };

        for key_value_pair in set_args {
            let Some((key, value)) = key_value_pair.split_once('=') else {
                return Err(CommandError::invalid_input(format!(
                    "invalid --set format: expected KEY=VALUE, got '{key_value_pair}'"
                )));
            };

            let key = key.trim();
            let value = value.trim();
            let mut current_payload = &mut payload;
            let key_parts: Vec<&str> = key.split('.').collect();

            for (key_part_index, key_part) in key_parts.iter().enumerate() {
                let is_last_key_part = key_part_index == key_parts.len() - 1;

                if is_last_key_part {
                    current_payload.insert((*key_part).to_string(), Value::String(value.to_string()));
                } else {
                    if !current_payload.contains_key(*key_part) {
                        current_payload.insert((*key_part).to_string(), Value::Object(Map::new()));
                    }

                    let Some(object_payload) = current_payload.get_mut(*key_part).and_then(Value::as_object_mut) else {
                        return Err(CommandError::invalid_input(format!(
                            "cannot set nested value on non-object path: {key}"
                        )));
                    };

                    current_payload = object_payload;
                }
            }
        }

        Ok(payload)
    }

    fn payload_as_object(
        &self,
        inline_payload: Option<&str>,
        payload_file_path: Option<&Path>,
        payload_label: &str,
    ) -> Result<Map<String, Value>, CommandError> {
        let payload_json = if let Some(inline_payload) = inline_payload {
            inline_payload.to_string()
        } else if let Some(payload_file_path) = payload_file_path {
            fs::read_to_string(payload_file_path).map_err(|read_error| {
                CommandError::invalid_input(format!(
                    "failed to read {payload_label} file {}: {read_error}",
                    payload_file_path.display()
                ))
            })?
        } else {
            "{}".to_string()
        };

        let parsed_payload_value = serde_json::from_str::<Value>(&payload_json)
            .map_err(|parse_error| CommandError::invalid_input(format!("{payload_label} must be valid json: {parse_error}")))?;

        let Some(parsed_payload_object) = parsed_payload_value.as_object() else {
            return Err(CommandError::invalid_input(format!("{payload_label} must be a json object")));
        };

        Ok(parsed_payload_object.clone())
    }

    fn discover_workflow_lock(
        parsed_workflow: &Workflow,
        lock_context: Option<&McpLockResolutionContext>,
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

        McpLock::discover_from_workflow_with_lock_context(parsed_workflow, lock_context).map_err(|mcp_error| {
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

struct PromptedLockContext {
    lock_context: McpLockResolutionContext,
    prompted_value_was_captured: bool,
}

impl PromptedLockContext {
    fn as_ref(&self) -> Option<&McpLockResolutionContext> {
        if self.lock_context.input.is_empty()
            && self.lock_context.secrets.is_empty()
            && self.lock_context.dynamic.is_empty()
            && self.lock_context.agent_outputs.is_empty()
            && self.lock_context.agent_contexts.is_empty()
        {
            return None;
        }

        Some(&self.lock_context)
    }
}

#[derive(Debug, Clone)]
struct CliRuntimeSchemaContext;

impl CliRuntimeSchemaContext {
    fn from_workflow(workflow: &Workflow) -> Result<Self, CommandError> {
        let workflow_type_inference = CliWorkflowTypeInference::from_workflow(workflow)?;

        let inferred_input_type = workflow_type_inference
            .input_type
            .unwrap_or_else(|| WorkflowType::Object(BTreeMap::new()));
        let inferred_output_type = workflow_type_inference.workflow_output_type;
        let input_schema_value = workflow_type_to_json_schema(&inferred_input_type);
        let output_schema_value = workflow_type_to_json_schema(&inferred_output_type);
        let _input_schema = serde_json::from_value::<Schema>(input_schema_value)
            .map_err(|error| CommandError::internal(format!("failed to convert inferred workflow input type into schema: {error}")))?;

        let _output_schema = serde_json::from_value::<Schema>(output_schema_value)
            .map_err(|error| CommandError::internal(format!("failed to convert inferred workflow output type into schema: {error}")))?;

        Ok(Self)
    }
}

#[derive(Debug, Clone)]
struct CliWorkflowTypeInference {
    input_type: Option<WorkflowType>,
    workflow_output_type: WorkflowType,
}

struct CliToolTypes {
    input: HashMap<String, WorkflowType>,
    bindings: HashMap<String, WorkflowType>,
    output: HashMap<String, WorkflowType>,
}

impl CliWorkflowTypeInference {
    fn from_workflow(workflow: &Workflow) -> Result<Self, CommandError> {
        let named_schema_types = Self::collect_named_schema_types(workflow);
        let input_type = Self::build_input_type(workflow, &named_schema_types)?;
        let secrets_type = Self::build_secrets_type(workflow, &named_schema_types)?;
        let tool_types = Self::collect_tool_types(workflow, &named_schema_types)?;
        let agent_output_types = Self::collect_agent_output_types(workflow, &named_schema_types)?;
        let workflow_output_type =
            Self::infer_workflow_output_type(workflow, input_type.clone(), secrets_type, agent_output_types, tool_types)?;

        Ok(Self {
            input_type,
            workflow_output_type,
        })
    }

    fn collect_named_schema_types(workflow: &Workflow) -> HashMap<String, TypeExpression> {
        let mut named_schema_types = HashMap::new();

        for declaration in workflow.declarations() {
            let Declaration::Schema(schema_declaration) = declaration else {
                continue;
            };

            named_schema_types.insert(
                schema_declaration.name.clone(),
                TypeExpression::Object(schema_declaration.fields.clone()),
            );
        }

        named_schema_types
    }

    fn build_input_type(
        workflow: &Workflow,
        named_schema_types: &HashMap<String, TypeExpression>,
    ) -> Result<Option<WorkflowType>, CommandError> {
        let Some(input_declaration) = workflow.find_input() else {
            return Ok(None);
        };

        let input_type_expression = TypeExpression::Object(input_declaration.fields.clone());
        let input_type = workflow_type_from_dsl(&input_type_expression, named_schema_types)
            .map_err(|runtime_error| CommandError::internal(runtime_error.to_string()))?;

        Ok(Some(input_type))
    }

    fn build_secrets_type(
        workflow: &Workflow,
        named_schema_types: &HashMap<String, TypeExpression>,
    ) -> Result<Option<WorkflowType>, CommandError> {
        let Some(secrets_declaration) = workflow.find_secrets() else {
            return Ok(None);
        };

        let secrets_type_expression = TypeExpression::Object(secrets_declaration.fields.clone());
        let secrets_type = workflow_type_from_dsl(&secrets_type_expression, named_schema_types)
            .map_err(|runtime_error| CommandError::internal(runtime_error.to_string()))?;

        Ok(Some(secrets_type))
    }

    fn collect_agent_output_types(
        workflow: &Workflow,
        named_schema_types: &HashMap<String, TypeExpression>,
    ) -> Result<HashMap<String, WorkflowType>, CommandError> {
        let mut agent_output_types = HashMap::new();

        for declaration in workflow.declarations() {
            let Declaration::Agent(agent_declaration) = declaration else {
                continue;
            };

            let iteration_output_type = if let Some(agent_output_type_expression) = agent_declaration.output_type() {
                workflow_type_from_dsl(&agent_output_type_expression, named_schema_types)
                    .map_err(|runtime_error| CommandError::internal(runtime_error.to_string()))?
            } else {
                WorkflowType::String
            };

            let final_output_type = if agent_declaration.for_loop.is_some() {
                WorkflowType::Array {
                    item_type: Box::new(iteration_output_type),
                    fixed_length: None,
                }
                .normalize()
            } else {
                iteration_output_type
            };

            agent_output_types.insert(agent_declaration.name.clone(), final_output_type);
        }

        Ok(agent_output_types)
    }

    fn collect_tool_types(workflow: &Workflow, named_schema_types: &HashMap<String, TypeExpression>) -> Result<CliToolTypes, CommandError> {
        let mut input = HashMap::new();
        let mut bindings = HashMap::new();
        let mut output = HashMap::new();

        for tool_declaration in workflow.tool_declarations() {
            input.insert(
                tool_declaration.name.clone(),
                workflow_type_from_dsl(&TypeExpression::Object(tool_declaration.input_fields.clone()), named_schema_types)
                    .map_err(|runtime_error| CommandError::internal(runtime_error.to_string()))?,
            );
            bindings.insert(
                tool_declaration.name.clone(),
                workflow_type_from_dsl(&TypeExpression::Object(tool_declaration.binding_fields.clone()), named_schema_types)
                    .map_err(|runtime_error| CommandError::internal(runtime_error.to_string()))?,
            );
            output.insert(
                tool_declaration.name.clone(),
                workflow_type_from_dsl(&TypeExpression::Object(tool_declaration.output_fields.clone()), named_schema_types)
                    .map_err(|runtime_error| CommandError::internal(runtime_error.to_string()))?,
            );
        }

        Ok(CliToolTypes { input, bindings, output })
    }

    fn infer_workflow_output_type(
        workflow: &Workflow,
        input_type: Option<WorkflowType>,
        secrets_type: Option<WorkflowType>,
        agent_output_types: HashMap<String, WorkflowType>,
        tool_types: CliToolTypes,
    ) -> Result<WorkflowType, CommandError> {
        let Some(output_declaration) = workflow.find_output() else {
            return Err(CommandError::internal(String::from("workflow requires an `output` block")));
        };

        let mut type_inference_context = TypeInferenceContext {
            input_type,
            secrets_type,
            agent_output_types,
            tool_input_types: tool_types.input,
            tool_binding_types: tool_types.bindings,
            tool_output_types: tool_types.output,
            local_binding_types: HashMap::new(),
        };

        let dynamic_fields = workflow
            .declarations()
            .iter()
            .filter_map(|declaration| match declaration {
                Declaration::Dynamic(dynamic_block) => Some(dynamic_block.fields.as_slice()),
                _ => None,
            })
            .flatten()
            .collect::<Vec<_>>();

        Self::infer_dynamic_field_types(dynamic_fields, &mut type_inference_context)?;

        let mut output_field_types = BTreeMap::new();

        for output_field in &output_declaration.fields {
            let output_field_type = infer_expression_type(&output_field.value, &type_inference_context, "workflow output type inference")
                .map_err(|runtime_error| CommandError::internal(runtime_error.to_string()))?;

            output_field_types.insert(output_field.name.clone(), output_field_type);
        }

        Ok(WorkflowType::Object(output_field_types).normalize())
    }

    fn infer_dynamic_field_types(
        dynamic_fields: Vec<&ObjectField>,
        type_inference_context: &mut TypeInferenceContext,
    ) -> Result<(), CommandError> {
        let mut pending_dynamic_fields = dynamic_fields;

        while !pending_dynamic_fields.is_empty() {
            let pending_count_before_pass = pending_dynamic_fields.len();
            let mut last_error = None;

            pending_dynamic_fields.retain(|dynamic_field| {
                let inference_result = infer_expression_type(
                    &dynamic_field.value,
                    type_inference_context,
                    &format!("dynamic field `{}` type inference", dynamic_field.name),
                );

                match inference_result {
                    Ok(field_type) => {
                        type_inference_context
                            .local_binding_types
                            .insert(dynamic_field.name.clone(), field_type);

                        false
                    }
                    Err(runtime_error) => {
                        last_error = Some(runtime_error);

                        true
                    }
                }
            });

            if pending_dynamic_fields.len() == pending_count_before_pass {
                if let Some(runtime_error) = last_error {
                    return Err(CommandError::internal(runtime_error.to_string()));
                }

                break;
            }
        }

        Ok(())
    }
}
