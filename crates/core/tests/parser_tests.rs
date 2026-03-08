use engine_ai_core::parser::AstBuilder;

#[test]
fn test_parse_basic_agent() {
    let workflow = r#"
provider ollama1 {
    driver <- "ollama"
    api_endpoint <- "http://localhost:11434"
    models <- ["qwen3:8b"]
}

agent test {
    model <- "ollama1/qwen3:8b"
    prompt <- "Hello world"
}
"#;

    let builder = AstBuilder::new("test.ai".to_string());
    let result = builder.parse(workflow);

    assert!(result.is_ok());
    let workflow = result.unwrap();
    assert_eq!(workflow.agents.len(), 1);
    assert_eq!(workflow.agents[0].name, "test");
}

#[test]
fn test_parse_terminal_agent() {
    let workflow = r#"
provider ollama1 {
    driver <- "ollama"
    models <- ["qwen3:8b"]
}

<- agent test {
    model <- "ollama1/qwen3:8b"
    prompt <- "Hello"
}
"#;

    let builder = AstBuilder::new("test.ai".to_string());
    let result = builder.parse(workflow);

    assert!(result.is_ok());
    let workflow = result.unwrap();
    assert!(workflow.agents[0].is_terminal);
}

#[test]
fn test_parse_inline_schema() {
    let workflow = r#"
provider ollama1 {
    driver <- "ollama"
    models <- ["qwen3:8b"]
}

agent test {
    model <- "ollama1/qwen3:8b"
    
    output <- {
        name: string
        age: number
    }
    
    prompt <- "Generate a person"
}
"#;

    let builder = AstBuilder::new("test.ai".to_string());
    let result = builder.parse(workflow);

    assert!(result.is_ok());
}

#[test]
fn test_parse_string_interpolation() {
    let workflow = r#"
provider ollama1 {
    driver <- "ollama"
    models <- ["qwen3:8b"]
}

agent test {
    model <- "ollama1/qwen3:8b"
    prompt <- "Hello {{ input.name }}"
}
"#;

    let builder = AstBuilder::new("test.ai".to_string());
    let result = builder.parse(workflow);

    assert!(result.is_ok());
}

#[test]
fn test_parse_url_with_slashes() {
    let workflow = r#"
provider ollama1 {
    driver <- "ollama"
    api_endpoint <- "http://100.76.5.36:11434"
    models <- ["qwen3:8b"]
}

agent test {
    model <- "ollama1/qwen3:8b"
    prompt <- "Hello"
}
"#;

    let builder = AstBuilder::new("test.ai".to_string());
    let result = builder.parse(workflow);

    assert!(result.is_ok());
    let workflow = result.unwrap();
    assert_eq!(
        workflow.providers[0].api_endpoint,
        Some("http://100.76.5.36:11434".to_string())
    );
}
