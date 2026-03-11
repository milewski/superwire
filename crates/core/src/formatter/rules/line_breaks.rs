use super::{FormattingError, FormattingRule};
use crate::ast::Workflow;

#[cfg(test)]
use crate::ast::{Agent, Span};

/// Rule that ensures proper line breaks between different sections
pub struct LineBreaksRule;

impl LineBreaksRule {
    pub fn new() -> Self {
        Self
    }
}

impl FormattingRule for LineBreaksRule {
    fn apply(&self, _workflow: &mut Workflow) -> Result<(), FormattingError> {
        // Line breaks are handled during serialization
        // Standard formatting: newlines between agents, double newlines between sections
        Ok(())
    }

    fn priority(&self) -> u32 {
        30 // Lower priority - after spacing and indentation
    }
}

impl Default for LineBreaksRule {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_breaks_rule_creation() {
        let rule = LineBreaksRule::new();
        assert_eq!(rule.priority(), 30);
    }

    #[test]
    fn test_apply_to_workflow_single_agent() {
        let mut workflow = Workflow {
            providers: vec![],
            schemas: vec![],
            agents: vec![Agent {
                name: "test".to_string(),
                is_terminal: false,
                properties: vec![],
                span: Span::new(0, 0, 0, 0),
            }],
            input: None,
            output: None,
            span: Span::new(0, 0, 0, 0),
        };

        let rule = LineBreaksRule::new();
        let result = rule.apply(&mut workflow);
        assert!(result.is_ok());
    }

    #[test]
    fn test_apply_to_workflow_multiple_agents() {
        let mut workflow = Workflow {
            providers: vec![],
            schemas: vec![],
            agents: vec![
                Agent {
                    name: "first".to_string(),
                    is_terminal: false,
                    properties: vec![],
                    span: Span::new(0, 0, 0, 0),
                },
                Agent {
                    name: "second".to_string(),
                    is_terminal: false,
                    properties: vec![],
                    span: Span::new(0, 0, 0, 0),
                },
            ],
            input: None,
            output: None,
            span: Span::new(0, 0, 0, 0),
        };

        let rule = LineBreaksRule::new();
        let result = rule.apply(&mut workflow);
        assert!(result.is_ok());
    }
}