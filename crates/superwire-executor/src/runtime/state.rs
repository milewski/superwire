use crate::model::ToolCallTracker;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::Arc;
use superwire_core::semantic::support::expression::EvaluationContext;

#[derive(Debug, Clone)]
pub struct RuntimeState {
    input_values: Arc<Map<String, Value>>,
    secret_values: Arc<Map<String, Value>>,
    agent_outputs: Arc<HashMap<String, Value>>,
    agent_contexts: Arc<HashMap<String, Value>>,
    local_bindings: Arc<HashMap<String, Value>>,
    tool_call_tracker: ToolCallTracker,
}

impl RuntimeState {
    #[must_use]
    pub fn new(input_values: Map<String, Value>, secret_values: Map<String, Value>) -> Self {
        Self {
            input_values: Arc::new(input_values),
            secret_values: Arc::new(secret_values),
            agent_outputs: Arc::new(HashMap::new()),
            agent_contexts: Arc::new(HashMap::new()),
            local_bindings: Arc::new(HashMap::new()),
            tool_call_tracker: ToolCallTracker::default(),
        }
    }

    #[must_use]
    pub fn evaluation_context(&self) -> EvaluationContext {
        EvaluationContext {
            input_values: self.input_values.as_ref().clone(),
            secret_values: self.secret_values.as_ref().clone(),
            agent_outputs: self.agent_outputs.as_ref().clone(),
            agent_contexts: self.agent_contexts.as_ref().clone(),
            local_bindings: self.local_bindings.as_ref().clone(),
        }
    }

    #[must_use]
    pub fn evaluation_context_with_bindings(&self, local_bindings: &HashMap<String, Value>) -> EvaluationContext {
        let mut merged_local_bindings = self.local_bindings.as_ref().clone();

        for (binding_name, binding_value) in local_bindings {
            merged_local_bindings.insert(binding_name.clone(), binding_value.clone());
        }

        EvaluationContext {
            input_values: self.input_values.as_ref().clone(),
            secret_values: self.secret_values.as_ref().clone(),
            agent_outputs: self.agent_outputs.as_ref().clone(),
            agent_contexts: self.agent_contexts.as_ref().clone(),
            local_bindings: merged_local_bindings,
        }
    }

    pub fn insert_local_binding(&mut self, binding_name: String, binding_value: Value) {
        Arc::make_mut(&mut self.local_bindings).insert(binding_name, binding_value);
    }

    pub fn insert_agent_result(&mut self, agent_name: String, output: Value, context: Value) {
        Arc::make_mut(&mut self.agent_outputs).insert(agent_name.clone(), output);
        Arc::make_mut(&mut self.agent_contexts).insert(agent_name, context);
    }

    #[must_use]
    pub fn tool_call_tracker(&self) -> ToolCallTracker {
        self.tool_call_tracker.clone()
    }
}
