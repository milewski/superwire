use super::{ExecutorError, ToolCallExecutionContext, WorkflowExecutor};
use crate::event::ExecutorEvent;
use crate::model::{ModelToolDefinition, ToolCallTracker};
use crate::runtime::state::RuntimeState;
use serde_json::{Map, Value};
use std::collections::HashMap;
use superwire_core::semantic::support::types::validate_value_against_type;
use superwire_core::semantic::PlannedAgent;
use tokio::sync::mpsc;

pub(super) trait PlannedAgentSchemaExt {
    fn push_finalize_tool_definition(&self, tool_definitions: &mut Vec<ModelToolDefinition>) -> Value;

    fn validate_output_value(&self, output: &Value) -> Result<(), ExecutorError>;
}

impl PlannedAgentSchemaExt for PlannedAgent {
    fn push_finalize_tool_definition(&self, tool_definitions: &mut Vec<ModelToolDefinition>) -> Value {
        let output_schema = self.iteration_output_schema();
        tool_definitions.push(ModelToolDefinition::finalize(output_schema.clone()));

        output_schema
    }

    fn validate_output_value(&self, output: &Value) -> Result<(), ExecutorError> {
        self.validate_iteration_output_value(output)
            .map_err(|message| ExecutorError::AgentOutputTypeMismatch {
                agent_name: self.name.clone(),
                message,
            })
    }
}

impl WorkflowExecutor {
    pub(super) fn evaluate_workflow_output(
        &self,
        runtime_state: &RuntimeState,
        event_sender: Option<&mpsc::Sender<ExecutorEvent>>,
        tool_call_tracker: &ToolCallTracker,
    ) -> Result<Value, ExecutorError> {
        let mut output_fields = Map::new();
        let evaluation_context = runtime_state.evaluation_context(HashMap::new());
        let tool_call_execution_context = ToolCallExecutionContext::new(&evaluation_context, event_sender, tool_call_tracker);

        for output_field in &self.execution_plan.output_declaration.fields {
            let output_value = self.evaluate_runtime_expression(&output_field.value, tool_call_execution_context, "workflow output")?;
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
