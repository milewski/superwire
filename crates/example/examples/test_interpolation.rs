use engine_ai_core::ast::{Reference, Value};
use engine_ai_core::execution::RuntimeContext;

fn main() {
    let mut context = RuntimeContext::new();

    context.set_agent_output("topic".to_string(), serde_json::json!("Artificial Intelligence"));

    let interpolated = Value::Interpolated("Write a short article about: {{ topic }}".to_string());

    match context.resolve_value(&interpolated) {
        Ok(result) => {
            println!("Resolved: {}", result);
        }
        Err(error) => {
            eprintln!("Error: {}", error);
        }
    }
}
