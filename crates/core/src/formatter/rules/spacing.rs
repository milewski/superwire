use super::{FormattingError, FormattingRule};
use crate::ast::{AgentProperty, Value, Workflow};

/// Rule that ensures proper spacing around assignment operators
pub struct SpacingRule;

impl SpacingRule {
    pub fn new() -> Self {
        Self
    }

    /// Apply spacing to a value (recursively for nested structures)
    fn apply_to_value(&self, _value: &mut Value) -> Result<(), FormattingError> {
        // Spacing is handled during serialization
        // This rule can be used for any AST-level spacing normalization
        Ok(())
    }
}

impl FormattingRule for SpacingRule {
    fn apply(&self, workflow: &mut Workflow) -> Result<(), FormattingError> {
        // Apply spacing rules to all agents
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
                    AgentProperty::Output { .. } => {
                        // Schema references don't need spacing adjustments
                    }
                    AgentProperty::Prompt { value, .. } => {
                        self.apply_to_value(value)?;
                    }
                    AgentProperty::ForEach { .. } => {
                        // ForEach doesn't have a value field
                    }
                }
            }
        }
        Ok(())
    }

    fn priority(&self) -> u32 {
        10 // High priority - spacing should be applied early
    }
}

impl Default for SpacingRule {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Agent, AgentProperty, Value, Workflow, Span};

    #[test]
    fn test_spacing_rule_creation() {
        let rule = SpacingRule::new();
        assert_eq!(rule.priority(), 10);
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
                    AgentProperty::Model {
                        value: Value::String("gpt-4".to_string()),
                        span: Span::new(0, 0, 0, 0),
                    },
                    AgentProperty::Prompt {
                        value: Value::String("Hello world".to_string()),
                        span: Span::new(0, 0, 0, 0),
                    },
                ],
                span: Span::new(0, 0, 0, 0),
            }],
            input: None,
            output: None,
            span: Span::new(0, 0, 0, 0),
        };

        let rule = SpacingRule::new();
        let result = rule.apply(&mut workflow);
        assert!(result.is_ok());
    }
}