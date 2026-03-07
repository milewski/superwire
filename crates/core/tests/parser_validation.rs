use engine_ai_core::parse_workflow;
use engine_ai_core::validation::validate_workflow;

#[test]
fn parses_basic_workflow() {
    let source = r#"
provider ollama1 {
    driver <- "ollama"
    api_endpoint <- "http://localhost:11434"
    models <- ["qwen3.5:27b"]
}

schema person {
    name: string "Full name"
}

agent collect {
    model <- "ollama1/qwen3.5:27b"
    output <- schema.person
    prompt <- "Generate a person"
}
"#;

    let document = parse_workflow(source).expect("workflow should parse");
    assert_eq!(document.providers.len(), 1);
    assert_eq!(document.schemas.len(), 1);
    assert_eq!(document.agents.len(), 1);
    assert_eq!(document.agents[0].name, "collect");
}

#[test]
fn rejects_duplicate_agents() {
    let source = r#"
provider ollama1 {
    driver <- "ollama"
    models <- ["qwen3.5:27b"]
}

agent one {
    model <- "ollama1/qwen3.5:27b"
}

agent one {
    model <- "ollama1/qwen3.5:27b"
}
"#;

    let document = parse_workflow(source).expect("workflow should parse");
    let error = validate_workflow(&document).expect_err("validation should fail");
    assert!(error.to_string().contains("duplicate agent"));
}

#[test]
fn rejects_unknown_provider_model_pair() {
    let source = r#"
provider ollama1 {
    driver <- "ollama"
    models <- ["qwen3.5:27b"]
}

agent one {
    model <- "ollama1/qwen3.5:72b"
}
"#;

    let document = parse_workflow(source).expect("workflow should parse");
    let error = validate_workflow(&document).expect_err("validation should fail");
    assert!(error.to_string().contains("not declared by provider"));
}

#[test]
fn parses_for_each_binding() {
    let source = r#"
provider ollama1 {
    driver <- "ollama"
    models <- ["qwen3.5:27b"]
}

<- agent multiply {
    model <- "ollama1/qwen3.5:27b"
    for_each <- [1, 2, 3] as index
    prompt <- "Return {{ index }}"
}
"#;

    let document = parse_workflow(source).expect("workflow should parse");
    let agent = &document.agents[0];
    assert!(agent.is_terminal);
    assert!(agent.for_each.is_some());
}
