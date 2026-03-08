use engine_ai_core::parser::AstBuilder;

#[test]
fn test_context_sharing_reference() {
    let workflow = r#"
provider ollama1 {
    driver <- "ollama"
    models <- ["qwen3:8b"]
}

agent one {
    model <- "ollama1/qwen3:8b"
    prompt <- "Hello"
}

agent two {
    model <- "ollama1/qwen3:8b"
    context <- agent.one.context
    prompt <- "Continue the conversation"
}
"#;

    let builder = AstBuilder::new("test.ai".to_string());
    let result = builder.parse(workflow);

    assert!(result.is_ok());
    let workflow = result.unwrap();

    assert_eq!(workflow.agents.len(), 2);

    let agent_two = &workflow.agents[1];
    let context_property = agent_two.properties.iter().find_map(|prop| {
        if let engine_ai_core::ast::AgentProperty::Context { value, .. } = prop {
            Some(value)
        } else {
            None
        }
    });

    assert!(context_property.is_some());

    if let Some(engine_ai_core::ast::Value::Reference(reference)) = context_property {
        if let engine_ai_core::ast::Reference::AgentContext { agent } = reference {
            assert_eq!(agent, "one");
        } else {
            panic!("Expected AgentContext reference");
        }
    } else {
        panic!("Expected reference value");
    }
}

#[test]
fn test_context_in_output_block() {
    let workflow = r#"
provider ollama1 {
    driver <- "ollama"
    models <- ["qwen3:8b"]
}

agent collect_person {
    model <- "ollama1/qwen3:8b"
    output <- {
        name: string
    }
    prompt <- "Generate a person"
}

output {
    person_name <- agent.collect_person.name
    context <- agent.collect_person.context
}
"#;

    let builder = AstBuilder::new("test.ai".to_string());
    let result = builder.parse(workflow);

    assert!(result.is_ok());
    let workflow = result.unwrap();

    assert!(workflow.output.is_some());
    let output_block = workflow.output.unwrap();

    let context_field = output_block.fields.iter().find(|f| f.name == "context");
    assert!(context_field.is_some());

    if let Some(field) = context_field {
        if let engine_ai_core::ast::Value::Reference(reference) = &field.value {
            if let engine_ai_core::ast::Reference::AgentContext { agent } = reference {
                assert_eq!(agent, "collect_person");
            } else {
                panic!("Expected AgentContext reference");
            }
        } else {
            panic!("Expected reference value");
        }
    }
}

#[test]
fn test_compact_function_with_context() {
    let workflow = r#"
provider ollama1 {
    driver <- "ollama"
    models <- ["qwen3:8b"]
}

agent collect_person {
    model <- "ollama1/qwen3:8b"
    output <- {
        name: string
    }
    prompt <- "Generate a person"
}

output {
    summary <- compact {
        model <- "ollama1/qwen3:8b"
        context <- agent.collect_person.context
    }
}
"#;

    let builder = AstBuilder::new("test.ai".to_string());
    let result = builder.parse(workflow);

    assert!(result.is_ok());
    let workflow = result.unwrap();

    assert!(workflow.output.is_some());
    let output_block = workflow.output.unwrap();

    let summary_field = output_block.fields.iter().find(|f| f.name == "summary");
    assert!(summary_field.is_some());

    if let Some(field) = summary_field {
        if let engine_ai_core::ast::Value::FunctionCall(function_call) = &field.value {
            assert_eq!(function_call.name, "compact");
            assert!(function_call.arguments.contains_key("model"));
            assert!(function_call.arguments.contains_key("context"));
        } else {
            panic!("Expected function call value");
        }
    }
}

#[test]
fn test_compact_function_with_multiple_contexts() {
    let workflow = r#"
provider ollama1 {
    driver <- "ollama"
    models <- ["qwen3:8b"]
}

agent one {
    model <- "ollama1/qwen3:8b"
    prompt <- "First"
}

agent two {
    model <- "ollama1/qwen3:8b"
    prompt <- "Second"
}

output {
    combined_summary <- compact {
        model <- "ollama1/qwen3:8b"
        context <- [agent.one.context, agent.two.context]
    }
}
"#;

    let builder = AstBuilder::new("test.ai".to_string());
    let result = builder.parse(workflow);

    assert!(result.is_ok());
    let workflow = result.unwrap();

    assert!(workflow.output.is_some());
    let output_block = workflow.output.unwrap();

    let summary_field = output_block.fields.iter().find(|f| f.name == "combined_summary");
    assert!(summary_field.is_some());

    if let Some(field) = summary_field {
        if let engine_ai_core::ast::Value::FunctionCall(function_call) = &field.value {
            assert_eq!(function_call.name, "compact");

            if let Some(engine_ai_core::ast::Value::Array(contexts)) = function_call.arguments.get("context") {
                assert_eq!(contexts.len(), 2);
            } else {
                panic!("Expected array of contexts");
            }
        } else {
            panic!("Expected function call value");
        }
    }
}
