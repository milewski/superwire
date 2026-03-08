use engine_ai_core::parser::AstBuilder;
use engine_ai_core::validation::WorkflowValidator;

#[test]
fn test_compact_missing_model() {
    let workflow = r#"
provider ollama1 {
    driver <- "ollama"
    models <- ["qwen3:8b"]
}

agent one {
    model <- "ollama1/qwen3:8b"
    prompt <- "Hello"
}

output {
    summary <- compact {
        context <- agent.one.context
    }
}
"#;

    let builder = AstBuilder::new("test.ai".to_string());
    let parsed = builder.parse(workflow);
    assert!(parsed.is_ok());

    let workflow_ast = parsed.unwrap();
    let validation_result = WorkflowValidator::validate(&workflow_ast);

    assert!(validation_result.is_err());
    let errors = validation_result.unwrap_err();
    assert!(errors.iter().any(|e| matches!(
        e,
        engine_ai_core::validation::error::ValidationError::MissingRequiredArgument { .. }
    )));
}

#[test]
fn test_compact_missing_context() {
    let workflow = r#"
provider ollama1 {
    driver <- "ollama"
    models <- ["qwen3:8b"]
}

agent one {
    model <- "ollama1/qwen3:8b"
    prompt <- "Hello"
}

output {
    summary <- compact {
        model <- "ollama1/qwen3:8b"
    }
}
"#;

    let builder = AstBuilder::new("test.ai".to_string());
    let parsed = builder.parse(workflow);
    assert!(parsed.is_ok());

    let workflow_ast = parsed.unwrap();
    let validation_result = WorkflowValidator::validate(&workflow_ast);

    assert!(validation_result.is_err());
    let errors = validation_result.unwrap_err();
    assert!(errors.iter().any(|e| matches!(
        e,
        engine_ai_core::validation::error::ValidationError::MissingRequiredArgument { .. }
    )));
}

#[test]
fn test_compact_invalid_provider() {
    let workflow = r#"
provider ollama1 {
    driver <- "ollama"
    models <- ["qwen3:8b"]
}

agent one {
    model <- "ollama1/qwen3:8b"
    prompt <- "Hello"
}

output {
    summary <- compact {
        model <- "invalid_provider/qwen3:8b"
        context <- agent.one.context
    }
}
"#;

    let builder = AstBuilder::new("test.ai".to_string());
    let parsed = builder.parse(workflow);
    assert!(parsed.is_ok());

    let workflow_ast = parsed.unwrap();
    let validation_result = WorkflowValidator::validate(&workflow_ast);

    assert!(validation_result.is_err());
    let errors = validation_result.unwrap_err();
    assert!(errors.iter().any(|e| matches!(
        e,
        engine_ai_core::validation::error::ValidationError::UndefinedReference { .. }
    )));
}

#[test]
fn test_compact_invalid_model() {
    let workflow = r#"
provider ollama1 {
    driver <- "ollama"
    models <- ["qwen3:8b"]
}

agent one {
    model <- "ollama1/qwen3:8b"
    prompt <- "Hello"
}

output {
    summary <- compact {
        model <- "ollama1/invalid_model"
        context <- agent.one.context
    }
}
"#;

    let builder = AstBuilder::new("test.ai".to_string());
    let parsed = builder.parse(workflow);
    assert!(parsed.is_ok());

    let workflow_ast = parsed.unwrap();
    let validation_result = WorkflowValidator::validate(&workflow_ast);

    assert!(validation_result.is_err());
    let errors = validation_result.unwrap_err();
    assert!(errors.iter().any(|e| matches!(
        e,
        engine_ai_core::validation::error::ValidationError::ProviderModelMismatch { .. }
    )));
}

#[test]
fn test_compact_valid() {
    let workflow = r#"
provider ollama1 {
    driver <- "ollama"
    models <- ["qwen3:8b"]
}

agent one {
    model <- "ollama1/qwen3:8b"
    prompt <- "Hello"
}

output {
    summary <- compact {
        model <- "ollama1/qwen3:8b"
        context <- agent.one.context
    }
}
"#;

    let builder = AstBuilder::new("test.ai".to_string());
    let parsed = builder.parse(workflow);
    assert!(parsed.is_ok());

    let workflow_ast = parsed.unwrap();
    let validation_result = WorkflowValidator::validate(&workflow_ast);

    assert!(validation_result.is_ok());
}
