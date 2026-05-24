use crate::dsl::DslProperty;
use crate::dsl::{
    structure, AgentDeclaration, AgentExpressionPropertyName, AgentForLoop, AgentForLoopPattern, AgentProperty, Declaration, Expression,
    InputDeclaration, ModelDeclaration, ModelUsage, ObjectField, OutputDeclaration, ProviderDeclaration, SecretsDeclaration,
    ToolDeclaration, TypeExpression, Workflow,
};
use crate::semantic::support::type_inference::{infer_expression_type, TypeInferenceContext};
use crate::semantic::support::types::{ensure_type_matches, workflow_type_from_rust_schema, WorkflowSchemaCache, WorkflowType};
use crate::semantic::WorkflowSemanticError;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct TypedWorkflowIr {
    pub input_type: Option<WorkflowType>,
    pub secrets_type: Option<WorkflowType>,
    pub output_declaration: OutputDeclaration,
    pub workflow_output_type: WorkflowType,
    pub tools: Vec<TypedToolIr>,
    pub agents: Vec<TypedAgentIr>,
}

#[derive(Debug, Clone)]
pub struct TypedToolIr {
    pub name: String,
    pub declaration: ToolDeclaration,
    pub input_type: WorkflowType,
    pub binding_type: WorkflowType,
    pub output_type: WorkflowType,
}

#[derive(Debug, Clone)]
pub struct TypedAgentIr {
    pub name: String,
    pub declaration: AgentDeclaration,
    pub provider_name: String,
    pub model_name: String,
    pub model_id_expression: Expression,
    pub inference_fields: Vec<ObjectField>,
    pub iteration_output_type: WorkflowType,
    pub final_output_type: WorkflowType,
    pub dependencies: Vec<String>,
}

impl TypedToolIr {
    #[must_use]
    pub fn input_schema(&self) -> Value {
        self.input_type.json_schema_value()
    }

    #[must_use]
    pub fn input_schema_with_cache(&self, schema_cache: &mut WorkflowSchemaCache) -> Value {
        self.input_type.json_schema_value_with_cache(schema_cache)
    }

    #[must_use]
    pub fn model_input_schema(&self, bindings: &Value) -> Value {
        let mut schema_cache = WorkflowSchemaCache::disabled();

        self.model_input_schema_with_cache(bindings, &mut schema_cache)
    }

    #[must_use]
    pub fn model_input_schema_with_cache(&self, bindings: &Value, schema_cache: &mut WorkflowSchemaCache) -> Value {
        let mut input_schema = self.input_schema_with_cache(schema_cache);
        let Some(binding_object) = bindings.as_object() else {
            return input_schema;
        };
        let binding_names = binding_object.keys().cloned().collect::<HashSet<_>>();

        if let Some(properties) = input_schema.get_mut("properties").and_then(Value::as_object_mut) {
            for binding_name in &binding_names {
                properties.remove(binding_name);
            }
        }

        let mut remove_required = false;

        if let Some(required_fields) = input_schema.get_mut("required").and_then(Value::as_array_mut) {
            required_fields.retain(|required_field| {
                required_field
                    .as_str()
                    .is_none_or(|required_field_name| !binding_names.contains(required_field_name))
            });
            remove_required = required_fields.is_empty();
        }

        if remove_required {
            if let Some(schema_object) = input_schema.as_object_mut() {
                schema_object.remove("required");
            }
        }

        input_schema
    }

    #[must_use]
    pub fn output_schema(&self) -> Value {
        self.output_type.json_schema_value()
    }

    #[must_use]
    pub fn output_schema_with_cache(&self, schema_cache: &mut WorkflowSchemaCache) -> Value {
        self.output_type.json_schema_value_with_cache(schema_cache)
    }
}

