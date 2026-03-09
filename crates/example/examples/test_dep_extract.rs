use engine_ai_core::parser::AstBuilder;

fn main() {
    let workflow_content = r#"
agent topic {
    model <- "ollama1/qwen3:8b"
    prompt <- "Suggest an interesting topic"
}

<- agent article {
    model <- "ollama1/qwen3:8b"
    prompt <- "Write about: {{ topic }}"
}
"#;

    let builder = AstBuilder::new("test.ai".to_string());

    match builder.parse(workflow_content) {
        Ok(workflow) => {
            for agent in &workflow.agents {
                println!("\nAgent: {}", agent.name);
                for (i, prop) in agent.properties.iter().enumerate() {
                    println!("  Property {i}: {prop:?}");
                }
            }
        }
        Err(error) => {
            eprintln!("Parse error: {error}");
        }
    }
}
