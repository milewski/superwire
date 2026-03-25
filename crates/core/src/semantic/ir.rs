use crate::dsl::{
    AgentDeclaration, AgentProperty, CallArgument, Declaration, Expression, InputDeclaration, OutputDeclaration, SchemaDeclaration,
    SecretsDeclaration, TypeExpression, Workflow,
};
use crate::runtime::error::WorkflowRuntimeError;
use crate::runtime::expression::collect_agent_dependencies;
use crate::runtime::type_inference::{infer_expression_type, TypeInferenceContext};
use crate::runtime::types::{ensure_type_matches, workflow_type_from_dsl, workflow_type_from_rust_schema, WorkflowType};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct TypedWorkflowIr {
    pub input_type: Option<WorkflowType>,
    pub secrets_type: Option<WorkflowType>,
    pub output_declaration: OutputDeclaration,
    pub workflow_output_type: WorkflowType,
    pub agents: Vec<TypedAgentIr>,
}

#[derive(Debug, Clone)]
pub struct TypedAgentIr {
    pub name: String,
    pub declaration: AgentDeclaration,
    pub provider_name: String,
    pub model_name: String,
    pub iteration_output_type: WorkflowType,
    pub final_output_type: WorkflowType,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentPropertyName {
    Model,
}

impl AgentPropertyName {
    #[must_use]
    fn as_str(self) -> &'static str {
        match self {
            Self::Model => "model",
        }
    }
}

pub fn build_typed_workflow_ir<Input, Output>(workflow: &Workflow) -> Result<TypedWorkflowIr, WorkflowRuntimeError>
where
    Input: Serialize + JsonSchema,
    Output: DeserializeOwned + JsonSchema,
{
    let named_schema_types = collect_named_schema_types(workflow);
    let input_type = build_input_type(workflow.find_input(), &named_schema_types)?;
    let secrets_type = build_secrets_type(workflow.find_secrets(), &named_schema_types)?;
    let output_declaration = workflow
        .find_output()
        .ok_or_else(|| WorkflowRuntimeError::MissingDeclaration {
            message: "workflow requires an `output` block".to_string(),
        })?
        .clone();

    let (agents, agent_output_types) = collect_typed_agents(workflow, &named_schema_types)?;
    let workflow_output_type =
        infer_workflow_output_type(&output_declaration, input_type.clone(), secrets_type.clone(), &agent_output_types)?;

    validate_input_type_compatibility::<Input>(input_type.as_ref())?;
    validate_output_type_compatibility::<Output>(&workflow_output_type)?;

    Ok(TypedWorkflowIr {
        input_type,
        secrets_type,
        output_declaration,
        workflow_output_type,
        agents,
    })
}

fn collect_named_schema_types(workflow: &Workflow) -> HashMap<String, TypeExpression> {
    let mut named_schema_types = HashMap::new();

    for declaration in workflow.declarations() {
        let Declaration::Schema(SchemaDeclaration { name, fields, span: _ }) = declaration else {
            continue;
        };

        named_schema_types.insert(name.clone(), TypeExpression::Object(fields.clone()));
    }

    named_schema_types
}

fn build_input_type(
    input_declaration: Option<&InputDeclaration>,
    named_schema_types: &HashMap<String, TypeExpression>,
) -> Result<Option<WorkflowType>, WorkflowRuntimeError> {
    let Some(input_declaration) = input_declaration else {
        return Ok(None);
    };

    let object_type_expression = TypeExpression::Object(input_declaration.fields.clone());
    let input_type = workflow_type_from_dsl(&object_type_expression, named_schema_types)?;

    Ok(Some(input_type))
}

fn build_secrets_type(
    secrets_declaration: Option<&SecretsDeclaration>,
    named_schema_types: &HashMap<String, TypeExpression>,
) -> Result<Option<WorkflowType>, WorkflowRuntimeError> {
    let Some(secrets_declaration) = secrets_declaration else {
        return Ok(None);
    };

    let object_type_expression = TypeExpression::Object(secrets_declaration.fields.clone());
    let secrets_type = workflow_type_from_dsl(&object_type_expression, named_schema_types)?;

    Ok(Some(secrets_type))
}

