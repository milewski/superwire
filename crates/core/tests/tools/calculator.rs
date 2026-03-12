use engine_ai_core::impl_tool;
use engine_ai_core::tool_error;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    Add,
    Subtract,
    Multiply,
    Divide,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CalculatorParams {
    pub operation: Operation,
    pub a: f64,
    pub b: f64,
}

#[derive(Default)]
pub struct CalculatorTool;

impl_tool!(CalculatorTool, CalculatorParams, {
    name: "calculator",
    description: "Perform basic arithmetic operations (add, subtract, multiply, divide)",
    execute: |params| {
        if params.operation == Operation::Divide && params.b == 0.0 {
            Err(tool_error!("Division by zero", "Ensure the divisor is not zero"))
        } else {
            let result = match params.operation {
                Operation::Add => params.a + params.b,
                Operation::Subtract => params.a - params.b,
                Operation::Multiply => params.a * params.b,
                Operation::Divide => params.a / params.b,
            };

            Ok(serde_json::json!({
                "operation": params.operation,
                "a": params.a,
                "b": params.b,
                "result": result
            }))
        }
    }
});
