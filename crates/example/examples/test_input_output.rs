use engine_ai_core::parser::AstBuilder;

fn main() {
    let workflow_content = std::fs::read_to_string("crates/example/workflows/input_output.ai").unwrap();

    let builder = AstBuilder::new("input_output.ai".to_string());

    match builder.parse(&workflow_content) {
        Ok(workflow) => {
            println!("Parsed successfully!");

            if let Some(input) = &workflow.input {
                println!("\nInput block:");
                for field in &input.fields {
                    println!("  {}: {:?}", field.name, field.field_type);
                }
            }

            if let Some(output) = &workflow.output {
                println!("\nOutput block:");
                for field in &output.fields {
                    println!("  {}: {:?}", field.name, field.value);
                }
            }

            println!("\nAgents: {}", workflow.agents.len());
        }
        Err(error) => {
            eprintln!("Parse error: {}", error);
        }
    }
}
