use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use engine_ai_core::dsl::{Declaration, TypeExpression, Workflow};
use engine_ai_core::runtime::type_inference::{infer_expression_type, TypeInferenceContext};
use engine_ai_core::runtime::types::{workflow_type_from_dsl, workflow_type_to_json_schema, WorkflowType};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::types::WorkflowExecutionError;

thread_local! {
    static DYNAMIC_WORKFLOW_SCHEMA_CONTEXT: RefCell<Option<FfiRuntimeSchemaContext>> = const { RefCell::new(None) };
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DynamicWorkflowInputValue {
    value: Value,
}

impl From<Value> for DynamicWorkflowInputValue {
    fn from(value: Value) -> Self {
        Self { value }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DynamicWorkflowOutputValue {
    value: Value,
}

impl DynamicWorkflowOutputValue {
    pub fn into_inner(self) -> Value {
        self.value
    }
}

impl From<Value> for DynamicWorkflowOutputValue {
    fn from(value: Value) -> Self {
        Self { value }
    }
}

impl JsonSchema for DynamicWorkflowInputValue {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("DynamicWorkflowInputValue")
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

impl JsonSchema for DynamicWorkflowOutputValue {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("DynamicWorkflowOutputValue")
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
pub struct FfiRuntimeSchemaContext {
    input_schema: Schema,
    output_schema: Schema,
}

impl FfiRuntimeSchemaContext {
    pub fn from_workflow(workflow: &Workflow) -> Result<Self, WorkflowExecutionError> {
        let workflow_type_inference = WorkflowTypeInference::from_workflow(workflow)?;

        let inferred_input_type = workflow_type_inference
            .input_type
            .unwrap_or_else(|| WorkflowType::Object(BTreeMap::new()));
        let inferred_output_type = workflow_type_inference.workflow_output_type;
        let input_schema_value = workflow_type_to_json_schema(&inferred_input_type);
        let output_schema_value = workflow_type_to_json_schema(&inferred_output_type);
        let input_schema = serde_json::from_value::<Schema>(input_schema_value).map_err(|error| {
            WorkflowExecutionError::validation_failed(format!("failed to convert inferred workflow input type into schema: {error}"), None)
        })?;

        let output_schema = serde_json::from_value::<Schema>(output_schema_value).map_err(|error| {
            WorkflowExecutionError::validation_failed(
                format!("failed to convert inferred workflow output type into schema: {error}"),
                None,
            )
        })?;

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

    pub fn with_scope<ExecutionResult>(&self, execute_with_schema_context: impl FnOnce() -> ExecutionResult) -> ExecutionResult {
        DYNAMIC_WORKFLOW_SCHEMA_CONTEXT.with(|runtime_schema_context_cell| {
            let previous_context = runtime_schema_context_cell.replace(Some(self.clone()));
            let execution_result = execute_with_schema_context();
            runtime_schema_context_cell.replace(previous_context);

            execution_result
        })
    }
}

#[derive(Debug, Clone)]
struct WorkflowTypeInference {
    input_type: Option<WorkflowType>,
    workflow_output_type: WorkflowType,
}

impl WorkflowTypeInference {
    fn from_workflow(workflow: &Workflow) -> Result<Self, WorkflowExecutionError> {
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
    ) -> Result<Option<WorkflowType>, WorkflowExecutionError> {
        let Some(input_declaration) = workflow.find_input() else {
            return Ok(None);
        };

        let input_type_expression = TypeExpression::Object(input_declaration.fields.clone());
        let input_type = workflow_type_from_dsl(&input_type_expression, named_schema_types)
            .map_err(|runtime_error| WorkflowExecutionError::validation_failed(runtime_error.to_string(), None))?;

        Ok(Some(input_type))
    }

    fn build_secrets_type(
        workflow: &Workflow,
        named_schema_types: &HashMap<String, TypeExpression>,
    ) -> Result<Option<WorkflowType>, WorkflowExecutionError> {
        let Some(secrets_declaration) = workflow.find_secrets() else {
            return Ok(None);
        };

        let secrets_type_expression = TypeExpression::Object(secrets_declaration.fields.clone());
        let secrets_type = workflow_type_from_dsl(&secrets_type_expression, named_schema_types)
            .map_err(|runtime_error| WorkflowExecutionError::validation_failed(runtime_error.to_string(), None))?;

        Ok(Some(secrets_type))
    }

    fn collect_agent_output_types(
        workflow: &Workflow,
        named_schema_types: &HashMap<String, TypeExpression>,
    ) -> Result<HashMap<String, WorkflowType>, WorkflowExecutionError> {
        let mut agent_output_types = HashMap::new();

        for declaration in workflow.declarations() {
            let Declaration::Agent(agent_declaration) = declaration else {
                continue;
            };

            let iteration_output_type = if let Some(agent_output_type_expression) = agent_declaration.output_type() {
                workflow_type_from_dsl(agent_output_type_expression, named_schema_types)
                    .map_err(|runtime_error| WorkflowExecutionError::validation_failed(runtime_error.to_string(), None))?
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
    ) -> Result<WorkflowType, WorkflowExecutionError> {
        let Some(output_declaration) = workflow.find_output() else {
            return Err(WorkflowExecutionError::validation_failed(
                String::from("workflow requires an `output` block"),
                None,
            ));
        };

        let type_inference_context = TypeInferenceContext {
            input_type,
            secrets_type,
            agent_output_types,
            local_binding_types: HashMap::new(),
        };

        let mut output_field_types = BTreeMap::new();

        for output_field in &output_declaration.fields {
            let output_field_type =
                infer_expression_type(&output_field.value, &type_inference_context, "ffi workflow output type inference")
                    .map_err(|runtime_error| WorkflowExecutionError::validation_failed(runtime_error.to_string(), None))?;

            output_field_types.insert(output_field.name.clone(), output_field_type);
        }

        Ok(WorkflowType::Object(output_field_types).normalize())
    }
}
