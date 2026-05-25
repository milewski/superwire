use super::{AgentCacheOptions, ExecutorError, ToolCallExecutionContext, WorkflowExecutor};
use crate::model::{ModelSchema, ModelToolDefinition, ToolCallTracker};
use crate::runtime::state::RuntimeState;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use superwire_dsl::{Expression, TypeExpression};
use superwire_protocol::event::ExecutorEvent;
use superwire_semantic::support::expression::{evaluate_expression, EvaluationContext};
use superwire_semantic::support::types::validate_value_against_type;
use superwire_semantic::support::types::WorkflowType;
use superwire_semantic::PlannedAgent;
use tokio::sync::mpsc;

pub(super) trait PlannedAgentSchemaExt {
    fn push_finalize_tool_definition(&self, tool_definitions: &mut Vec<ModelToolDefinition>) -> ModelSchema;

    fn output_injections(&self, evaluation_context: &EvaluationContext) -> Result<AgentOutputInjections, ExecutorError>;

    fn inject_output_values(&self, output: Value, output_injections: &AgentOutputInjections) -> Result<Value, ExecutorError>;

    fn validate_output_value(&self, output: &Value) -> Result<(), ExecutorError>;
}

impl PlannedAgentSchemaExt for PlannedAgent {
    fn push_finalize_tool_definition(&self, tool_definitions: &mut Vec<ModelToolDefinition>) -> ModelSchema {
        let output_schema = self.model_output_schema();
        tool_definitions.push(ModelToolDefinition::finalize(output_schema.clone()));

        output_schema
    }

    fn output_injections(&self, evaluation_context: &EvaluationContext) -> Result<AgentOutputInjections, ExecutorError> {
        let mut output_injections = AgentOutputInjections::default();
        let Some(output_type_expression) = self.declaration.output_type() else {
            return Ok(output_injections);
        };

        output_type_expression.collect_output_injections(
            Vec::new(),
            evaluation_context,
            &format!("output injection for agent `{}`", self.name),
            &mut output_injections,
        )?;

        Ok(output_injections)
    }

    fn inject_output_values(&self, output: Value, output_injections: &AgentOutputInjections) -> Result<Value, ExecutorError> {
        let mut injected_output = output;

        for output_injection in &output_injections.values {
            output_injection
                .inject_into(&mut injected_output)
                .map_err(|message| ExecutorError::AgentOutputTypeMismatch {
                    agent_name: self.name.clone(),
                    message,
                })?;
        }

        Ok(injected_output)
    }

