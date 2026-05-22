use std::collections::{BTreeMap, HashMap};

use schemars::Schema;
use superwire_core::dsl::{Declaration, ObjectField, TypeExpression, Workflow};
use superwire_core::semantic::support::type_inference::{infer_expression_type, TypeInferenceContext};
use superwire_core::semantic::support::types::{workflow_type_from_dsl, WorkflowSchemaCache, WorkflowType};

use crate::diagnostics::CommandError;

#[derive(Debug, Clone)]
pub(super) struct CliRuntimeSchemaContext;

impl CliRuntimeSchemaContext {
    pub(super) fn from_workflow(workflow: &Workflow) -> Result<Self, CommandError> {
        let workflow_type_inference = CliWorkflowTypeInference::from_workflow(workflow)?;

        let inferred_input_type = workflow_type_inference
            .input_type
            .unwrap_or_else(|| WorkflowType::Object(BTreeMap::new()));
        let inferred_output_type = workflow_type_inference.workflow_output_type;
        let mut schema_cache = WorkflowSchemaCache::new();
        let input_schema_value = inferred_input_type.json_schema_value_with_cache(&mut schema_cache);
        let output_schema_value = inferred_output_type.json_schema_value_with_cache(&mut schema_cache);
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