fn collect_typed_agents(
    workflow: &Workflow,
    named_schema_types: &HashMap<String, TypeExpression>,
) -> Result<(Vec<TypedAgentIr>, HashMap<String, WorkflowType>), WorkflowRuntimeError> {
    let mut agents = Vec::new();
    let mut agent_output_types = HashMap::new();

    for declaration in workflow.declarations() {
        let Declaration::Agent(agent_declaration) = declaration else {
            continue;
        };

        let iteration_output_type = if let Some(output_type_expression) = optional_agent_output_type(agent_declaration) {
            workflow_type_from_dsl(output_type_expression, named_schema_types)?
        } else {
            WorkflowType::String
        };

        let final_output_type = if agent_declaration.for_loop.is_some() {
            WorkflowType::Array {
                item_type: Box::new(iteration_output_type.clone()),
                fixed_length: None,
            }
            .normalize()
        } else {
            iteration_output_type.clone()
        };

        let (provider_name, model_name) = parse_agent_model_binding(agent_declaration)?;
        let dependencies = collect_dependencies_for_agent(agent_declaration);

        agents.push(TypedAgentIr {
            name: agent_declaration.name.clone(),
            declaration: agent_declaration.clone(),
            provider_name,
            model_name,
            iteration_output_type,
            final_output_type: final_output_type.clone(),
            dependencies,
        });

        agent_output_types.insert(agent_declaration.name.clone(), final_output_type);
    }

    Ok((agents, agent_output_types))
}

fn optional_agent_output_type(agent_declaration: &AgentDeclaration) -> Option<&TypeExpression> {
    for agent_property in &agent_declaration.properties {
        if let AgentProperty::Output(output_type_expression) = agent_property {
            return Some(output_type_expression);
        }
    }

    None
}

fn collect_dependencies_for_agent(agent_declaration: &AgentDeclaration) -> Vec<String> {
    let mut dependencies = HashSet::new();

    if let Some(for_loop) = &agent_declaration.for_loop {
        collect_agent_dependencies(&for_loop.iterable, &mut dependencies);
    }

    for agent_property in &agent_declaration.properties {
        match agent_property {
            AgentProperty::Model(expression)
            | AgentProperty::Prompt(expression)
            | AgentProperty::Context(expression)
            | AgentProperty::Inference(expression)
            | AgentProperty::Tools(expression)
            | AgentProperty::Custom {
                name: _,
                value: expression,
            } => {
                collect_agent_dependencies(expression, &mut dependencies);
            }
            AgentProperty::Output(_) => {}
        }
    }

    dependencies.remove(&agent_declaration.name);

    let mut sorted_dependencies = dependencies.into_iter().collect::<Vec<_>>();
    sorted_dependencies.sort();

    sorted_dependencies
}

fn infer_workflow_output_type(
    output_declaration: &OutputDeclaration,
    input_type: Option<WorkflowType>,
    secrets_type: Option<WorkflowType>,
    agent_output_types: &HashMap<String, WorkflowType>,
) -> Result<WorkflowType, WorkflowRuntimeError> {
    let inference_context = TypeInferenceContext {
        input_type,
        secrets_type,
        agent_output_types: agent_output_types.clone(),
        local_binding_types: HashMap::new(),
    };

    let mut output_fields = BTreeMap::new();

    for output_field in &output_declaration.fields {
        let field_type = infer_expression_type(&output_field.value, &inference_context, "workflow output type inference")?;
        output_fields.insert(output_field.name.clone(), field_type);
    }

    Ok(WorkflowType::Object(output_fields).normalize())
}

