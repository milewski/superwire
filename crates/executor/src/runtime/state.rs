use serde_json::{Map, Value};
use std::collections::HashMap;
use superwire_core::semantic::support::expression::EvaluationContext;

#[derive(Debug, Clone)]
pub struct RuntimeState {
    input_values: Map<String, Value>,
    secret_values: Map<String, Value>,
    agent_outputs: HashMap<String, Value>,
    agent_contexts: HashMap<String, Value>,
    local_bindings: HashMap<String, Value>,
}

impl RuntimeState {
    #[must_use] 
    pub fn new(input_values: Map<String, Value>, secret_values: Map<String, Value>) -> Self {
        Self {
            input_values,
            secret_values,
            agent_outputs: HashMap::new(),
            agent_contexts: HashMap::new(),
            local_bindings: HashMap::new(),
        }
    }

    #[must_use] 
    pub fn evaluation_context(&self, local_bindings: HashMap<String, Value>) -> EvaluationContext {
        let mut merged_local_bindings = self.local_bindings.clone();

        for (binding_name, binding_value) in local_bindings {
            merged_local_bindings.insert(binding_name, binding_value);
        }

        EvaluationContext {
            input_values: self.input_values.clone(),
            secret_values: self.secret_values.clone(),
            agent_outputs: self.agent_outputs.clone(),
            agent_contexts: self.agent_contexts.clone(),
            local_bindings: merged_local_bindings,
        }
    }

    pub fn insert_local_binding(&mut self, binding_name: String, binding_value: Value) {
        self.local_bindings.insert(binding_name, binding_value);
    }

    pub fn insert_agent_result(&mut self, agent_name: String, output: Value, context: Value) {
        self.agent_outputs.insert(agent_name.clone(), output);
        self.agent_contexts.insert(agent_name, context);
    }
}
