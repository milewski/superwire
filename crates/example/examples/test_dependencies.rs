use engine_ai_core::parser::AstBuilder;
use engine_ai_core::parser::DependencyGraph;

fn main() {
    let workflow_content = r#"
agent topic {
    model <- "ollama1/qwen3:8b"
    prompt <- "Suggest an interesting topic for a blog post. Reply with just the topic, nothing else."
}

<- agent article {
    model <- "ollama1/qwen3:8b"
    prompt <- "Write a short article about: {{ topic }}"
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
            }

            match DependencyGraph::build(&workflow) {
                Ok(graph) => {
                    let order = graph.topological_order();
                    println!("\nExecution order: {:?}", order);
                }
                Err(error) => {
                    eprintln!("Dependency graph error: {}", error);
                }
            }
        }
        Err(error) => {
            eprintln!("Parse error: {}", error);
        }
    }
}
