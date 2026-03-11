use super::{FormattingError, FormattingRule};
use crate::ast::{AgentProperty, Value, Workflow};

#[cfg(test)]
use crate::ast::{Agent, Span};

/// Rule that handles array formatting decisions
/// Arrays break into multiple lines only when content is very long (>80 chars)
pub struct ArrayFormattingRule;

impl ArrayFormattingRule {
    pub fn new() -> Self {
        Self
    }

    /// Determine if an array should break into multiple lines
    pub fn should_break_array(&self, items: &[Value]) -> bool {
        // Calculate total content length
        let total_length: usize = items.iter().map(Self::estimate_value_length).sum();

        // Add commas and spaces
        let with_separators = total_length + (items.len().saturating_sub(1) * 2);

        // Break if content is too long (>80 chars)
        with_separators > 80
    }

    /// Estimate the serialized length of a value
    fn estimate_value_length(value: &Value) -> usize {
        match value {
            Value::String(s) => s.len() + 2,          // Add quotes
            Value::MultilineString(s) => s.len() + 6, // Add triple quotes
            Value::Number(_) => 10,                   // Rough estimate
            Value::Boolean(b) => {
                if *b {
                    4
                } else {
                    5
                }
            } // "true" or "false"
            Value::Null => 4,                         // "null"
            Value::Array(items) => {
                let inner: usize = items.iter().map(Self::estimate_value_length).sum();
                inner + 2 + (items.len().saturating_sub(1) * 2) // brackets + commas
            }
            Value::Object(obj) => {
                let inner: usize = obj
                    .iter()
                    .map(|(k, v)| k.len() + 2 + Self::estimate_value_length(v)) // key + ": " + value
                    .sum();
                inner + 2 + (obj.len().saturating_sub(1) * 2) // braces + commas
            }
            Value::Reference(_) => 20,             // Rough estimate for references
            Value::FunctionCall(_) => 30,          // Rough estimate for function calls
            Value::Interpolated(s) => s.len() + 2, // Add quotes
        }
    }

    /// Apply array formatting to a value recursively
    fn apply_to_value(value: &mut Value) -> Result<(), FormattingError> {
        match value {
            Value::Array(items) => {
                // Recursively apply to nested values
                for item in items.iter_mut() {
                    Self::apply_to_value(item)?;
                }
                // Array breaking logic is handled during serialization
            }
            Value::Object(obj) => {
                // Recursively apply to object values
                for (_, val) in obj.iter_mut() {
                    Self::apply_to_value(val)?;
                }
            }
            Value::FunctionCall(func) => {
                // Apply to function call arguments
                for arg in func.arguments.values_mut() {
                    Self::apply_to_value(arg)?;
                }
            }
            _ => {
                // Other value types don't contain nested arrays
            }
        }
        Ok(())
    }
}

impl FormattingRule for ArrayFormattingRule {
    fn apply(&self, workflow: &mut Workflow) -> Result<(), FormattingError> {
        // Apply array formatting to all agents
        for agent in &mut workflow.agents {
            for property in &mut agent.properties {
                match property {
                    AgentProperty::Model { value, .. } => {
                        Self::apply_to_value(value)?;
                    }
                    AgentProperty::Tools { value, .. } => {
                        Self::apply_to_value(value)?;
                    }
                    AgentProperty::Context { value, .. } => {
                        Self::apply_to_value(value)?;
                    }
                    AgentProperty::Prompt { value, .. } => {
                        Self::apply_to_value(value)?;
                    }
                    AgentProperty::ForEach { .. } => {
                        // ForEach doesn't have a value field
                    }
                    AgentProperty::Output { .. } => {
                        // Schema references don't contain arrays
                    }
                }
            }
        }

        // Apply to output block values
        if let Some(output) = &mut workflow.output {
            for field in &mut output.fields {
                Self::apply_to_value(&mut field.value)?;
            }
        }

        Ok(())
    }

    fn priority(&self) -> u32 {
        40 // Lower priority - after basic formatting
    }
}

impl Default for ArrayFormattingRule {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_array_formatting_rule_creation() {
        let rule = ArrayFormattingRule::new();
        assert_eq!(rule.priority(), 40);
    }

    #[test]
    fn test_should_break_array_short_content() {
        let rule = ArrayFormattingRule::new();

        // Small arrays should not break
        let small_array = vec![Value::String("a".to_string()), Value::String("b".to_string())];
        assert!(!rule.should_break_array(&small_array));
    }

    #[test]
    fn test_should_break_array_long_content() {
        let rule = ArrayFormattingRule::new();

        // Array with long content should break (make it longer to exceed 80 chars)
        let long_content_array = vec![
            Value::String(
                "this is a very long string that definitely exceeds the eighty character threshold for breaking arrays"
                    .to_string(),
            ),
            Value::String("another very long string that also exceeds the threshold".to_string()),
        ];
        assert!(rule.should_break_array(&long_content_array));
    }

    #[test]
    fn test_estimate_value_length() {
        assert_eq!(
            ArrayFormattingRule::estimate_value_length(&Value::String("hello".to_string())),
            7
        ); // 5 + 2 quotes
        assert_eq!(ArrayFormattingRule::estimate_value_length(&Value::Boolean(true)), 4);
        assert_eq!(ArrayFormattingRule::estimate_value_length(&Value::Null), 4);
    }

    #[test]
    fn test_apply_to_workflow() {
        let mut workflow = Workflow {
            providers: vec![],
            schemas: vec![],
            agents: vec![Agent {
                name: "test".to_string(),
                is_terminal: false,
                properties: vec![AgentProperty::Tools {
                    value: Value::Array(vec![
                        Value::String("bash".to_string()),
                        Value::String("grep".to_string()),
                    ]),
                    span: Span::new(0, 0, 0, 0),
                }],
                span: Span::new(0, 0, 0, 0),
            }],
            input: None,
            output: None,
            span: Span::new(0, 0, 0, 0),
        };

        let rule = ArrayFormattingRule::new();
        let result = rule.apply(&mut workflow);
        assert!(result.is_ok());
    }
}