pub fn build_typed_workflow_ir<Input, Output>(workflow: &Workflow) -> Result<TypedWorkflowIr, WorkflowSemanticError>
where
    Input: Serialize + JsonSchema,
    Output: DeserializeOwned + JsonSchema,
{
    let named_schema_types = workflow.named_schema_types();
    let input_type = build_input_type(workflow.find_input(), &named_schema_types)?;
    let secrets_type = build_secrets_type(workflow.find_secrets(), &named_schema_types)?;
    let output_declaration = workflow
        .find_output()
        .ok_or_else(|| WorkflowSemanticError::MissingDeclaration {
            message: "workflow requires an `output` block".to_string(),
        })?
        .clone();

    let tool_types = collect_tool_types(workflow, &named_schema_types)?;
    let (agents, agent_output_types) =
        collect_typed_agents(workflow, &named_schema_types, input_type.clone(), secrets_type.clone(), &tool_types)?;
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
        tools: tool_types.tools,
        agents,
    })
}

pub fn build_dynamic_typed_workflow_ir(workflow: &Workflow) -> Result<TypedWorkflowIr, WorkflowSemanticError> {
    let named_schema_types = workflow.named_schema_types();
    let input_type = build_input_type(workflow.find_input(), &named_schema_types)?;
    let secrets_type = build_secrets_type(workflow.find_secrets(), &named_schema_types)?;
    let output_declaration = workflow
        .find_output()
        .ok_or_else(|| WorkflowSemanticError::MissingDeclaration {
            message: "workflow requires an `output` block".to_string(),
        })?
        .clone();

    let tool_types = collect_tool_types(workflow, &named_schema_types)?;
    let (agents, agent_output_types) =
        collect_typed_agents(workflow, &named_schema_types, input_type.clone(), secrets_type.clone(), &tool_types)?;
    let workflow_output_type = infer_workflow_output_type(
        workflow,
        &output_declaration,
        input_type.clone(),
        secrets_type.clone(),
        &agent_output_types,
        &tool_types,
    )?;

    Ok(TypedWorkflowIr {
        input_type,
        secrets_type,
        output_declaration,
        workflow_output_type,
        tools: tool_types.tools,
        agents,
    })
}

struct ToolTypes {
    tools: Vec<TypedToolIr>,
    input: HashMap<String, WorkflowType>,
    bindings: HashMap<String, WorkflowType>,
    output: HashMap<String, WorkflowType>,
}

fn collect_tool_types(
    workflow: &Workflow,
    named_schema_types: &HashMap<String, TypeExpression>,
) -> Result<ToolTypes, WorkflowSemanticError> {
    let mut tools = Vec::new();
    let mut input = HashMap::new();
    let mut bindings = HashMap::new();
    let mut output = HashMap::new();

    for tool_declaration in workflow.tool_declarations() {
        let input_type = TypeExpression::Object(tool_declaration.input_fields.clone()).to_workflow_type(named_schema_types)?;
        let binding_type = TypeExpression::Object(tool_declaration.binding_fields.clone()).to_workflow_type(named_schema_types)?;
        let output_type = if tool_declaration.has_untyped_mcp_output() {
            WorkflowType::Any
        } else {
            TypeExpression::Object(tool_declaration.output_fields.clone()).to_workflow_type(named_schema_types)?
        };
        input.insert(tool_declaration.name.clone(), input_type.clone());
        bindings.insert(tool_declaration.name.clone(), binding_type.clone());
        output.insert(tool_declaration.name.clone(), output_type.clone());
        tools.push(TypedToolIr {
            name: tool_declaration.name.clone(),
            declaration: tool_declaration.clone(),
            input_type,
            binding_type,
            output_type,
        });
    }

    Ok(ToolTypes {
        tools,
        input,
        bindings,
        output,
    })
}

fn build_input_type(
    input_declaration: Option<&InputDeclaration>,
    named_schema_types: &HashMap<String, TypeExpression>,
) -> Result<Option<WorkflowType>, WorkflowSemanticError> {
    let Some(input_declaration) = input_declaration else {
        return Ok(None);
    };

    let object_type_expression = TypeExpression::Object(input_declaration.fields.clone());
    let input_type = object_type_expression.to_workflow_type(named_schema_types)?;

    Ok(Some(input_type))
}

fn build_secrets_type(
    secrets_declaration: Option<&SecretsDeclaration>,
    named_schema_types: &HashMap<String, TypeExpression>,
) -> Result<Option<WorkflowType>, WorkflowSemanticError> {
    let Some(secrets_declaration) = secrets_declaration else {
        return Ok(None);
    };

    let object_type_expression = TypeExpression::Object(secrets_declaration.fields.clone());
    let secrets_type = object_type_expression.to_workflow_type(named_schema_types)?;

    Ok(Some(secrets_type))
}

