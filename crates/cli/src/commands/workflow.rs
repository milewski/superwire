use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use superwire_core::dsl::{
    parse_workflow, AgentForLoop, AgentForLoopPattern, CallArgument, Declaration, Expression, ObjectField, StringTemplatePart,
    TypeExpression, TypedField, Workflow,
};
use superwire_core::semantic::support::type_inference::{infer_expression_type, TypeInferenceContext};
use superwire_core::semantic::support::types::{workflow_type_from_dsl, workflow_type_to_json_schema, WorkflowType};
use superwire_core::semantic::{compile_workflow_pipeline, ExecutionPlan, TypedWorkflowIr, WorkflowPipelineInput};
use superwire_executor::{ExecutorError, OpenAiModelProvider, WorkflowExecutor};

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
            WorkflowSubcommand::ToJson(to_json_workflow_command) => to_json_workflow_command.execute(),
            WorkflowSubcommand::Check(check_workflow_command) => check_workflow_command.execute(),
            WorkflowSubcommand::Run(run_workflow_command) => run_workflow_command.execute(),
        }
    }
}

#[derive(Debug, Subcommand)]
enum WorkflowSubcommand {
    ToJson(ToJsonWorkflowCommand),
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
            CommandError::internal(error.render_with_source(&workflow_source, &self.workflow_path.display().to_string()))
        })?;

        let _runtime_schema_context = CliRuntimeSchemaContext::from_workflow(&parsed_workflow)?;
        let workflow_executor =
            WorkflowExecutor::from_source(&workflow_source).map_err(|error| CommandError::internal(error.to_string()))?;

        let output_value = async_runtime
            .block_on(workflow_executor.execute(
                Value::Object(input_value),
                Value::Object(secrets_value),
                &OpenAiModelProvider,
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

    fn collect_tool_types(workflow: &Workflow, named_schema_types: &HashMap<String, TypeExpression>) -> Result<CliToolTypes, CommandError> {
        let mut input = HashMap::new();
        let mut bindings = HashMap::new();
        let mut output = HashMap::new();

        for declaration in workflow.declarations() {
            let Declaration::Tool(tool_declaration) = declaration else {
                continue;
            };

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

#[derive(Debug, Args)]
struct ToJsonWorkflowCommand {
    #[arg(value_name = "WORKFLOW_PATH")]
    workflow_path: PathBuf,

    #[arg(short = 'o', long = "output", value_name = "OUTPUT_PATH")]
    output_path: Option<PathBuf>,

    #[arg(long, action = clap::ArgAction::SetTrue)]
    compact: bool,
}

impl ToJsonWorkflowCommand {
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

        let compiled_pipeline = runtime_schema_context
            .with_scope(|| {
                compile_workflow_pipeline::<DynamicWorkflowInput, DynamicWorkflowOutput>(WorkflowPipelineInput::Workflow(&parsed_workflow))
            })
            .map_err(|workflow_runtime_error| CommandError::invalid_input(workflow_runtime_error.to_string()))?;

        let workflow_representation = WorkflowJsonRepresentation::from_compilation(
            &self.workflow_path,
            parsed_workflow,
            compiled_pipeline.typed_workflow_ir(),
            compiled_pipeline.execution_plan(),
        );

        let serialized_json = if self.compact {
            serde_json::to_string(&workflow_representation).map_err(|serialization_error| {
                CommandError::internal(format!("failed to serialize workflow json: {serialization_error}"))
            })?
        } else {
            serde_json::to_string_pretty(&workflow_representation).map_err(|serialization_error| {
                CommandError::internal(format!("failed to serialize workflow json: {serialization_error}"))
            })?
        };

        if let Some(output_path) = self.output_path {
            fs::write(&output_path, serialized_json).map_err(|write_error| {
                CommandError::internal(format!("failed to write workflow json to {}: {write_error}", output_path.display()))
            })?;

            return Ok(());
        }

        println!("{serialized_json}");

        Ok(())
    }
}

#[derive(Debug, Serialize)]
struct WorkflowJsonRepresentation {
    format: String,
    workflow_path: String,
    input: Option<SerializableContractType>,
    secrets: Option<SerializableContractType>,
    dynamic: BTreeMap<String, Value>,
    schemas: Vec<SerializableSchema>,
    tools: Vec<SerializableToolDeclaration>,
    providers: Vec<SerializableProvider>,
    agents: Vec<SerializableAgent>,
    output: SerializableWorkflowOutput,
    execution: SerializableExecution,
}

impl WorkflowJsonRepresentation {
    fn from_compilation(
        workflow_path: &Path,
        workflow: Workflow,
        typed_workflow_ir: &TypedWorkflowIr,
        execution_plan: &ExecutionPlan,
    ) -> Self {
        let declarations = workflow.declarations();
        let named_schema_types = CliWorkflowTypeInference::collect_named_schema_types(&workflow);
        let dependents_by_agent = Self::collect_dependents_by_agent(execution_plan);
        let execution_batches = Self::resolve_execution_batches(execution_plan);
        let batch_indexes_by_agent = Self::batch_indexes_by_agent(&execution_batches);

        let mut providers = Vec::new();
        let mut schemas = Vec::new();
        let mut tools = Vec::new();
        let mut dynamic = BTreeMap::<String, Value>::new();

        for declaration in declarations {
            match declaration {
                Declaration::Provider(provider_declaration) => {
                    providers.push(SerializableProvider::from_declaration(provider_declaration));
                }
                Declaration::Schema(schema_declaration) => {
                    schemas.push(SerializableSchema::from_declaration(schema_declaration));
                }
                Declaration::Tool(tool_declaration) => {
                    tools.push(SerializableToolDeclaration::from_declaration(tool_declaration, &named_schema_types));
                }
                Declaration::Dynamic(dynamic_block) => {
                    for dynamic_field in &dynamic_block.fields {
                        dynamic.insert(
                            dynamic_field.name.clone(),
                            SerializableExpression::to_compact_json(&dynamic_field.value),
                        );
                    }
                }
                Declaration::McpServer(_)
                | Declaration::Secrets(_)
                | Declaration::Input(_)
                | Declaration::Agent(_)
                | Declaration::Output(_) => {}
            }
        }

        let mut agents = Vec::new();

        for typed_agent in &typed_workflow_ir.agents {
            let declaration = workflow
                .find_agent(&typed_agent.name)
                .expect("agent declaration should exist for typed agent");

            let planned_agent = execution_plan
                .planned_agents
                .get(&typed_agent.name)
                .expect("planned agent should exist for typed agent");

            let batch_index = batch_indexes_by_agent
                .get(&typed_agent.name)
                .copied()
                .expect("batch index should exist for typed agent");

            let dependents = dependents_by_agent.get(&typed_agent.name).cloned().unwrap_or_default();

            agents.push(SerializableAgent::from_compilation(
                declaration,
                typed_agent,
                planned_agent.dependencies.clone(),
                dependents,
                batch_index,
            ));
        }

        Self {
            format: "superwire_workflow_compact_v1".to_string(),
            workflow_path: workflow_path.display().to_string(),
            input: typed_workflow_ir
                .input_type
                .as_ref()
                .map(SerializableContractType::from_workflow_type),
            secrets: typed_workflow_ir
                .secrets_type
                .as_ref()
                .map(SerializableContractType::from_workflow_type),
            dynamic,
            schemas,
            tools,
            providers,
            agents,
            output: SerializableWorkflowOutput::from_output_declaration(
                &typed_workflow_ir.output_declaration,
                &typed_workflow_ir.workflow_output_type,
            ),
            execution: SerializableExecution::from_plan(execution_plan, execution_batches),
        }
    }

    fn collect_dependents_by_agent(execution_plan: &ExecutionPlan) -> HashMap<String, Vec<String>> {
        let mut dependents_by_agent = HashMap::<String, Vec<String>>::new();

        for agent_name in &execution_plan.agent_execution_order {
            dependents_by_agent.insert(agent_name.clone(), Vec::new());
        }

        for agent_name in &execution_plan.agent_execution_order {
            let planned_agent = execution_plan
                .planned_agents
                .get(agent_name)
                .expect("planned agent should exist while collecting dependents");

            for dependency_name in &planned_agent.dependencies {
                dependents_by_agent
                    .entry(dependency_name.clone())
                    .or_default()
                    .push(agent_name.clone());
            }
        }

        for dependent_agent_names in dependents_by_agent.values_mut() {
            dependent_agent_names.sort();
            dependent_agent_names.dedup();
        }

        dependents_by_agent
    }

    fn resolve_execution_batches(execution_plan: &ExecutionPlan) -> Vec<Vec<String>> {
        let mut unresolved_agent_names = execution_plan
            .agent_execution_order
            .iter()
            .cloned()
            .collect::<std::collections::HashSet<_>>();
        let mut resolved_agent_names = std::collections::HashSet::<String>::new();
        let mut execution_batches = Vec::<Vec<String>>::new();

        while !unresolved_agent_names.is_empty() {
            let mut ready_agent_names = Vec::<String>::new();

            for agent_name in &execution_plan.agent_execution_order {
                if !unresolved_agent_names.contains(agent_name) {
                    continue;
                }

                let planned_agent = execution_plan
                    .planned_agents
                    .get(agent_name)
                    .expect("planned agent should exist while collecting execution batches");

                if planned_agent
                    .dependencies
                    .iter()
                    .any(|dependency_name| !resolved_agent_names.contains(dependency_name))
                {
                    continue;
                }

                ready_agent_names.push(agent_name.clone());
            }

            if ready_agent_names.is_empty() {
                break;
            }

            for ready_agent_name in &ready_agent_names {
                unresolved_agent_names.remove(ready_agent_name);
                resolved_agent_names.insert(ready_agent_name.clone());
            }

            execution_batches.push(ready_agent_names);
        }

        execution_batches
    }

    fn batch_indexes_by_agent(execution_batches: &[Vec<String>]) -> HashMap<String, usize> {
        let mut batch_indexes_by_agent = HashMap::<String, usize>::new();

        for (batch_index, batch_agent_names) in execution_batches.iter().enumerate() {
            for agent_name in batch_agent_names {
                batch_indexes_by_agent.insert(agent_name.clone(), batch_index);
            }
        }

        batch_indexes_by_agent
    }
}

#[derive(Debug, Serialize)]
struct SerializableContractType {
    workflow_type: SerializableWorkflowType,
    json_schema: Value,
}

impl SerializableContractType {
    fn from_workflow_type(workflow_type: &WorkflowType) -> Self {
        Self {
            workflow_type: SerializableWorkflowType::from_workflow_type(workflow_type),
            json_schema: workflow_type_to_json_schema(workflow_type),
        }
    }
}

#[derive(Debug, Serialize)]
struct SerializableSchema {
    name: String,
    fields: Vec<SerializableTypedField>,
}

impl SerializableSchema {
    fn from_declaration(schema_declaration: &superwire_core::dsl::SchemaDeclaration) -> Self {
        Self {
            name: schema_declaration.name.clone(),
            fields: schema_declaration
                .fields
                .iter()
                .map(SerializableTypedField::from_typed_field)
                .collect::<Vec<_>>(),
        }
    }
}

#[derive(Debug, Serialize)]
struct SerializableToolDeclaration {
    name: String,
    description: Option<String>,
    source: Option<String>,
    input: Vec<SerializableTypedField>,
    input_schema: Value,
    bindings: Vec<SerializableTypedField>,
    fixed_bindings: BTreeMap<String, Value>,
    binding_schema: Value,
}

impl SerializableToolDeclaration {
    fn from_declaration(
        tool_declaration: &superwire_core::dsl::ToolDeclaration,
        named_schema_types: &HashMap<String, TypeExpression>,
    ) -> Self {
        Self {
            name: tool_declaration.name.clone(),
            description: tool_declaration.description.clone(),
            source: tool_declaration
                .source
                .as_ref()
                .and_then(superwire_core::dsl::ToolSource::mcp_tool_name)
                .map(|tool_name| format!("mcp.{tool_name}")),
            input: tool_declaration
                .input_fields
                .iter()
                .map(SerializableTypedField::from_typed_field)
                .collect::<Vec<_>>(),
            input_schema: Self::json_schema_for_fields(&tool_declaration.input_fields, named_schema_types),
            bindings: tool_declaration
                .binding_fields
                .iter()
                .map(SerializableTypedField::from_typed_field)
                .collect::<Vec<_>>(),
            fixed_bindings: tool_declaration
                .fixed_binding_fields
                .iter()
                .map(|field| (field.name.clone(), SerializableExpression::to_compact_json(&field.value)))
                .collect::<BTreeMap<_, _>>(),
            binding_schema: Self::json_schema_for_fields(&tool_declaration.binding_fields, named_schema_types),
        }
    }

    fn json_schema_for_fields(typed_fields: &[TypedField], named_schema_types: &HashMap<String, TypeExpression>) -> Value {
        let object_type_expression = TypeExpression::Object(typed_fields.to_vec());
        let workflow_type = workflow_type_from_dsl(&object_type_expression, named_schema_types)
            .expect("tool declaration field schemas should resolve during workflow compilation");
        let mut json_schema_value = workflow_type_to_json_schema(&workflow_type);

        if let Some(json_schema_object) = json_schema_value.as_object_mut() {
            let has_empty_required = json_schema_object
                .get("required")
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty);

            if has_empty_required {
                json_schema_object.remove("required");
            }
        }

        json_schema_value
    }
}

#[derive(Debug, Serialize)]
struct SerializableTypedField {
    name: String,
    field_type: SerializableTypeExpression,
    description: Option<String>,
}

impl SerializableTypedField {
    fn from_typed_field(typed_field: &TypedField) -> Self {
        Self {
            name: typed_field.name.clone(),
            field_type: SerializableTypeExpression::from_type_expression(&typed_field.field_type),
            description: typed_field.description.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
struct SerializableProvider {
    name: String,
    driver: Option<String>,
    models: Option<Vec<String>>,
    config: BTreeMap<String, Value>,
}

impl SerializableProvider {
    fn from_declaration(provider_declaration: &superwire_core::dsl::ProviderDeclaration) -> Self {
        let mut config = BTreeMap::<String, Value>::new();

        for provider_property in &provider_declaration.properties {
            config.insert(
                provider_property.name.clone(),
                SerializableExpression::to_compact_json(&provider_property.value),
            );
        }

        let driver = Self::extract_string_literal_property(provider_declaration, "driver");
        let models = Self::extract_string_list_property(provider_declaration, "models");

        Self {
            name: provider_declaration.name.clone(),
            driver,
            models,
            config,
        }
    }

    fn extract_string_literal_property(
        provider_declaration: &superwire_core::dsl::ProviderDeclaration,
        property_name: &str,
    ) -> Option<String> {
        let property = provider_declaration
            .properties
            .iter()
            .find(|provider_property| provider_property.name == property_name)?;

        if let Expression::StringLiteral(property_value) = &property.value {
            return Some(property_value.clone());
        }

        None
    }

    fn extract_string_list_property(
        provider_declaration: &superwire_core::dsl::ProviderDeclaration,
        property_name: &str,
    ) -> Option<Vec<String>> {
        let property = provider_declaration
            .properties
            .iter()
            .find(|provider_property| provider_property.name == property_name)?;

        let Expression::ArrayLiteral(array_values) = &property.value else {
            return None;
        };

        let mut string_values = Vec::with_capacity(array_values.len());

        for array_value in array_values {
            let Expression::StringLiteral(string_literal) = array_value else {
                return None;
            };

            string_values.push(string_literal.clone());
        }

        Some(string_values)
    }
}

#[derive(Debug, Serialize)]
struct SerializableAgent {
    name: String,
    provider: String,
    model: Value,
    prompt: Value,
    context: Option<Value>,
    inference: Option<Value>,
    tools: Vec<SerializableToolBinding>,
    dynamic: BTreeMap<String, Value>,
    for_each: Option<SerializableForEach>,
    output: SerializableAgentOutput,
    dependencies: Vec<String>,
    dependents: Vec<String>,
    batch: usize,
}

impl SerializableAgent {
    fn from_compilation(
        agent_declaration: &superwire_core::dsl::AgentDeclaration,
        typed_agent: &superwire_core::semantic::TypedAgentIr,
        dependencies: Vec<String>,
        dependents: Vec<String>,
        batch: usize,
    ) -> Self {
        let prompt_expression = agent_declaration
            .expression_property(superwire_core::dsl::AgentExpressionPropertyName::Prompt)
            .expect("prompt expression should exist after typecheck");

        let model_value = SerializableExpression::to_compact_json(&typed_agent.model_expression);
        let prompt_value = SerializableExpression::to_compact_json(prompt_expression);
        let context_value = agent_declaration
            .expression_property(superwire_core::dsl::AgentExpressionPropertyName::Context)
            .map(SerializableExpression::to_compact_json);
        let inference_value = agent_declaration
            .expression_property(superwire_core::dsl::AgentExpressionPropertyName::Inference)
            .map(SerializableExpression::to_compact_json);
        let tools = agent_declaration
            .expression_property(superwire_core::dsl::AgentExpressionPropertyName::Tools)
            .map(SerializableToolBinding::from_tools_expression)
            .unwrap_or_default();
        let dynamic = agent_declaration
            .dynamic_blocks()
            .flat_map(|dynamic_block| dynamic_block.fields.iter())
            .map(|dynamic_field| {
                (
                    dynamic_field.name.clone(),
                    SerializableExpression::to_compact_json(&dynamic_field.value),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let for_each = agent_declaration.for_loop.as_ref().map(SerializableForEach::from_for_loop);

        Self {
            name: typed_agent.name.clone(),
            provider: typed_agent.provider_name.clone(),
            model: model_value,
            prompt: prompt_value,
            context: context_value,
            inference: inference_value,
            tools,
            dynamic,
            for_each,
            output: SerializableAgentOutput::from_compilation(&typed_agent.iteration_output_type, &typed_agent.final_output_type),
            dependencies,
            dependents,
            batch,
        }
    }
}

#[derive(Debug, Serialize)]
struct SerializableForEach {
    pattern: Value,
    iterable: Value,
}

impl SerializableForEach {
    fn from_for_loop(for_loop: &AgentForLoop) -> Self {
        let pattern = match &for_loop.pattern {
            AgentForLoopPattern::Identifier(identifier_name) => {
                json!({ "identifier": identifier_name })
            }
            AgentForLoopPattern::ObjectDestructuring(field_names) => {
                json!({ "object": field_names })
            }
        };

        let iterable = SerializableExpression::to_compact_json(&for_loop.iterable);

        Self { pattern, iterable }
    }
}

#[derive(Debug, Serialize)]
struct SerializableToolBinding {
    name: String,
    bind: BTreeMap<String, Value>,
}

impl SerializableToolBinding {
    fn from_tools_expression(tools_expression: &Expression) -> Vec<Self> {
        let Expression::ArrayLiteral(tool_entries) = tools_expression else {
            return Vec::new();
        };

        let mut tool_bindings = Vec::new();

        for tool_entry in tool_entries {
            match tool_entry {
                Expression::Reference(reference) => {
                    if !reference.is_keyword_root(superwire_core::dsl::ReferenceKeyword::Tool) {
                        continue;
                    }

                    let Some(tool_name) = reference.first_access_field() else {
                        continue;
                    };

                    tool_bindings.push(Self {
                        name: tool_name.to_string(),
                        bind: BTreeMap::new(),
                    });
                }
                Expression::ToolCall(tool_call) => {
                    if !tool_call.callee.is_keyword_root(superwire_core::dsl::ReferenceKeyword::Tool) {
                        continue;
                    }

                    let Some(tool_name) = tool_call.callee.first_access_field() else {
                        continue;
                    };

                    let mut binding_values = BTreeMap::<String, Value>::new();

                    for binding_field in &tool_call.binding_fields {
                        binding_values.insert(
                            binding_field.name.clone(),
                            SerializableExpression::to_compact_json(&binding_field.value),
                        );
                    }

                    tool_bindings.push(Self {
                        name: tool_name.to_string(),
                        bind: binding_values,
                    });
                }
                Expression::StringLiteral(_)
                | Expression::StringTemplate(_)
                | Expression::NumberLiteral(_)
                | Expression::BooleanLiteral(_)
                | Expression::NullLiteral
                | Expression::ArrayLiteral(_)
                | Expression::ObjectLiteral(_)
                | Expression::FunctionCall(_) => {}
            }
        }

        tool_bindings
    }
}

#[derive(Debug, Serialize)]
struct SerializableAgentOutput {
    iteration: SerializableContractType,
    final_output: SerializableContractType,
}

impl SerializableAgentOutput {
    fn from_compilation(iteration_output_type: &WorkflowType, final_output_type: &WorkflowType) -> Self {
        Self {
            iteration: SerializableContractType::from_workflow_type(iteration_output_type),
            final_output: SerializableContractType::from_workflow_type(final_output_type),
        }
    }
}

#[derive(Debug, Serialize)]
struct SerializableWorkflowOutput {
    fields: BTreeMap<String, Value>,
    contract: SerializableContractType,
}

impl SerializableWorkflowOutput {
    fn from_output_declaration(output_declaration: &superwire_core::dsl::OutputDeclaration, workflow_output_type: &WorkflowType) -> Self {
        let mut fields = BTreeMap::<String, Value>::new();

        for output_field in &output_declaration.fields {
            fields.insert(
                output_field.name.clone(),
                SerializableExpression::to_compact_json(&output_field.value),
            );
        }

        Self {
            fields,
            contract: SerializableContractType::from_workflow_type(workflow_output_type),
        }
    }
}

#[derive(Debug, Serialize)]
struct SerializableExecution {
    order: Vec<String>,
    batches: Vec<Vec<String>>,
    edges: Vec<SerializableExecutionEdge>,
}

impl SerializableExecution {
    fn from_plan(execution_plan: &ExecutionPlan, execution_batches: Vec<Vec<String>>) -> Self {
        let mut edges = Vec::<SerializableExecutionEdge>::new();

        for agent_name in &execution_plan.agent_execution_order {
            let planned_agent = execution_plan
                .planned_agents
                .get(agent_name)
                .expect("planned agent should exist while serializing execution");

            for dependency_name in &planned_agent.dependencies {
                edges.push(SerializableExecutionEdge {
                    from: dependency_name.clone(),
                    to: agent_name.clone(),
                });
            }
        }

        Self {
            order: execution_plan.agent_execution_order.clone(),
            batches: execution_batches,
            edges,
        }
    }
}

#[derive(Debug, Serialize)]
struct SerializableExecutionEdge {
    from: String,
    to: String,
}

struct SerializableExpression;

impl SerializableExpression {
    fn to_compact_json(expression: &Expression) -> Value {
        match expression {
            Expression::StringLiteral(string_value) => Value::String(string_value.clone()),
            Expression::NumberLiteral(number_value) => {
                if let Ok(parsed_number) = number_value.parse::<i64>() {
                    return Value::Number(parsed_number.into());
                }

                if let Ok(parsed_number) = number_value.parse::<f64>() {
                    if let Some(number_value) = serde_json::Number::from_f64(parsed_number) {
                        return Value::Number(number_value);
                    }
                }

                Value::String(number_value.clone())
            }
            Expression::BooleanLiteral(boolean_value) => Value::Bool(*boolean_value),
            Expression::NullLiteral => Value::Null,
            Expression::Reference(reference) => json!({ "$ref": reference.render_path() }),
            Expression::StringTemplate(string_template) => {
                let mut template_parts = Vec::<Value>::new();

                for template_part in &string_template.parts {
                    match template_part {
                        StringTemplatePart::Text(text_segment) => {
                            template_parts.push(Value::String(text_segment.clone()));
                        }
                        StringTemplatePart::Interpolation(interpolation_expression) => {
                            template_parts.push(json!({
                                "$expr": Self::to_compact_json(interpolation_expression),
                            }));
                        }
                    }
                }

                json!({ "$template": template_parts })
            }
            Expression::FunctionCall(function_call) => {
                let mut positional_arguments = Vec::<Value>::new();
                let mut named_arguments = BTreeMap::<String, Value>::new();

                for call_argument in &function_call.arguments {
                    match call_argument {
                        CallArgument::Positional(positional_argument) => {
                            positional_arguments.push(Self::to_compact_json(positional_argument));
                        }
                        CallArgument::Named(named_argument) => {
                            named_arguments.insert(named_argument.name.clone(), Self::to_compact_json(&named_argument.value));
                        }
                    }
                }

                json!({
                    "$call": function_call.callee.render_path(),
                    "args": positional_arguments,
                    "named": named_arguments,
                })
            }
            Expression::ToolCall(tool_call) => {
                let mut input_values = Map::<String, Value>::new();
                let mut binding_values = Map::<String, Value>::new();

                for object_field in &tool_call.input_fields {
                    input_values.insert(object_field.name.clone(), Self::to_compact_json(&object_field.value));
                }

                for object_field in &tool_call.binding_fields {
                    binding_values.insert(object_field.name.clone(), Self::to_compact_json(&object_field.value));
                }

                json!({
                    "$tool_call": tool_call.callee.render_path(),
                    "input": input_values,
                    "bindings": binding_values,
                })
            }
            Expression::ArrayLiteral(array_values) => Value::Array(array_values.iter().map(Self::to_compact_json).collect::<Vec<_>>()),
            Expression::ObjectLiteral(object_fields) => {
                let mut object_values = Map::<String, Value>::new();

                for object_field in object_fields {
                    object_values.insert(object_field.name.clone(), Self::to_compact_json(&object_field.value));
                }

                Value::Object(object_values)
            }
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum SerializableTypeExpression {
    String,
    Number,
    Float,
    Boolean,
    Null,
    SchemaReference {
        name: String,
    },
    StringEnum {
        value: String,
    },
    StringEnumReference {
        reference: Value,
    },
    Array {
        item_type: Box<SerializableTypeExpression>,
        fixed_length: Option<u64>,
    },
    Tuple {
        items: Vec<SerializableTypeExpression>,
    },
    Object {
        fields: Vec<SerializableTypedField>,
    },
    Union {
        members: Vec<SerializableTypeExpression>,
    },
}

impl SerializableTypeExpression {
    fn from_type_expression(type_expression: &TypeExpression) -> Self {
        match type_expression {
            TypeExpression::String => Self::String,
            TypeExpression::Number => Self::Number,
            TypeExpression::Float => Self::Float,
            TypeExpression::Boolean => Self::Boolean,
            TypeExpression::Null => Self::Null,
            TypeExpression::SchemaReference(schema_name) => Self::SchemaReference { name: schema_name.clone() },
            TypeExpression::StringEnum(string_enum_value) => Self::StringEnum {
                value: string_enum_value.clone(),
            },
            TypeExpression::StringEnumReference(reference) => Self::StringEnumReference {
                reference: SerializableExpression::to_compact_json(&Expression::Reference(reference.clone())),
            },
            TypeExpression::Array { item_type, fixed_length } => Self::Array {
                item_type: Box::new(Self::from_type_expression(item_type)),
                fixed_length: *fixed_length,
            },
            TypeExpression::Tuple(tuple_items) => Self::Tuple {
                items: tuple_items.iter().map(Self::from_type_expression).collect::<Vec<_>>(),
            },
            TypeExpression::Object(object_fields) => Self::Object {
                fields: object_fields
                    .iter()
                    .map(SerializableTypedField::from_typed_field)
                    .collect::<Vec<_>>(),
            },
            TypeExpression::Union(union_members) => Self::Union {
                members: union_members.iter().map(Self::from_type_expression).collect::<Vec<_>>(),
            },
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum SerializableWorkflowType {
    String,
    Integer,
    Float,
    Boolean,
    Null,
    StringEnum {
        values: Vec<String>,
    },
    Array {
        item_type: Box<SerializableWorkflowType>,
        fixed_length: Option<u64>,
    },
    Tuple {
        items: Vec<SerializableWorkflowType>,
    },
    Object {
        fields: BTreeMap<String, SerializableWorkflowType>,
    },
    Union {
        members: Vec<SerializableWorkflowType>,
    },
}

impl SerializableWorkflowType {
    fn from_workflow_type(workflow_type: &WorkflowType) -> Self {
        match workflow_type {
            WorkflowType::String => Self::String,
            WorkflowType::Integer => Self::Integer,
            WorkflowType::Float => Self::Float,
            WorkflowType::Boolean => Self::Boolean,
            WorkflowType::Null => Self::Null,
            WorkflowType::StringEnum(enum_values) => Self::StringEnum {
                values: enum_values.clone(),
            },
            WorkflowType::Array { item_type, fixed_length } => Self::Array {
                item_type: Box::new(Self::from_workflow_type(item_type)),
                fixed_length: *fixed_length,
            },
            WorkflowType::Tuple(tuple_items) => Self::Tuple {
                items: tuple_items.iter().map(Self::from_workflow_type).collect::<Vec<_>>(),
            },
            WorkflowType::Object(object_fields) => Self::Object {
                fields: object_fields
                    .iter()
                    .map(|(field_name, field_type)| (field_name.clone(), Self::from_workflow_type(field_type)))
                    .collect::<BTreeMap<_, _>>(),
            },
            WorkflowType::Union(union_members) => Self::Union {
                members: union_members.iter().map(Self::from_workflow_type).collect::<Vec<_>>(),
            },
        }
    }
}