fn validate_input_type_compatibility<Input>(input_type: Option<&WorkflowType>) -> Result<(), WorkflowRuntimeError>
where
    Input: Serialize + JsonSchema,
{
    let rust_input_type = workflow_type_from_rust_schema::<Input>()?;

    if let Some(expected_input_type) = input_type {
        if ensure_type_matches(expected_input_type, &rust_input_type) {
            return Ok(());
        }

        Err(WorkflowRuntimeError::InputTypeMismatch {
            expected: expected_input_type.to_string(),
            found: rust_input_type.to_string(),
        })
    } else {
        if is_no_input_type(&rust_input_type) {
            return Ok(());
        }

        Err(WorkflowRuntimeError::InputTypeMismatch {
            expected: "no input".to_string(),
            found: rust_input_type.to_string(),
        })
    }
}

fn validate_output_type_compatibility<Output>(workflow_output_type: &WorkflowType) -> Result<(), WorkflowRuntimeError>
where
    Output: DeserializeOwned + JsonSchema,
{
    let rust_output_type = workflow_type_from_rust_schema::<Output>()?;

    if ensure_type_matches(workflow_output_type, &rust_output_type) {
        return Ok(());
    }

    Err(WorkflowRuntimeError::OutputTypeMismatch {
        expected: workflow_output_type.to_string(),
        found: rust_output_type.to_string(),
    })
}

fn is_no_input_type(workflow_type: &WorkflowType) -> bool {
    match workflow_type {
        WorkflowType::Null => true,
        WorkflowType::Object(fields) => fields.is_empty(),
        WorkflowType::String
        | WorkflowType::Integer
        | WorkflowType::Float
        | WorkflowType::Boolean
        | WorkflowType::StringEnum(_)
        | WorkflowType::Array {
            item_type: _,
            fixed_length: _,
        }
        | WorkflowType::Tuple(_)
        | WorkflowType::Union(_) => false,
    }
}

fn parse_agent_model_binding(agent_declaration: &AgentDeclaration) -> Result<(String, String), WorkflowRuntimeError> {
    let model_expression = required_agent_property_expression(agent_declaration, AgentPropertyName::Model)?;
    let Expression::FunctionCall(model_call) = model_expression else {
        return Err(WorkflowRuntimeError::InvalidAgentProperty {
            agent_name: agent_declaration.name.clone(),
            property: AgentPropertyName::Model.as_str().to_string(),
            message: "model must be a provider call like provider_name(\"model\")".to_string(),
        });
    };

    if !model_call.callee.accesses.is_empty() {
        return Err(WorkflowRuntimeError::InvalidAgentProperty {
            agent_name: agent_declaration.name.clone(),
            property: AgentPropertyName::Model.as_str().to_string(),
            message: "model function callee must be a direct provider name".to_string(),
        });
    }

    let provider_name = model_call
        .callee
        .root
        .as_identifier()
        .ok_or_else(|| WorkflowRuntimeError::InvalidAgentProperty {
            agent_name: agent_declaration.name.clone(),
            property: AgentPropertyName::Model.as_str().to_string(),
            message: "model provider name must be an identifier".to_string(),
        })?
        .to_string();

    let mut detected_model_names = Vec::<String>::new();

    for call_argument in &model_call.arguments {
        match call_argument {
            CallArgument::Positional(expression) => {
                if let Expression::StringLiteral(model_name) = expression {
                    detected_model_names.push(model_name.clone());
                }
            }
            CallArgument::Named(named_argument) if named_argument.name == AgentPropertyName::Model.as_str() => {
                let Expression::StringLiteral(model_name) = &named_argument.value else {
                    return Err(WorkflowRuntimeError::InvalidAgentProperty {
                        agent_name: agent_declaration.name.clone(),
                        property: AgentPropertyName::Model.as_str().to_string(),
                        message: "named `model` argument must be a string".to_string(),
                    });
                };

                detected_model_names.push(model_name.clone());
            }
            CallArgument::Named(_) => {}
        }
    }

    if detected_model_names.is_empty() {
        return Err(WorkflowRuntimeError::InvalidAgentProperty {
            agent_name: agent_declaration.name.clone(),
            property: AgentPropertyName::Model.as_str().to_string(),
            message: "missing model name argument".to_string(),
        });
    }

    let model_name = detected_model_names[0].clone();

    if detected_model_names.iter().any(|candidate_name| candidate_name != &model_name) {
        return Err(WorkflowRuntimeError::InvalidAgentProperty {
            agent_name: agent_declaration.name.clone(),
            property: AgentPropertyName::Model.as_str().to_string(),
            message: "ambiguous model name arguments".to_string(),
        });
    }

    Ok((provider_name, model_name))
}

