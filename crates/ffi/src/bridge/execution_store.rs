use std::collections::{HashMap, VecDeque};
use std::sync::RwLock;

use serde_json::Value;

use crate::types::{ExecutionValueName, WorkflowExecutionError};

const MAX_DEFERRED_EXECUTION_RESULTS: usize = 256;

#[derive(Debug, Clone)]
enum DeferredExecutionResult {
    Success { output: Value, context: Value },
    Failure { error: WorkflowExecutionError, context: Value },
}

#[derive(Default)]
pub struct ExecutionResultStore {
    results_by_execution_id: RwLock<HashMap<String, DeferredExecutionResult>>,
    registration_order: RwLock<VecDeque<String>>,
}

impl ExecutionResultStore {
    pub fn insert_success(&self, execution_id: String, output: Value, context: Value) {
        self.insert_result(execution_id, DeferredExecutionResult::Success { output, context });
    }

    pub fn insert_failure(&self, execution_id: String, error: WorkflowExecutionError, context: Value) {
        self.insert_result(execution_id, DeferredExecutionResult::Failure { error, context });
    }

    pub fn read_value(&self, execution_id: &str, value_name: ExecutionValueName) -> Result<Value, WorkflowExecutionError> {
        let results_by_execution_id = self
            .results_by_execution_id
            .read()
            .expect("execution result store lock should not be poisoned");

        let Some(deferred_result) = results_by_execution_id.get(execution_id) else {
            return Err(WorkflowExecutionError::tool_invocation_failed(
                format!("unknown deferred execution `{execution_id}`"),
                None,
            ));
        };

        match (deferred_result, value_name) {
            (DeferredExecutionResult::Success { output, .. }, ExecutionValueName::Success) => Ok(output.clone()),
            (DeferredExecutionResult::Failure { error, .. }, ExecutionValueName::Error) => serde_json::to_value(error).map_err(|error| {
                WorkflowExecutionError::internal(format!("failed to serialize deferred error for `{execution_id}`: {error}"))
            }),
            (
                DeferredExecutionResult::Success { context, .. } | DeferredExecutionResult::Failure { context, .. },
                ExecutionValueName::Context,
            ) => Ok(context.clone()),
            (DeferredExecutionResult::Success { .. }, ExecutionValueName::Error)
            | (DeferredExecutionResult::Failure { .. }, ExecutionValueName::Success) => Ok(Value::Null),
        }
    }

    fn insert_result(&self, execution_id: String, deferred_execution_result: DeferredExecutionResult) {
        let mut results_by_execution_id = self
            .results_by_execution_id
            .write()
            .expect("execution result store lock should not be poisoned");
        let mut registration_order = self
            .registration_order
            .write()
            .expect("execution result registration order lock should not be poisoned");

        if let Some(existing_execution_position) = registration_order
            .iter()
            .position(|registered_execution_id| registered_execution_id == &execution_id)
        {
            registration_order.remove(existing_execution_position);
        }

        registration_order.push_back(execution_id.clone());
        results_by_execution_id.insert(execution_id, deferred_execution_result);

        self.trim_to_capacity(&mut results_by_execution_id, &mut registration_order);
    }

    fn trim_to_capacity(
        &self,
        results_by_execution_id: &mut HashMap<String, DeferredExecutionResult>,
        registration_order: &mut VecDeque<String>,
    ) {
        while results_by_execution_id.len() > MAX_DEFERRED_EXECUTION_RESULTS {
            let Some(oldest_execution_id) = registration_order.pop_front() else {
                break;
            };

            results_by_execution_id.remove(&oldest_execution_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::ExecutionResultStore;
    use crate::types::{ExecutionValueName, WorkflowExecutionError};

    #[test]
    fn trims_oldest_results_when_capacity_is_exceeded() {
        let execution_result_store = ExecutionResultStore::default();

        for execution_index in 0..260 {
            execution_result_store.insert_success(
                format!("execution-{execution_index}"),
                json!({ "value": execution_index }),
                json!({}),
            );
        }

        let oldest_result = execution_result_store.read_value("execution-0", ExecutionValueName::Success);
        let newest_result = execution_result_store
            .read_value("execution-259", ExecutionValueName::Success)
            .expect("newest execution should still be present");

        assert!(matches!(
            oldest_result,
            Err(WorkflowExecutionError {
                code: _,
                message: _,
                context: _,
                details: _,
            })
        ));

        assert_eq!(newest_result, json!({ "value": 259 }));
    }
}
