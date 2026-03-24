use crate::ast::Workflow;
use crate::error::WorkflowError;
use crate::parser::grammar::Rule;
use pest::iterators::Pair;

pub(crate) trait GrammarVisitor {
    fn visit_workflow(&mut self, pair: Pair<'_, Rule>) -> Result<Workflow, WorkflowError>;
}