fn required_agent_property_expression(
    agent_declaration: &AgentDeclaration,
    property_name: AgentPropertyName,
) -> Result<&Expression, WorkflowRuntimeError> {
    optional_agent_property_expression(agent_declaration, property_name).ok_or_else(|| WorkflowRuntimeError::InvalidAgentProperty {
        agent_name: agent_declaration.name.clone(),
        property: property_name.as_str().to_string(),
        message: "property is required".to_string(),
    })
}

fn optional_agent_property_expression(agent_declaration: &AgentDeclaration, property_name: AgentPropertyName) -> Option<&Expression> {
    for agent_property in &agent_declaration.properties {
        match agent_property {
            AgentProperty::Model(expression) if property_name == AgentPropertyName::Model => return Some(expression),
            AgentProperty::Model(_)
            | AgentProperty::Prompt(_)
            | AgentProperty::Output(_)
            | AgentProperty::Context(_)
            | AgentProperty::Inference(_)
            | AgentProperty::Tools(_)
            | AgentProperty::Custom { name: _, value: _ } => {}
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::build_typed_workflow_ir;
    use crate::parse_inline_workflow;
    use crate::runtime::error::WorkflowRuntimeError;
    use crate::runtime::types::WorkflowType;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    use std::collections::BTreeMap;

    #[derive(Debug, Serialize, JsonSchema)]
    struct TestInput {
        topic: String,
    }

    #[derive(Debug, Deserialize, JsonSchema)]
    #[allow(dead_code)]
    struct TestOutput {
        values: Vec<i64>,
    }

    #[test]
    fn builds_typed_ir_with_dependencies_and_loop_output_types() {
        let workflow = parse_inline_workflow! {
            provider openai {
                driver: "openai"
                models: ["model-a"]
            }

            input {
                topic: string
            }

            agent researcher {
                model: openai("model-a")
                prompt: input.topic
                output: string
            }

            agent scorer for item in [agent.researcher] {
                model: openai("model-a")
                prompt: "score {{ item }}"
                output: number
            }

            output {
                values: agent.scorer
            }
        };

        let typed_workflow_ir = build_typed_workflow_ir::<TestInput, TestOutput>(&workflow).expect("typecheck should succeed");
        let scorer_agent = typed_workflow_ir
            .agents
            .iter()
            .find(|typed_agent| typed_agent.name == "scorer")
            .expect("scorer agent should be present");

        assert_eq!(scorer_agent.dependencies, vec!["researcher".to_string()]);
        assert_eq!(
            scorer_agent.final_output_type,
            WorkflowType::Array {
                item_type: Box::new(WorkflowType::Integer),
                fixed_length: None,
            }
            .normalize()
        );

        let mut expected_output_fields = BTreeMap::new();
        expected_output_fields.insert(
            "values".to_string(),
            WorkflowType::Array {
                item_type: Box::new(WorkflowType::Integer),
                fixed_length: None,
            }
            .normalize(),
        );

        assert_eq!(
            typed_workflow_ir.workflow_output_type,
            WorkflowType::Object(expected_output_fields).normalize()
        );
    }

    #[test]
    fn fails_typecheck_when_agent_model_property_is_missing() {
        #[derive(Debug, Deserialize, JsonSchema)]
        #[allow(dead_code)]
        struct Output {
            value: String,
        }

        let workflow = parse_inline_workflow! {
            agent missing_model {
                prompt: "hello"
                output: string
            }

            output {
                value: agent.missing_model
            }
        };

        let typecheck_result = build_typed_workflow_ir::<(), Output>(&workflow);

        assert!(matches!(
            typecheck_result,
            Err(WorkflowRuntimeError::InvalidAgentProperty { property, .. }) if property == "model"
        ));
    }
}
