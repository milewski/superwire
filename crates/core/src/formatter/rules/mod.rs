//! Rule-based formatting system for .ai files
//!
//! This module provides a trait-based approach to formatting where each
//! formatting concern is handled by a separate rule implementation.

use crate::ast::Workflow;

/// Trait for formatting rules that can be applied to workflows
pub trait FormattingRule {
    /// Apply this rule to a workflow, modifying it in place
    fn apply(&self, workflow: &mut Workflow) -> Result<(), FormattingError>;

    /// Get the name of this rule for debugging/logging
    fn name(&self) -> &'static str;

    /// Get the priority of this rule (lower numbers run first)
    fn priority(&self) -> u32 {
        100 // Default priority
    }
}

/// Error type for formatting rules
#[derive(Debug, Clone)]
pub struct FormattingError {
    pub rule_name: String,
    pub message: String,
}

impl FormattingError {
    pub fn new(rule_name: &str, message: impl Into<String>) -> Self {
        Self {
            rule_name: rule_name.to_string(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for FormattingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Formatting error in rule '{}': {}", self.rule_name, self.message)
    }
}

impl std::error::Error for FormattingError {}

/// Rule engine that applies multiple formatting rules in priority order
pub struct RuleEngine {
    rules: Vec<Box<dyn FormattingRule>>,
}

impl RuleEngine {
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
        }
    }

    /// Add a rule to the engine
    pub fn add_rule<R: FormattingRule + 'static>(mut self, rule: R) -> Self {
        self.rules.push(Box::new(rule));
        // Sort by priority after adding
        self.rules.sort_by_key(|r| r.priority());
        self
    }

    /// Apply all rules to a workflow
    pub fn apply(&self, workflow: &mut Workflow) -> Result<(), FormattingError> {
        for rule in &self.rules {
            rule.apply(workflow)?;
        }
        Ok(())
    }
}

impl Default for RuleEngine {
    fn default() -> Self {
        Self::new()
    }
}

// Re-export all rule modules
pub mod spacing;
pub mod indentation;
pub mod line_breaks;
pub mod array_formatting;
pub mod string_formatting;

pub use spacing::SpacingRule;
pub use indentation::IndentationRule;
pub use line_breaks::LineBreaksRule;
pub use array_formatting::ArrayFormattingRule;
pub use string_formatting::StringFormattingRule;