fn collect_typed_agents(
    workflow: &Workflow,
    named_schema_types: &HashMap<String, TypeExpression>,
    input_type: Option<WorkflowType>,
    secrets_type: Option<WorkflowType>,
    tool_types: &ToolTypes,
) -> Result<(Vec<TypedAgentIr>, HashMap<String, WorkflowType>), WorkflowSemanticError> {
    let provider_declarations = workflow.provider_declarations_by_name();
    let model_declarations = workflow.model_declarations_by_name();
    let mut agents = Vec::new();
    let mut agent_output_types = HashMap::new();
    let mut workflow_type_inference_context = TypeInferenceContext {
        input_type: input_type.clone(),
        secrets_type: secrets_type.clone(),
        agent_output_types: HashMap::new(),
        tool_input_types: tool_types.input.clone(),
        tool_binding_types: tool_types.bindings.clone(),
        tool_output_types: tool_types.output.clone(),
        local_binding_types: HashMap::new(),
    };
    let workflow_dynamic_fields = workflow
        .dynamic_blocks()
        .flat_map(|dynamic_block| dynamic_block.fields.as_slice())
        .collect::<Vec<_>>();

    infer_dynamic_field_types(workflow_dynamic_fields, &mut workflow_type_inference_context)?;

    let workflow_dynamic_field_types = workflow_type_inference_context.local_binding_types.clone();

    for declaration in workflow.declarations() {
        let Declaration::Agent(agent_declaration) = declaration else {
            continue;
        };

        let iteration_output_type_expression = agent_declaration.inferred_iteration_output_type_expression();
        let final_output_type_expression = agent_declaration.inferred_final_output_type_expression();
        let mut agent_type_inference_context = TypeInferenceContext {
            input_type: input_type.clone(),
            secrets_type: secrets_type.clone(),
            agent_output_types: agent_output_types.clone(),
            tool_input_types: tool_types.input.clone(),
            tool_binding_types: tool_types.bindings.clone(),
            tool_output_types: tool_types.output.clone(),
            local_binding_types: workflow_dynamic_field_types.clone(),
        };

        if let Some(agent_for_loop) = &agent_declaration.for_loop {
            let for_loop_binding_types = agent_for_loop.binding_types(&agent_type_inference_context)?;

            agent_type_inference_context.local_binding_types.extend(for_loop_binding_types);
        }

        let agent_dynamic_fields = agent_declaration
            .dynamic_blocks()
            .flat_map(|dynamic_block| dynamic_block.fields.iter())
            .collect::<Vec<_>>();

        infer_dynamic_field_types(agent_dynamic_fields, &mut agent_type_inference_context)?;

        let iteration_output_type = iteration_output_type_expression.infer_agent_output_type(
            &agent_type_inference_context,
            named_schema_types,
            &format!("agent `{}` output type", agent_declaration.name),
        )?;
        let final_output_type = final_output_type_expression.infer_agent_output_type(
            &agent_type_inference_context,
            named_schema_types,
            &format!("agent `{}` final output type", agent_declaration.name),
        )?;

        let model_usage = required_agent_model_usage(agent_declaration)?;
        let model_name = model_usage
            .model_name()
            .ok_or_else(|| WorkflowSemanticError::InvalidAgentProperty {
                agent_name: agent_declaration.name.clone(),
                property: structure::Agent::new().model.definition().name.to_string(),
                message: "model must reference a model profile like model.fast".to_string(),
            })?;
        let model_declaration = model_declarations
            .get(model_name)
            .copied()
            .ok_or_else(|| WorkflowSemanticError::InvalidAgentProperty {
                agent_name: agent_declaration.name.clone(),
                property: structure::Agent::new().model.definition().name.to_string(),
                message: format!("model profile `{model_name}` is not declared"),
            })?;
        let model_id_expression = model_declaration
            .id_expression()
            .ok_or_else(|| WorkflowSemanticError::InvalidAgentProperty {
                agent_name: agent_declaration.name.clone(),
                property: structure::Agent::new().model.definition().name.to_string(),
                message: format!("model profile `{model_name}` is missing `id`"),
            })?
            .clone();
        let provider_name = model_declaration.provider_name.clone();
        required_agent_property_expression(agent_declaration, AgentExpressionPropertyName::Instruction)?;

        let provider_declaration = provider_declarations.get(provider_name.as_str()).copied();
        let inference_fields = agent_declaration.effective_inference_fields(provider_declaration, model_declaration);
        let dependencies = collect_dependencies_for_agent(agent_declaration, provider_declaration, model_declaration);

        agents.push(TypedAgentIr {
            name: agent_declaration.name.clone(),
            declaration: agent_declaration.clone(),
            provider_name,
            model_name: model_name.to_string(),
            model_id_expression,
            inference_fields,
            iteration_output_type,
            final_output_type: final_output_type.clone(),
            dependencies,
        });

        agent_output_types.insert(agent_declaration.name.clone(), final_output_type);
    }

    Ok((agents, agent_output_types))
}

