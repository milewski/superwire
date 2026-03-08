use engine_ai_core::parser::AstBuilder;

fn main() {
    let workflow_content = r#"
<- agent greeting {
    model <- "ollama1/qwen3:8b"
    prompt <- "Say hello and introduce yourself as an AI assistant"
}
"#;

    let builder = AstBuilder::new("test.ai".to_string());

    match builder.parse(workflow_content) {
        Ok(workflow) => {
            println!("Parsed workflow successfully!");
            println!("Agents: {}", workflow.agents.len());

            for agent in &workflow.agents {
                println!("\nAgent: {}", agent.name);
                println!("Terminal: {}", agent.is_terminal);
                println!("Properties: {}", agent.properties.len());

                for (index, property) in agent.properties.iter().enumerate() {
                    println!("  Property {}: {:?}", index, property);
                }
            }
        }
        Err(error) => {
            eprintln!("Parse error: {}", error);
        }
    }
}
