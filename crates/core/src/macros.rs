/// Re-export the workflow! procedural macro for inline DSL parsing
///
/// This macro allows you to write workflow DSL inline in your Rust code.
/// It's defined as a procedural macro in the `engine-ai-macros` crate.
///
/// # Examples
///
/// ```ignore
/// use engine_ai_core::workflow;
///
/// let parsed = workflow! {
///     agent test {
///         model: "gpt-4"
///         prompt: "Hello"
///     }
/// };
/// ```
pub use engine_ai_macros::workflow;

/// Convenience macro for executing workflows with minimal boilerplate
///
/// # Examples
///
/// ```ignore
/// use engine_ai_core::try_workflow;
///
/// #[tokio::main]
/// async fn main() {
///     match try_workflow!("workflow.ai").await {
///         Ok(result) => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
///         Err(error) => eprintln!("Error: {}", error),
///     }
/// }
/// ```
#[macro_export]
macro_rules! try_workflow {
    ($path:expr) => {{
        async {
            let engine = $crate::execution::engine::ExecutionEngine::new();
            engine.execute_workflow_content(include_str!($path)).await
        }
    }};
}
