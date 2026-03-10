use engine_ai_core::impl_tool;
use engine_ai_core::tool_error;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
        let result = match params.operation {
            Operation::Add => params.a + params.b,
            Operation::Subtract => params.a - params.b,
            Operation::Multiply => params.a * params.b,
            Operation::Divide => {
                if params.b == 0.0 {
                    return Err(tool_error!("Division by zero", "Ensure the divisor is not zero"));
                }
                params.a / params.b
            }
        };

        Ok(serde_json::json!({
            "operation": params.operation,
            "a": params.a,
            "b": params.b,
            "result": result
        }))
    }
});