trait AgentForLoopTypeExt {
    fn binding_types(&self, type_inference_context: &TypeInferenceContext) -> Result<HashMap<String, WorkflowType>, WorkflowSemanticError>;
}

impl AgentForLoopTypeExt for AgentForLoop {
    fn binding_types(&self, type_inference_context: &TypeInferenceContext) -> Result<HashMap<String, WorkflowType>, WorkflowSemanticError> {
        let iterable_type = infer_expression_type(&self.iterable, type_inference_context, "agent for-loop iterable type inference")?;
        let item_type = iterable_type
            .array_item_type()
            .ok_or_else(|| WorkflowSemanticError::ExpressionEvaluation {
                context: "agent for-loop iterable type inference".to_string(),
                message: format!("for-loop iterable must be an array, found {iterable_type}"),
            })?;
        let mut binding_types = HashMap::new();

        match &self.pattern {
            AgentForLoopPattern::Identifier(identifier) => {
                binding_types.insert(identifier.clone(), item_type);
            }
            AgentForLoopPattern::ObjectDestructuring(field_names) => {
                for field_name in field_names {
                    let field_type = item_type
                        .field_type(field_name)
                        .ok_or_else(|| WorkflowSemanticError::ExpressionEvaluation {
                            context: "agent for-loop binding type inference".to_string(),
                            message: format!("for-loop item type does not include field `{field_name}`"),
                        })?;

                    binding_types.insert(field_name.clone(), field_type);
                }
            }
        }

        Ok(binding_types)
    }
}

trait WorkflowTypeArrayExt {
    fn array_item_type(&self) -> Option<WorkflowType>;
}

impl WorkflowTypeArrayExt for WorkflowType {
    fn array_item_type(&self) -> Option<WorkflowType> {
        match self {
            Self::Array {
                item_type,
                fixed_length: _,
            } => Some((**item_type).clone()),
            Self::Union(union_members) => union_members.iter().find_map(Self::array_item_type),
            Self::Any
            | Self::String
            | Self::Integer
            | Self::Float
            | Self::Boolean
            | Self::Null
            | Self::AnyObject
            | Self::StringEnum(_)
            | Self::Tuple(_)
            | Self::Object(_)
            | Self::Variant {
                discriminator: _,
                cases: _,
            } => None,
        }
    }
}

