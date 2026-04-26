use crate::dsl::{
    AgentDeclaration, AgentProperty, Declaration, Expression, InputDeclaration, OutputDeclaration, ProviderDeclaration, SchemaDeclaration,
    SecretsDeclaration, ToolDeclaration, TypeExpression, Workflow,
};
use crate::runtime::error::WorkflowRuntimeError;
use crate::runtime::expression::collect_agent_dependencies;
use crate::runtime::type_inference::TypeInferenceContext;
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
    pub model_expression: Expression,
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

    let tool_types = collect_tool_types(workflow, &named_schema_types)?;
    let (agents, agent_output_types) = collect_typed_agents(workflow, &named_schema_types)?;
    let workflow_output_type = infer_workflow_output_type(
        workflow,
        &output_declaration,
        input_type.clone(),
        secrets_type.clone(),
        &agent_output_types,
        &tool_types,
    )?;

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

struct ToolTypes {
    input: HashMap<String, WorkflowType>,
    bindings: HashMap<String, WorkflowType>,
    output: HashMap<String, WorkflowType>,
}

fn collect_tool_types(
    workflow: &Workflow,
    named_schema_types: &HashMap<String, TypeExpression>,
) -> Result<ToolTypes, WorkflowRuntimeError> {
    let mut input = HashMap::new();
    let mut bindings = HashMap::new();
    let mut output = HashMap::new();

    for declaration in workflow.declarations() {
        let Declaration::Tool(ToolDeclaration {
            name,
            input_fields,
            binding_fields,
            output_fields,
            ..
        }) = declaration
        else {
            continue;
        };

        input.insert(
            name.clone(),
            workflow_type_from_dsl(&TypeExpression::Object(input_fields.clone()), named_schema_types)?,
        );
        bindings.insert(
            name.clone(),
            workflow_type_from_dsl(&TypeExpression::Object(binding_fields.clone()), named_schema_types)?,
        );
        output.insert(
            name.clone(),
            workflow_type_from_dsl(&TypeExpression::Object(output_fields.clone()), named_schema_types)?,
        );
    }

    Ok(ToolTypes { input, bindings, output })
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

        let iteration_output_type_expression = agent_declaration.inferred_iteration_output_type_expression();
        let final_output_type_expression = agent_declaration.inferred_final_output_type_expression();
        let iteration_output_type = workflow_type_from_dsl(&iteration_output_type_expression, named_schema_types)?;
        let final_output_type = workflow_type_from_dsl(&final_output_type_expression, named_schema_types)?;

        let (provider_name, model_expression) = parse_agent_model_binding(agent_declaration)?;
        let provider_declaration = workflow.find_provider(&provider_name);
        let dependencies = collect_dependencies_for_agent(agent_declaration, provider_declaration);

        agents.push(TypedAgentIr {
            name: agent_declaration.name.clone(),
            declaration: agent_declaration.clone(),
            provider_name,
            model_expression,
            iteration_output_type,
            final_output_type: final_output_type.clone(),
            dependencies,
        });

        agent_output_types.insert(agent_declaration.name.clone(), final_output_type);
    }

    Ok((agents, agent_output_types))
}

fn collect_dependencies_for_agent(agent_declaration: &AgentDeclaration, provider_declaration: Option<&ProviderDeclaration>) -> Vec<String> {
    let mut dependencies = HashSet::new();

    if let Some(provider_declaration) = provider_declaration {
        for provider_property in &provider_declaration.properties {
            collect_agent_dependencies(&provider_property.value, &mut dependencies);
        }
    }

    if let Some(for_loop) = &agent_declaration.for_loop {
        collect_agent_dependencies(&for_loop.iterable, &mut dependencies);
    }

    for agent_property in &agent_declaration.properties {
        match agent_property {
            AgentProperty::Model(expression)
            | AgentProperty::Prompt(expression)
            | AgentProperty::Context(expression)
            | AgentProperty::Inference(expression)
            | AgentProperty::Tools(expression) => {
                collect_agent_dependencies(expression, &mut dependencies);
            }
            AgentProperty::Let(let_binding) => {
                collect_agent_dependencies(&let_binding.value, &mut dependencies);
            }
            AgentProperty::Output {
                output_type_expression: _,
                description: _,
            } => {}
        }
    }

    dependencies.remove(&agent_declaration.name);

    let mut sorted_dependencies = dependencies.into_iter().collect::<Vec<_>>();
    sorted_dependencies.sort();

    sorted_dependencies
}

