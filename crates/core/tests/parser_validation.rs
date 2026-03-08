use engine_ai_core::ast::Expression;
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

#[test]
fn parses_workflow_input_and_output_blocks() {
    let source = r#"
input {
    user_name: string
}

output {
    greeting <- input.user_name
    metadata <- { source <- "workflow" }
}
"#;

    let document = parse_workflow(source).expect("workflow should parse");
    assert!(document.input.is_some());
    assert!(matches!(document.output, Some(Expression::Object(_))));
}

#[test]
fn allows_input_references_in_prompts_and_workflow_output() {
    let source = r#"
provider ollama1 {
    driver <- "ollama"
    models <- ["qwen3.5:27b"]
}

input {
    user_name: string
}

output {
    name <- input.user_name
}

agent greet {
    model <- "ollama1/qwen3.5:27b"
    prompt <- "Hello {{ input.user_name }}"
}
"#;

    let document = parse_workflow(source).expect("workflow should parse");
    validate_workflow(&document).expect("workflow should validate");
}

#[test]
fn rejects_duplicate_workflow_input_blocks() {
    let source = r#"
input {
    user_name: string
}

input {
    task_name: string
}
"#;

    let error = parse_workflow(source).expect_err("workflow should fail to parse");
    assert!(error.to_string().contains("duplicate workflow input block"));
}

#[test]
fn rejects_duplicate_workflow_output_blocks() {
    let source = r#"
output {
    first <- "one"
}

output {
    second <- "two"
}
"#;

    let error = parse_workflow(source).expect_err("workflow should fail to parse");
    assert!(error.to_string().contains("duplicate workflow output block"));
}

#[test]
fn parses_compact_function_in_workflow_output() {
    let source = r#"
provider local {
    driver <- "mock"
    models <- ["demo"]
}

agent collect {
    model <- "local/demo"
    prompt <- "Generate data"
}

output {
    summary <- compact { model <- "local/demo", context <- [agent.collect.context] }
}
"#;

    let document = parse_workflow(source).expect("workflow should parse");
    validate_workflow(&document).expect("workflow should validate");
}

#[test]
fn rejects_compact_function_without_model() {
    let source = r#"
provider local {
    driver <- "mock"
    models <- ["demo"]
}

agent collect {
    model <- "local/demo"
    prompt <- "Generate data"
}

output {
    summary <- compact { context <- [agent.collect.context] }
}
"#;

    let document = parse_workflow(source).expect("workflow should parse");
    let error = validate_workflow(&document).expect_err("validation should fail");
    assert!(error.to_string().contains("compact function requires 'model' argument"));
}

#[test]
fn rejects_compact_function_without_context() {
    let source = r#"
provider local {
    driver <- "mock"
    models <- ["demo"]
}

agent collect {
    model <- "local/demo"
    prompt <- "Generate data"
}

output {
    summary <- compact { model <- "local/demo" }
}
"#;

    let document = parse_workflow(source).expect("workflow should parse");
    let error = validate_workflow(&document).expect_err("validation should fail");
    assert!(error
        .to_string()
        .contains("compact function requires 'context' argument"));
}

#[test]
fn rejects_agent_context_summary_in_workflow_output() {
    let source = r#"
provider local {
    driver <- "mock"
    models <- ["demo"]
}

agent collect {
    model <- "local/demo"
    prompt <- "Generate data"
}

output {
    summary <- agent.collect.context.summary
}
"#;

    let document = parse_workflow(source).expect("workflow should parse");
    let error = validate_workflow(&document).expect_err("validation should fail");
    assert!(error
        .to_string()
        .contains("agent.<name>.context.summary is not supported"));
    assert!(error.to_string().contains("Use compact function instead"));
}