fn collect_dependencies_for_agent(
    agent_declaration: &AgentDeclaration,
    provider_declaration: Option<&ProviderDeclaration>,
    model_declaration: &ModelDeclaration,
) -> Vec<String> {
    let mut dependencies = HashSet::new();

    if let Some(provider_declaration) = provider_declaration {
        for provider_property in &provider_declaration.properties {
            provider_property.value.collect_agent_dependencies(&mut dependencies);
        }
    }

    for model_property in &model_declaration.properties {
        model_property.value.collect_agent_dependencies(&mut dependencies);
    }

    if let Some(for_loop) = &agent_declaration.for_loop {
        for_loop.iterable.collect_agent_dependencies(&mut dependencies);
    }

    for agent_property in &agent_declaration.properties {
        match agent_property {
            AgentProperty::InvalidModel(expression)
            | AgentProperty::Instruction(expression)
            | AgentProperty::Context(expression)
            | AgentProperty::Uses(expression) => {
                expression.collect_agent_dependencies(&mut dependencies);
            }
            AgentProperty::Model(model_usage) => {
                for model_property in &model_usage.properties {
                    model_property.value.collect_agent_dependencies(&mut dependencies);
                }
            }
            AgentProperty::Dynamic(dynamic_block) => {
                for field in &dynamic_block.fields {
                    field.value.collect_agent_dependencies(&mut dependencies);
                }
            }
            AgentProperty::Output { fields: _, span: _ } | AgentProperty::Unknown { name: _, span: _ } => {}
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
) -> Result<WorkflowType, WorkflowSemanticError> {
    let mut inference_context = TypeInferenceContext {
        input_type,
        secrets_type,
        agent_output_types: agent_output_types.clone(),
        tool_input_types: tool_types.input.clone(),
        tool_binding_types: tool_types.bindings.clone(),
        tool_output_types: tool_types.output.clone(),
        local_binding_types: HashMap::new(),
    };

    let dynamic_fields = workflow
        .dynamic_blocks()
        .flat_map(|dynamic_block| dynamic_block.fields.as_slice())
        .collect::<Vec<_>>();

    infer_dynamic_field_types(dynamic_fields, &mut inference_context)?;

    let mut output_fields = BTreeMap::new();

    for output_field in &output_declaration.fields {
        let field_type = output_field
            .value
            .infer_type(&inference_context, "workflow output type inference")?;
        output_fields.insert(output_field.name.clone(), field_type);
    }

    Ok(WorkflowType::Object(output_fields).normalize())
}

fn infer_dynamic_field_types(
    dynamic_fields: Vec<&ObjectField>,
    inference_context: &mut TypeInferenceContext,
) -> Result<(), WorkflowSemanticError> {
    let mut pending_dynamic_fields = dynamic_fields;

    while !pending_dynamic_fields.is_empty() {
        let pending_count_before_pass = pending_dynamic_fields.len();
        let mut last_error = None;

        pending_dynamic_fields.retain(|dynamic_field| {
            match dynamic_field
                .value
                .infer_type(inference_context, &format!("dynamic field `{}` type inference", dynamic_field.name))
            {
                Ok(field_type) => {
                    inference_context.local_binding_types.insert(dynamic_field.name.clone(), field_type);

                    false
                }
                Err(error) => {
                    last_error = Some(error);

                    true
                }
            }
        });

        if pending_dynamic_fields.len() == pending_count_before_pass {
            if let Some(error) = last_error {
                return Err(error);
            }

            break;
        }
    }

    Ok(())
}

fn validate_input_type_compatibility<Input>(input_type: Option<&WorkflowType>) -> Result<(), WorkflowSemanticError>
where
    Input: Serialize + JsonSchema,
{
    let rust_input_type = workflow_type_from_rust_schema::<Input>()?;

    if let Some(expected_input_type) = input_type {
        if ensure_type_matches(expected_input_type, &rust_input_type) {
            return Ok(());
        }

        Err(WorkflowSemanticError::InputTypeMismatch {
            expected: expected_input_type.to_string(),
            found: rust_input_type.to_string(),
        })
    } else {
        if is_no_input_type(&rust_input_type) {
            return Ok(());
        }

        Err(WorkflowSemanticError::InputTypeMismatch {
            expected: "no input".to_string(),
            found: rust_input_type.to_string(),
        })
    }
}

fn validate_output_type_compatibility<Output>(workflow_output_type: &WorkflowType) -> Result<(), WorkflowSemanticError>
where
    Output: DeserializeOwned + JsonSchema,
{
    let rust_output_type = workflow_type_from_rust_schema::<Output>()?;

    if ensure_type_matches(workflow_output_type, &rust_output_type) {
        return Ok(());
    }

    Err(WorkflowSemanticError::OutputTypeMismatch {
        expected: workflow_output_type.to_string(),
        found: rust_output_type.to_string(),
    })
}

fn is_no_input_type(workflow_type: &WorkflowType) -> bool {
    match workflow_type {
        WorkflowType::Null => true,
        WorkflowType::Object(fields) => fields.is_empty(),
        WorkflowType::Any
        | WorkflowType::String
        | WorkflowType::Integer
        | WorkflowType::Float
        | WorkflowType::Boolean
        | WorkflowType::AnyObject
        | WorkflowType::StringEnum(_)
        | WorkflowType::Array {
            item_type: _,
            fixed_length: _,
        }
        | WorkflowType::Tuple(_)
        | WorkflowType::Variant {
            discriminator: _,
            cases: _,
        }
        | WorkflowType::Union(_) => false,
    }
}

fn required_agent_model_usage(agent_declaration: &AgentDeclaration) -> Result<&ModelUsage, WorkflowSemanticError> {
    agent_declaration
        .model_usage()
        .ok_or_else(|| WorkflowSemanticError::InvalidAgentProperty {
            agent_name: agent_declaration.name.clone(),
            property: structure::Agent::new().model.definition().name.to_string(),
            message: "property is required".to_string(),
        })
}

fn required_agent_property_expression(
    agent_declaration: &AgentDeclaration,
    property_name: AgentExpressionPropertyName,
) -> Result<&Expression, WorkflowSemanticError> {
    optional_agent_property_expression(agent_declaration, property_name).ok_or_else(|| WorkflowSemanticError::InvalidAgentProperty {
        agent_name: agent_declaration.name.clone(),
        property: property_name.as_str().to_string(),
        message: "property is required".to_string(),
    })
}

fn optional_agent_property_expression(
    agent_declaration: &AgentDeclaration,
    property_name: AgentExpressionPropertyName,
) -> Option<&Expression> {
    agent_declaration.expression_property(property_name)
}

#[cfg(test)]
mod tests {
    use super::build_typed_workflow_ir;
    use crate::parse_inline_workflow;
    use crate::semantic::support::types::WorkflowType;
    use crate::semantic::WorkflowSemanticError;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    use serde_json::json;
    use std::collections::BTreeMap;

    #[derive(Debug, Serialize, JsonSchema)]
    struct TestInput {
        topic: String,
    }

    #[derive(Debug, Deserialize, JsonSchema)]
    #[allow(dead_code)]
    struct ScoreOutput {
        value: i64,
    }

    #[derive(Debug, Deserialize, JsonSchema)]
    #[allow(dead_code)]
    struct TestOutput {
        values: Vec<ScoreOutput>,
    }

    #[derive(Debug, Deserialize, JsonSchema)]
    #[allow(dead_code)]
    struct GreetingOutput {
        greeting: String,
    }

    #[test]
    fn builds_typed_ir_with_dependencies_and_loop_output_types() {
        let workflow = parse_inline_workflow! {
            provider openai from openai {}

            model openai_model from openai {
                id: "model-a"
            }

            input {
                topic: string
            }

            agent researcher {
                model: model.openai_model
                instruction: input.topic
                output {
                    value: string
                }
            }

            agent scorer for item in [agent.researcher] {
                model: model.openai_model
                instruction: "score {{ item }}"
                output {
                    value: number
                }
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
                item_type: Box::new(WorkflowType::Object(BTreeMap::from([
                    ("value".to_string(), WorkflowType::Integer,)
                ]))),
                fixed_length: None,
            }
            .normalize()
        );

        let mut expected_output_fields = BTreeMap::new();
        expected_output_fields.insert(
            "values".to_string(),
            WorkflowType::Array {
                item_type: Box::new(WorkflowType::Object(BTreeMap::from([("value".to_string(), WorkflowType::Integer)]))),
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
    fn infers_untyped_mcp_tool_outputs_as_any() {
        let workflow = parse_inline_workflow! {
            mcp mintilify {
                endpoint: "https://example.test/mcp"
            }

            from mcp.mintilify {
                tool query_docs_filesystem_superwire {
                    bindings {
                        command: "ls"
                    }
                }
            }

            output {
                greeting: call tool.query_docs_filesystem_superwire {}
            }
        };

        let typed_workflow_ir = build_typed_workflow_ir::<(), GreetingOutput>(&workflow).expect("typecheck should succeed");

        assert_eq!(
            typed_workflow_ir.workflow_output_type,
            WorkflowType::Object(BTreeMap::from([("greeting".to_string(), WorkflowType::Any)]))
        );
    }

    #[test]
    fn model_input_schema_removes_bound_tool_fields() {
        #[derive(Debug, Deserialize, JsonSchema)]
        #[allow(dead_code)]
        struct Output {
            value: String,
        }

        let workflow = parse_inline_workflow! {
            tool lookup {
                input {
                    workspace_id: string
                    query: string
                }

                bindings {
                    workspace_id: "workspace-1"
                }

                output {
                    value: string
                }
            }

            output {
                value: "ok"
            }
        };

        let typed_workflow_ir = build_typed_workflow_ir::<(), Output>(&workflow).expect("typecheck should succeed");
        let typed_tool = typed_workflow_ir
            .tools
            .iter()
            .find(|tool| tool.name == "lookup")
            .expect("lookup tool should be present");
        let schema = typed_tool.model_input_schema(&json!({ "workspace_id": "workspace-1" }));

        assert!(schema["properties"].get("workspace_id").is_none());
        assert_eq!(schema["properties"]["query"]["type"], "string");
        assert_eq!(schema["required"], json!(["query"]));
    }

    #[test]
    fn includes_provider_expression_dependencies_in_agent_execution_dependencies() {
        #[derive(Debug, Deserialize, JsonSchema)]
        #[allow(dead_code)]
        struct Output {
            value: String,
        }

        let workflow = parse_inline_workflow! {
            provider base_provider from openai {
                endpoint: "http://localhost:1234/v1"
                api_key: "test-api-key"
            }

            model base_provider_model from base_provider {
                id: "model-a"
            }

            provider dynamic_provider from openai {
                endpoint: agent.base.endpoint
                api_key: "test-api-key"
            }

            model dynamic_provider_model from dynamic_provider {
                id: "model-a"
            }

            agent base {
                model: model.base_provider_model
                instruction: "base"
                output {
                    endpoint: string
                }
            }

            agent dependent {
                model: model.dynamic_provider_model
                instruction: "dependent"
                output {
                    value: string
                }
            }

            output {
                value: agent.dependent.value
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
    fn merges_model_and_model_usage_inference_by_precedence() {
        #[derive(Debug, Deserialize, JsonSchema)]
        #[allow(dead_code)]
        struct Output {
            value: String,
        }

        let workflow = parse_inline_workflow! {
            provider openai from openai {}

            model fast from openai {
                id: "model-a"

                inference {
                    temperature: 0.2
                    max_tokens: 4_000
                }
            }

            agent writer {
                model: model.fast {
                    inference {
                        temperature: 0.8
                    }
                }

                instruction: "write"
                output {
                    value: string
                }
            }

            output {
                value: agent.writer.value
            }
        };

        let typed_workflow_ir = build_typed_workflow_ir::<(), Output>(&workflow).expect("typecheck should succeed");
        let writer_agent = typed_workflow_ir
            .agents
            .iter()
            .find(|typed_agent| typed_agent.name == "writer")
            .expect("writer agent should be present");
        let inference_field_names = writer_agent
            .inference_fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(inference_field_names, vec!["temperature", "max_tokens"]);
        assert!(matches!(writer_agent.inference_fields[0].value, crate::dsl::Expression::NumberLiteral(ref value) if value == "0.8"));
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
                instruction: "hello"
                output {
                    value: string
                }
            }

            output {
                value: agent.missing_model.value
            }
        };

        let typecheck_result = build_typed_workflow_ir::<(), Output>(&workflow);

        assert!(matches!(
            typecheck_result,
            Err(WorkflowSemanticError::InvalidAgentProperty { property, .. }) if property == "model"
        ));
    }

    #[test]
    fn fails_typecheck_when_agent_instruction_property_is_missing() {
        #[derive(Debug, Deserialize, JsonSchema)]
        #[allow(dead_code)]
        struct Output {
            value: String,
        }

        let workflow = parse_inline_workflow! {
            provider openai from openai {}

            model openai_model from openai {
                id: "model-a"
            }

            agent missing_instruction {
                model: model.openai_model
                output {
                    value: string
                }
            }

            output {
                value: agent.missing_instruction.value
            }
        };

        let typecheck_result = build_typed_workflow_ir::<(), Output>(&workflow);

        assert!(matches!(
            typecheck_result,
            Err(WorkflowSemanticError::InvalidAgentProperty { property, .. }) if property == "instruction"
        ));
    }
}
