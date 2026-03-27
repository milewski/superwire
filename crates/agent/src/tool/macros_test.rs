#[cfg(test)]
mod tests {
    use crate::tool;
    use crate::tool::Tool;
    use serde_json::json;

    tool! {
        /// Search for information
        SearchTool {
            query: String,
            max_results: Option<usize>,
        } => async |input| {
            Ok(json!({
                "query": input.query,
                "max_results": input.max_results.unwrap_or(10),
                "results": ["result1", "result2"]
            }))
        }
    }

    tool! {
        /// Calculate mathematical expressions
        CalculatorTool {
            expression: String,
        } => async |input| {
            Ok(json!({
                "expression": input.expression,
                "result": 42
            }))
        }
    }

    #[tokio::test]
    async fn test_tool_macro_basic() {
        let search_tool = SearchTool;

        assert_eq!(search_tool.name(), "SearchTool");
        assert!(!search_tool.description().is_empty());

        let input = SearchToolInput {
            query: "test query".to_string(),
            max_results: Some(5),
        };

        let result = search_tool.execute(input).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value["query"], "test query");
        assert_eq!(value["max_results"], 5);
    }

    #[tokio::test]
    async fn test_tool_macro_optional_fields() {
        let search_tool = SearchTool;

        let input = SearchToolInput {
            query: "another query".to_string(),
            max_results: None,
        };

        let result = search_tool.execute(input).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value["query"], "another query");
        assert_eq!(value["max_results"], 10);
    }

    #[tokio::test]
    async fn test_tool_macro_multiple_tools() {
        let calculator = CalculatorTool;

        assert_eq!(calculator.name(), "CalculatorTool");

        let input = CalculatorToolInput {
            expression: "2 + 2".to_string(),
        };

        let result = calculator.execute(input).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value["expression"], "2 + 2");
        assert_eq!(value["result"], 42);
    }

    #[test]
    fn test_tool_macro_registers_tools_in_inventory() {
        let has_registered_search_tool = crate::tool::registered_runtime_tools()
            .into_iter()
            .any(|runtime_tool| runtime_tool.definition().expect("tool definition should be available").name == "SearchTool");

        assert!(has_registered_search_tool);
    }
}
