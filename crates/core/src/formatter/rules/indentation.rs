//! Indentation rule for consistent code formatting

use super::{FormattingError, FormattingRule};
use crate::ast::Workflow;

/// Rule that ensures consistent 4-space indentation throughout the workflow
pub struct IndentationRule;

impl IndentationRule {
    pub fn new() -> Self {
        Self
    }

    /// Generate indentation string for a given level (always 4 spaces)
    pub fn get_indent(&self, level: usize) -> String {
        "    ".repeat(level)
    }
}

impl FormattingRule for IndentationRule {
    fn apply(&self, _workflow: &mut Workflow) -> Result<(), FormattingError> {
        // Indentation is handled during serialization
        // This rule ensures consistent indentation standards
        Ok(())
    }

    fn priority(&self) -> u32 {
        20 // Medium priority - after spacing but before line breaks
    }
}

impl Default for IndentationRule {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Workflow;

    #[test]
    fn test_indentation_rule_creation() {
        let rule = IndentationRule::new();
        assert_eq!(rule.priority(), 20);
    }

    #[test]
    fn test_get_indent() {
        let rule = IndentationRule::new();
        assert_eq!(rule.get_indent(0), "");
        assert_eq!(rule.get_indent(1), "    ");
        assert_eq!(rule.get_indent(2), "        ");
        assert_eq!(rule.get_indent(3), "            ");
    }

    #[test]
    fn test_apply_to_workflow() {
        let mut workflow = Workflow {
            providers: vec![],
            schemas: vec![],
            agents: vec![],
            input: None,
            output: None,
            span: crate::ast::Span::new(0, 0, 0, 0),
        };

        let rule = IndentationRule::new();
        let result = rule.apply(&mut workflow);
        assert!(result.is_ok());
    }
}
