use super::{FormattingError, FormattingRule};
use crate::ast::{AgentProperty, Value, Workflow};

#[cfg(test)]
use crate::ast::{Agent, Span};

/// Rule that handles string formatting, especially multiline strings
pub struct StringFormattingRule;

impl StringFormattingRule {
    pub fn new() -> Self {
        Self
    }

    /// Apply string formatting to a value recursively
    fn apply_to_value(value: &mut Value) -> Result<(), FormattingError> {
        match value {
            Value::String(s) => {
                // Convert to multiline if string is longer than 80 characters or contains newlines
                if s.len() > 80 || s.contains('\n') {
                    *value = Value::MultilineString(s.clone());
                }
            }
            Value::Interpolated(s) => {
                // Convert interpolated strings to multiline if they're long
                if s.len() > 80 || s.contains('\n') {
                    *value = Value::MultilineString(s.clone());
                }
            }
            Value::MultilineString(_) => {
                // Already multiline, nothing to do
            }
            Value::Array(items) => {
                // Recursively apply to nested values
                for item in items.iter_mut() {
                    Self::apply_to_value(item)?;
                }
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
                // Other value types don't contain strings
            }
        }
        Ok(())
    }
}

impl FormattingRule for StringFormattingRule {
    fn apply(&self, workflow: &mut Workflow) -> Result<(), FormattingError> {
        // Apply string formatting to all agents
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
                        // Schema references don't contain strings to format
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
        50 // Lower priority - after array formatting
    }
}

impl Default for StringFormattingRule {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_formatting_rule_creation() {
        let rule = StringFormattingRule::new();
        assert_eq!(rule.priority(), 50);
    }

    #[test]
    fn test_apply_to_workflow() {
        let mut workflow = Workflow {
            providers: vec![],
            schemas: vec![],
            agents: vec![Agent {
                name: "test".to_string(),
                is_terminal: false,
                properties: vec![
                    AgentProperty::Prompt {
                        value: Value::String("This is a very long string that should automatically break into multiline format when it exceeds a certain length threshold".to_string()),
                        span: Span::new(0, 0, 0, 0),
                    },
                ],
                span: Span::new(0, 0, 0, 0),
            }],
            input: None,
            output: None,
            span: Span::new(0, 0, 0, 0),
        };

        let rule = StringFormattingRule::new();
        let result = rule.apply(&mut workflow);
        assert!(result.is_ok());

        // Check that the long string was converted to multiline
        if let AgentProperty::Prompt { value, .. } = &workflow.agents[0].properties[0] {
            match value {
                Value::MultilineString(_) => {
                    // Success - string was converted to multiline
                }
                _ => panic!("Expected MultilineString, got {value:?}"),
            }
        } else {
            panic!("Expected Prompt property");
        }
    }

    #[test]
    fn test_apply_to_nested_values() {
        let mut workflow = Workflow {
            providers: vec![],
            schemas: vec![],
            agents: vec![Agent {
                name: "test".to_string(),
                is_terminal: false,
                properties: vec![
                    AgentProperty::Context {
                        value: Value::Array(vec![
                            Value::String("short".to_string()),
                            Value::String("This is a very long string that should automatically break into multiline format when it exceeds a certain length threshold".to_string()),
                        ]),
                        span: Span::new(0, 0, 0, 0),
                    },
                ],
                span: Span::new(0, 0, 0, 0),
            }],
            input: None,
            output: None,
            span: Span::new(0, 0, 0, 0),
        };

        let rule = StringFormattingRule::new();
        let result = rule.apply(&mut workflow);
        assert!(result.is_ok());

        // Check that the long string in the array was converted to multiline
        if let AgentProperty::Context { value, .. } = &workflow.agents[0].properties[0] {
            if let Value::Array(items) = value {
                // First item should remain a regular string
                assert!(matches!(items[0], Value::String(_)));
                // Second item should be converted to multiline
                assert!(matches!(items[1], Value::MultilineString(_)));
            } else {
                panic!("Expected Array value");
            }
        } else {
            panic!("Expected Context property");
        }
    }
}