    fn validate_output_value(&self, output: &Value) -> Result<(), ExecutorError> {
        self.validate_iteration_output_value(output)
            .map_err(|message| ExecutorError::AgentOutputTypeMismatch {
                agent_name: self.name.clone(),
                message,
            })
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct AgentOutputInjections {
    values: Vec<AgentOutputInjection>,
}

impl AgentOutputInjections {
    #[must_use]
    pub(super) fn fingerprint_value(&self) -> Value {
        Value::Array(self.values.iter().map(AgentOutputInjection::fingerprint_value).collect())
    }
}

#[derive(Debug, Clone)]
struct AgentOutputInjection {
    path: Vec<String>,
    value: Value,
}

impl AgentOutputInjection {
    fn inject_into(&self, output: &mut Value) -> Result<(), String> {
        let Some((field_name, remaining_path)) = self.path.split_first() else {
            *output = self.value.clone();

            return Ok(());
        };

        let output_object = Self::object_mut(output)?;
        let field_value = output_object.entry(field_name.clone()).or_insert_with(|| Value::Object(Map::new()));

        Self::inject_remaining_path(field_value, remaining_path, &self.value)
    }

    fn inject_remaining_path(output: &mut Value, path: &[String], value: &Value) -> Result<(), String> {
        let Some((field_name, remaining_path)) = path.split_first() else {
            *output = value.clone();

            return Ok(());
        };

        let output_object = Self::object_mut(output)?;
        let field_value = output_object.entry(field_name.clone()).or_insert_with(|| Value::Object(Map::new()));

        Self::inject_remaining_path(field_value, remaining_path, value)
    }

    fn object_mut(value: &mut Value) -> Result<&mut Map<String, Value>, String> {
        if value.is_null() {
            *value = Value::Object(Map::new());
        }

        value
            .as_object_mut()
            .ok_or_else(|| "agent output injection expected an object at the injection path".to_string())
    }

    fn fingerprint_value(&self) -> Value {
        serde_json::json!({
            "path": self.path,
            "value": self.value,
        })
    }
}

trait PlannedAgentOutputSchemaExt {
    fn model_output_schema(&self) -> ModelSchema;
}

impl PlannedAgentOutputSchemaExt for PlannedAgent {
    fn model_output_schema(&self) -> ModelSchema {
        let model_output_type = self
            .declaration
            .output_type()
            .and_then(|type_expression| type_expression.model_output_type(&self.iteration_output_type))
            .unwrap_or_else(|| match &self.iteration_output_type {
                WorkflowType::Object(_) => WorkflowType::Object(BTreeMap::new()),
                _ => self.iteration_output_type.clone(),
            });

        ModelSchema::workflow(model_output_type)
    }
}

trait TypeExpressionOutputInjectionExt {
    fn model_output_type(&self, output_type: &WorkflowType) -> Option<WorkflowType>;

    fn collect_output_injections(
        &self,
        path: Vec<String>,
        evaluation_context: &EvaluationContext,
        context: &str,
        output_injections: &mut AgentOutputInjections,
    ) -> Result<(), ExecutorError>;

    fn injection_expression(&self) -> Option<Expression>;
}

impl TypeExpressionOutputInjectionExt for TypeExpression {
    fn model_output_type(&self, output_type: &WorkflowType) -> Option<WorkflowType> {
        if self.injection_expression().is_some() {
            return None;
        }

        match (self, output_type) {
            (Self::Object(fields), WorkflowType::Object(output_fields)) => {
                let mut model_fields = BTreeMap::new();

                for field in fields {
                    let Some(field_output_type) = output_fields.get(&field.name) else {
                        continue;
                    };

                    if let Some(model_field_type) = field.field_type.model_output_type(field_output_type) {
                        model_fields.insert(field.name.clone(), model_field_type);
                    }
                }

                (!model_fields.is_empty()).then_some(WorkflowType::Object(model_fields))
            }
            (
                Self::Array { item_type, fixed_length },
                WorkflowType::Array {
                    item_type: output_item_type,
                    fixed_length: _,
                },
            ) => item_type
                .model_output_type(output_item_type)
                .map(|model_item_type| WorkflowType::Array {
                    item_type: Box::new(model_item_type),
                    fixed_length: *fixed_length,
                }),
            (Self::Tuple(item_types), WorkflowType::Tuple(output_item_types)) => {
                let mut model_item_types = Vec::new();

                for (item_type, output_item_type) in item_types.iter().zip(output_item_types) {
                    if let Some(model_item_type) = item_type.model_output_type(output_item_type) {
                        model_item_types.push(model_item_type);
                    }
                }

                Some(WorkflowType::Tuple(model_item_types))
            }
            (Self::Union(_), _) => Some(output_type.clone()),
            (
                Self::String
                | Self::Number
                | Self::Float
                | Self::Boolean
                | Self::Null
                | Self::AnyObject
                | Self::SchemaReference(_)
                | Self::StringEnum(_)
                | Self::StringEnumReference(_)
                | Self::Variant {
                    discriminator: _,
                    cases: _,
                },
                _,
            ) => Some(output_type.clone()),
            (Self::Array { .. } | Self::Tuple(_) | Self::Object(_), _) => Some(output_type.clone()),
        }
    }

    fn collect_output_injections(
        &self,
        path: Vec<String>,
        evaluation_context: &EvaluationContext,
        context: &str,
        output_injections: &mut AgentOutputInjections,
    ) -> Result<(), ExecutorError> {
        if let Some(expression) = self.injection_expression() {
            let value = evaluate_expression(&expression, evaluation_context, context).map_err(ExecutorError::Semantic)?;
            output_injections.values.push(AgentOutputInjection { path, value });

            return Ok(());
        }

        match self {
            Self::Object(fields) => {
                for field in fields {
                    let mut field_path = path.clone();
                    field_path.push(field.name.clone());
                    field
                        .field_type
                        .collect_output_injections(field_path, evaluation_context, context, output_injections)?;
                }
            }
            Self::Array {
                item_type,
                fixed_length: None,
            } => {
                if let Some(expression) = item_type.injection_expression() {
                    let item_value = evaluate_expression(&expression, evaluation_context, context).map_err(ExecutorError::Semantic)?;
                    output_injections.values.push(AgentOutputInjection {
                        path,
                        value: Value::Array(vec![item_value]),
                    });
                }
            }
            Self::String
            | Self::Number
            | Self::Float
            | Self::Boolean
            | Self::Null
            | Self::AnyObject
            | Self::SchemaReference(_)
            | Self::StringEnum(_)
            | Self::StringEnumReference(_)
            | Self::Array {
                item_type: _,
                fixed_length: Some(_),
            }
            | Self::Tuple(_)
            | Self::Variant {
                discriminator: _,
                cases: _,
            }
            | Self::Union(_) => {}
        }

        Ok(())
    }

    fn injection_expression(&self) -> Option<Expression> {
        match self {
            Self::StringEnum(string_value) => Some(Expression::StringLiteral(string_value.clone())),
            Self::StringEnumReference(reference) if reference.schema_name_and_field_path().is_none() => {
                Some(Expression::Reference(reference.clone()))
            }
            Self::String
            | Self::Number
            | Self::Float
            | Self::Boolean
            | Self::Null
            | Self::AnyObject
            | Self::SchemaReference(_)
            | Self::StringEnumReference(_)
            | Self::Array {
                item_type: _,
                fixed_length: _,
            }
            | Self::Tuple(_)
            | Self::Object(_)
            | Self::Variant {
                discriminator: _,
                cases: _,
            }
            | Self::Union(_) => None,
        }
    }
}

impl WorkflowExecutor {
    pub(super) async fn evaluate_workflow_output<ModelProviderType>(
        &self,
        runtime_state: &RuntimeState,
        event_sender: Option<&mpsc::Sender<ExecutorEvent>>,
        tool_call_tracker: &ToolCallTracker,
        model_provider: &ModelProviderType,
        cache_options: Option<&AgentCacheOptions>,
    ) -> Result<Value, ExecutorError>
    where
        ModelProviderType: crate::model::ModelProvider,
    {
        let mut output_fields = Map::new();
        let evaluation_context = runtime_state.evaluation_context();
        let tool_call_execution_context = ToolCallExecutionContext::new(&evaluation_context, event_sender, tool_call_tracker);

        for output_field in &self.execution_plan.output_declaration.fields {
            let output_value = self
                .evaluate_runtime_expression_with_model(
                    &output_field.value,
                    tool_call_execution_context,
                    "workflow output",
                    model_provider,
                    cache_options,
                )
                .await?;
            output_fields.insert(output_field.name.clone(), output_value);
        }

        Ok(Value::Object(output_fields))
    }

    pub(super) fn validate_workflow_output_value(&self, output: &Value) -> Result<(), ExecutorError> {
        validate_value_against_type(output, &self.execution_plan.workflow_output_type).map_err(|message| {
            ExecutorError::OutputTypeMismatch {
                expected: self.execution_plan.workflow_output_type.to_string(),
                found: format!("invalid runtime output: {message}"),
            }
        })
    }
}

pub(in crate::runtime) fn value_object(value: &Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}
