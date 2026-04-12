use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use superwire_agent::AgentError;
use superwire_core::dsl::{parse_workflow, Declaration, TypeExpression, Workflow};
use superwire_core::runtime::type_inference::{infer_expression_type, TypeInferenceContext};
use superwire_core::runtime::{
    types::{workflow_type_from_dsl, workflow_type_to_json_schema, WorkflowType},
    WorkflowRuntimeError,
};

use crate::diagnostics::CommandError;

thread_local! {
    static DYNAMIC_WORKFLOW_SCHEMA_CONTEXT: RefCell<Option<CliRuntimeSchemaContext>> = const { RefCell::new(None) };
}

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
        }
    }
}

#[derive(Debug, Subcommand)]
enum WorkflowSubcommand {
    Check(CheckWorkflowCommand),
    Run(RunWorkflowCommand),
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
            CommandError::invalid_input(parse_error.render_with_source(&workflow_source, &self.workflow_path.display().to_string()))
        })?;

        let runtime_schema_context = CliRuntimeSchemaContext::from_workflow(&parsed_workflow)
            .map_err(|schema_context_error| CommandError::invalid_input(schema_context_error.message().to_string()))?;
        let workflow_directory = self.workflow_path.parent().unwrap_or_else(|| Path::new("."));

        runtime_schema_context
            .with_scope(|| {
                superwire_core::WorkflowRuntime::<DynamicWorkflowInput, DynamicWorkflowOutput>::new_with_workflow_directory(
                    parsed_workflow,
                    workflow_directory,
                )
            })
            .map_err(Self::map_workflow_runtime_error)?;

        println!("workflow is valid");

        Ok(())
    }

    fn map_workflow_runtime_error(runtime_error: WorkflowRuntimeError) -> CommandError {
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
            CommandError::internal(error.render_with_source(&workflow_source, &self.workflow_path.display().to_string()))
        })?;

        let runtime_schema_context = CliRuntimeSchemaContext::from_workflow(&parsed_workflow)?;
        let workflow_directory = self.workflow_path.parent().unwrap_or_else(|| Path::new("."));

        let workflow_runtime = runtime_schema_context
            .with_scope(|| {
                superwire_core::WorkflowRuntime::<DynamicWorkflowInput, DynamicWorkflowOutput>::new_with_workflow_directory(
                    parsed_workflow.clone(),
                    workflow_directory,
                )
            })
            .map_err(|error| CommandError::internal(error.to_string()))?;

        let output_value = async_runtime
            .block_on(workflow_runtime.run_with_secrets(
                DynamicWorkflowInput { fields: input_value },
                DynamicWorkflowSecrets { fields: secrets_value },
            ))
            .map_err(Self::map_workflow_runtime_error)?;

        if self.pretty {
            println!(
                "{}",
                serde_json::to_string_pretty(&output_value.fields)
                    .map_err(|error| CommandError::internal(format!("failed to serialize pretty workflow output: {error}")))?
            );

            return Ok(());
        }

        println!(
            "{}",
            serde_json::to_string(&output_value.fields)
                .map_err(|error| CommandError::internal(format!("failed to serialize workflow output: {error}")))?
        );

        Ok(())
    }

    fn validate_payload_arguments(&self) -> Result<(), CommandError> {
        if self.input_json.is_some() && self.input_file.is_some() {
            return Err(CommandError::invalid_input("use either --input-json or --input-file, not both"));
        }

        if self.secrets_json.is_some() && self.secrets_file.is_some() {
            return Err(CommandError::invalid_input("use either --secrets-json or --secrets-file, not both"));
        }

        Ok(())
    }

    fn input_value(&self) -> Result<Map<String, Value>, CommandError> {
        self.payload_as_object(self.input_json.as_deref(), self.input_file.as_deref(), "input payload")
    }

    fn secrets_value(&self) -> Result<Map<String, Value>, CommandError> {
        self.payload_as_object(self.secrets_json.as_deref(), self.secrets_file.as_deref(), "secrets payload")
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
    fn map_workflow_runtime_error(error: WorkflowRuntimeError) -> CommandError {
        match error {
            WorkflowRuntimeError::AgentExecutionFailed { agent_name, source } => match *source {
                AgentError::ExecutionFailed {
                    error: executor_error,
                    context,
                } => CommandError::internal_with_details(
                    format!("agent execution failed for `{agent_name}`: {executor_error}"),
                    json!({
                        "type": "workflow_runtime_error",
                        "kind": "agent_execution_failed",
                        "agent_name": agent_name,
                        "executor_error": format!("{executor_error}"),
                        "context": context,
                    }),
                ),
                other_agent_error => CommandError::internal_with_details(
                    format!("agent execution failed for `{agent_name}`: {other_agent_error}"),
                    json!({
                        "type": "workflow_runtime_error",
                        "kind": "agent_execution_failed",
                        "agent_name": agent_name,
                        "agent_error": format!("{other_agent_error}"),
                    }),
                ),
            },
            other_runtime_error => CommandError::internal_with_details(
                other_runtime_error.to_string(),
                json!({
                    "type": "workflow_runtime_error",
                    "kind": "other",
                    "error": other_runtime_error.to_string(),
                }),
            ),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct DynamicWorkflowInput {
    #[serde(flatten)]
    fields: Map<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct DynamicWorkflowOutput {
    #[serde(flatten)]
    fields: Map<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
struct DynamicWorkflowSecrets {
    #[serde(flatten)]
    fields: Map<String, Value>,
}

impl JsonSchema for DynamicWorkflowInput {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("DynamicWorkflowInput")
    }

    fn json_schema(schema_generator: &mut SchemaGenerator) -> Schema {
        let _ = schema_generator;

        DYNAMIC_WORKFLOW_SCHEMA_CONTEXT.with(|runtime_schema_context_cell| {
            let runtime_schema_context = runtime_schema_context_cell.borrow();
            let runtime_schema_context = runtime_schema_context
                .as_ref()
                .expect("dynamic workflow schema context must be initialized before execution");

            runtime_schema_context.input_schema()
        })
    }
}

impl JsonSchema for DynamicWorkflowOutput {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("DynamicWorkflowOutput")
    }

    fn json_schema(schema_generator: &mut SchemaGenerator) -> Schema {
        let _ = schema_generator;

        DYNAMIC_WORKFLOW_SCHEMA_CONTEXT.with(|runtime_schema_context_cell| {
            let runtime_schema_context = runtime_schema_context_cell.borrow();
            let runtime_schema_context = runtime_schema_context
                .as_ref()
                .expect("dynamic workflow schema context must be initialized before execution");

            runtime_schema_context.output_schema()
        })
    }
}

#[derive(Debug, Clone)]
struct CliRuntimeSchemaContext {
    input_schema: Schema,
    output_schema: Schema,
}

impl CliRuntimeSchemaContext {
    fn from_workflow(workflow: &Workflow) -> Result<Self, CommandError> {
        let workflow_type_inference = CliWorkflowTypeInference::from_workflow(workflow)?;

        let inferred_input_type = workflow_type_inference
            .input_type
            .unwrap_or_else(|| WorkflowType::Object(BTreeMap::new()));
        let inferred_output_type = workflow_type_inference.workflow_output_type;
        let input_schema_value = workflow_type_to_json_schema(&inferred_input_type);
        let output_schema_value = workflow_type_to_json_schema(&inferred_output_type);
        let input_schema = serde_json::from_value::<Schema>(input_schema_value)
            .map_err(|error| CommandError::internal(format!("failed to convert inferred workflow input type into schema: {error}")))?;

        let output_schema = serde_json::from_value::<Schema>(output_schema_value)
            .map_err(|error| CommandError::internal(format!("failed to convert inferred workflow output type into schema: {error}")))?;

        Ok(Self {
            input_schema,
            output_schema,
        })
    }

    fn input_schema(&self) -> Schema {
        self.input_schema.clone()
    }

    fn output_schema(&self) -> Schema {
        self.output_schema.clone()
    }

    fn with_scope<ExecutionResult>(&self, execute_with_schema_context: impl FnOnce() -> ExecutionResult) -> ExecutionResult {
        DYNAMIC_WORKFLOW_SCHEMA_CONTEXT.with(|runtime_schema_context_cell| {
            let previous_context = runtime_schema_context_cell.replace(Some(self.clone()));
            let execution_result = execute_with_schema_context();
            runtime_schema_context_cell.replace(previous_context);

            execution_result
        })
    }
}

#[derive(Debug, Clone)]
struct CliWorkflowTypeInference {
    input_type: Option<WorkflowType>,
    workflow_output_type: WorkflowType,
}

impl CliWorkflowTypeInference {
    fn from_workflow(workflow: &Workflow) -> Result<Self, CommandError> {
        let named_schema_types = Self::collect_named_schema_types(workflow);
        let input_type = Self::build_input_type(workflow, &named_schema_types)?;
        let secrets_type = Self::build_secrets_type(workflow, &named_schema_types)?;
        let agent_output_types = Self::collect_agent_output_types(workflow, &named_schema_types)?;
        let workflow_output_type = Self::infer_workflow_output_type(workflow, input_type.clone(), secrets_type, agent_output_types)?;

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
                workflow_type_from_dsl(agent_output_type_expression, named_schema_types)
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

    fn infer_workflow_output_type(
        workflow: &Workflow,
        input_type: Option<WorkflowType>,
        secrets_type: Option<WorkflowType>,
        agent_output_types: HashMap<String, WorkflowType>,
    ) -> Result<WorkflowType, CommandError> {
        let Some(output_declaration) = workflow.find_output() else {
            return Err(CommandError::internal(String::from("workflow requires an `output` block")));
        };

        let type_inference_context = TypeInferenceContext {
            input_type,
            secrets_type,
            agent_output_types,
            local_binding_types: HashMap::new(),
        };

        let mut output_field_types = BTreeMap::new();

        for output_field in &output_declaration.fields {
            let output_field_type = infer_expression_type(&output_field.value, &type_inference_context, "workflow output type inference")
                .map_err(|runtime_error| CommandError::internal(runtime_error.to_string()))?;

            output_field_types.insert(output_field.name.clone(), output_field_type);
        }

        Ok(WorkflowType::Object(output_field_types).normalize())
    }
}
