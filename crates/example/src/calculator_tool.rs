use engine_ai_macros::tool;
use serde_json::Value;
use anyhow::Result;

/// Example custom tool using the #[tool] macro
/// The macro automatically implements the Tool trait with default methods
#[tool]
pub struct CalculatorTool;

// We can override the execute method by implementing it separately
// This works because Rust allows trait methods to be overridden
impl CalculatorTool {
    pub fn execute_custom(&self, args: Value) -> Result<Value> {
        let operation = args.get("operation")
            .and_then(|v| v.as_str())
            .unwrap_or("add");

        let a = args.get("a")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        let b = args.get("b")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        let result = match operation {
            "add" => a + b,
            "subtract" => a - b,
            "multiply" => a * b,
            "divide" => {
                if b != 0.0 {
                    a / b
                } else {
                    return Err(anyhow::anyhow!("Division by zero"));
                }
            }
            _ => return Err(anyhow::anyhow!("Unknown operation: {}", operation)),
        };

        Ok(serde_json::json!({
            "result": result
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_ai_core::Tool;

    #[test]
    fn test_calculator_tool_name() {
        let tool = CalculatorTool::new();
        assert_eq!(tool.name(), "CalculatorTool");
        assert!(tool.description().contains("CalculatorTool"));
    }

    #[test]
    fn test_calculator_add() {
        let tool = CalculatorTool::new();

        let args = serde_json::json!({
            "operation": "add",
            "a": 5.0,
            "b": 3.0
        });

        let result = tool.execute_custom(args).unwrap();
        assert_eq!(result["result"], 8.0);
    }

    #[test]
    fn test_calculator_multiply() {
        let tool = CalculatorTool::new();

        let args = serde_json::json!({
            "operation": "multiply",
            "a": 4.0,
            "b": 7.0
        });

        let result = tool.execute_custom(args).unwrap();
        assert_eq!(result["result"], 28.0);
    }

    #[test]
    fn test_calculator_divide_by_zero() {
        let tool = CalculatorTool::new();

        let args = serde_json::json!({
            "operation": "divide",
            "a": 10.0,
            "b": 0.0
        });

        let result = tool.execute_custom(args);
        assert!(result.is_err());
    }
}
