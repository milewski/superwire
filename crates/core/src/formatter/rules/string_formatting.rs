use super::{FormattingError, FormattingRule};
use crate::ast::{AgentProperty, Value, Workflow, Agent, Span};

/// Rule that handles string formatting, especially multiline strings
pub struct StringFormattingRule;

impl StringFormattingRule {
    pub fn new() -> Self {
        Self
    }

    /// Apply string formatting to a value recursively
    fn apply_to_value(&self, value: &mut Value) -> Result<(), FormattingError> {
        match value {
            Value::String(_) => {
                // String formatting is handled during serialization
                // This rule ensures consistent string handling standards
            }
            Value::Array(items) => {
                // Recursively apply to nested values
                for item in items.iter_mut() {
                    self.apply_to_value(item)?;
                }
            }
            Value::Object(obj) => {
                // Recursively apply to object values
                for (_, val) in obj.iter_mut() {
                    self.apply_to_value(val)?;
                }
            }
            Value::FunctionCall(func) => {
                // Apply to function call arguments
                for (_, arg) in func.arguments.iter_mut() {
                    self.apply_to_value(arg)?;
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
                        self.apply_to_value(value)?;
                    }
                    AgentProperty::Tools { value, .. } => {
                        self.apply_to_value(value)?;
                    }
                    AgentProperty::Context { value, .. } => {
                        self.apply_to_value(value)?;
                    }
                    AgentProperty::Prompt { value, .. } => {
                        self.apply_to_value(value)?;
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
                self.apply_to_value(&mut field.value)?;
            }
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "StringFormattingRule"
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
        assert_eq!(rule.name(), "StringFormattingRule");
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
                        value: Value::String("Hello\nWorld".to_string()),
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
                            Value::String("item1".to_string()),
                            Value::String("item2".to_string()),
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
    }
}