fn infer_workflow_output_type(
    workflow: &Workflow,
    output_declaration: &OutputDeclaration,
    input_type: Option<WorkflowType>,
    secrets_type: Option<WorkflowType>,
    agent_output_types: &HashMap<String, WorkflowType>,
    tool_types: &ToolTypes,
) -> Result<WorkflowType, WorkflowRuntimeError> {
    let mut inference_context = TypeInferenceContext {
        input_type,
        secrets_type,
        agent_output_types: agent_output_types.clone(),
        tool_input_types: tool_types.input.clone(),
        tool_binding_types: tool_types.bindings.clone(),
        tool_output_types: tool_types.output.clone(),
        local_binding_types: HashMap::new(),
    };

    for declaration in workflow.declarations() {
        let Declaration::Let(let_binding) = declaration else {
            continue;
        };

        let binding_type = let_binding
            .value
            .infer_type(&inference_context, &format!("let binding `{}` type inference", let_binding.name))?;
        inference_context.local_binding_types.insert(let_binding.name.clone(), binding_type);
    }

    let mut output_fields = BTreeMap::new();

    for output_field in &output_declaration.fields {
        let field_type = output_field
            .value
            .infer_type(&inference_context, "workflow output type inference")?;
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

fn parse_agent_model_binding(agent_declaration: &AgentDeclaration) -> Result<(String, Expression), WorkflowRuntimeError> {
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
        .identifier_name()
        .ok_or_else(|| WorkflowRuntimeError::InvalidAgentProperty {
            agent_name: agent_declaration.name.clone(),
            property: AgentPropertyName::Model.as_str().to_string(),
            message: "model provider name must be an identifier".to_string(),
        })?;

    let model_argument_expressions = model_call.model_argument_expressions();

    if model_argument_expressions.is_empty() {
        return Err(WorkflowRuntimeError::InvalidAgentProperty {
            agent_name: agent_declaration.name.clone(),
            property: AgentPropertyName::Model.as_str().to_string(),
            message: "missing model name argument".to_string(),
        });
    }

    if model_argument_expressions.len() > 1 {
        return Err(WorkflowRuntimeError::InvalidAgentProperty {
            agent_name: agent_declaration.name.clone(),
            property: AgentPropertyName::Model.as_str().to_string(),
            message: "ambiguous model name arguments".to_string(),
        });
    }

    Ok((provider_name.to_string(), model_argument_expressions[0].clone()))
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
            | AgentProperty::Output {
                output_type_expression: _,
                description: _,
            }
            | AgentProperty::Context(_)
            | AgentProperty::Inference(_)
            | AgentProperty::Tools(_)
            | AgentProperty::Let(_) => {}
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
    fn includes_provider_expression_dependencies_in_agent_execution_dependencies() {
        #[derive(Debug, Deserialize, JsonSchema)]
        #[allow(dead_code)]
        struct Output {
            value: String,
        }

        let workflow = parse_inline_workflow! {
            provider base_provider {
                driver: "openai"
                endpoint: "http://localhost:1234/v1"
                api_key: "test-api-key"
                models: ["model-a"]
            }

            provider dynamic_provider {
                driver: "openai"
                endpoint: agent.base.endpoint
                api_key: "test-api-key"
                models: ["model-a"]
            }

            agent base {
                model: base_provider("model-a")
                prompt: "base"
                output: {
                    endpoint: string
                }
            }

            agent dependent {
                model: dynamic_provider("model-a")
                prompt: "dependent"
                output: string
            }

            output {
                value: agent.dependent
            }
        };

        let typed_workflow_ir = build_typed_workflow_ir::<(), Output>(&workflow).expect("typecheck should succeed");
        let dependent_agent = typed_workflow_ir
            .agents
            .iter()
            .find(|typed_agent| typed_agent.name == "dependent")
            .expect("dependent agent should be present");

        assert_eq!(dependent_agent.dependencies, vec!["base".to_string()]);
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
