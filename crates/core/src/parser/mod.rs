pub mod ast_constructor;
pub mod builder;
pub mod error;
pub mod error_analyzer;
pub mod graph;
pub mod macros;

use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "parser/grammar.pest"]
pub struct WorkflowParser;

pub use ast_constructor::AstConstructor;
pub use builder::AstBuilder;
pub use error::ParserError;
pub use error_analyzer::ErrorAnalyzer;
pub use graph::DependencyGraph;
