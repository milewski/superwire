use engine_ai_core::execution::RuntimeContext;
use engine_ai_core::parser::AstBuilder;
use engine_ai_core::workflow;
use serde_json::json;
use std::fs;
use std::io::Write;

#[test]
fn test_file_template_function() {
    let temp_dir = std::env::temp_dir();
    let template_path = temp_dir.join("test_template.txt");
    let mut file = fs::File::create(&template_path).unwrap();
    file.write_all(b"Hello {{ name }}, you are {{ age }} years old")
        .unwrap();

    let workflow = format!(
        r#"
provider ollama1 {{
    driver <- "ollama"
    models <- ["qwen3:8b"]
}}

agent test {{
    model <- "ollama1/qwen3:8b"

    prompt <- file "{}" {{
        name <- "Alice"
        age <- "30"
    }}
}}
"#,
        template_path.display()
    );

    let builder = AstBuilder::new("test.ai".to_string());
    let result = builder.parse(&workflow);

    assert!(result.is_ok());
    let workflow = result.unwrap();

    let prompt_property = workflow.agents[0].properties.iter().find_map(|prop| {
        if let engine_ai_core::ast::AgentProperty::Prompt { value, .. } = prop {
            Some(value)
        } else {
            None
        }
    });

    assert!(prompt_property.is_some());

    if let Some(engine_ai_core::ast::Value::FunctionCall(function_call)) = prompt_property {
        assert_eq!(function_call.name, "file");
        assert!(function_call.arguments.contains_key("name"));
        assert!(function_call.arguments.contains_key("age"));
    } else {
        panic!("Expected function call value");
    }

    fs::remove_file(template_path).ok();
}

#[test]
fn test_nested_file_template_functions() {
    let temp_dir = std::env::temp_dir();

    let inner_template_path = temp_dir.join("test_inner_template.txt");
    let mut inner_file = fs::File::create(&inner_template_path).unwrap();
    inner_file.write_all(b"Inner value: {{ subfield }}").unwrap();

    let outer_template_path = temp_dir.join("test_outer_template.txt");
    let mut outer_file = fs::File::create(&outer_template_path).unwrap();
    outer_file
        .write_all(b"System: {{ system }}, Field: {{ field_a }}")
        .unwrap();

    let workflow = format!(
        r#"
provider ollama1 {{
    driver <- "ollama"
    models <- ["qwen3:8b"]
}}

agent test {{
    model <- "ollama1/qwen3:8b"

    prompt <- file "{}" {{
        system <- "System instructions"
        field_a <- file "{}" {{
            subfield <- "Value for subfield"
        }}
    }}
}}
"#,
        outer_template_path.display(),
        inner_template_path.display()
    );

    let builder = AstBuilder::new("test.ai".to_string());
    let result = builder.parse(&workflow);

    assert!(result.is_ok());
    let workflow = result.unwrap();

    let prompt_property = workflow.agents[0].properties.iter().find_map(|prop| {
        if let engine_ai_core::ast::AgentProperty::Prompt { value, .. } = prop {
            Some(value)
        } else {
            None
        }
    });

    assert!(prompt_property.is_some());

    if let Some(engine_ai_core::ast::Value::FunctionCall(function_call)) = prompt_property {
        assert_eq!(function_call.name, "file");
        assert!(function_call.arguments.contains_key("system"));
        assert!(function_call.arguments.contains_key("field_a"));

        if let Some(engine_ai_core::ast::Value::FunctionCall(inner_function)) = function_call.arguments.get("field_a") {
            assert_eq!(inner_function.name, "file");
            assert!(inner_function.arguments.contains_key("subfield"));
        } else {
            panic!("Expected nested function call");
        }
    } else {
        panic!("Expected function call value");
    }

    fs::remove_file(inner_template_path).ok();
    fs::remove_file(outer_template_path).ok();
}

#[test]
fn test_file_template_with_interpolation() {
    let temp_dir = std::env::temp_dir();
    let template_path = temp_dir.join("test_template_interp.txt");
    let mut file = fs::File::create(&template_path).unwrap();
    file.write_all(b"Hello {{ name }}, topic: {{ topic }}").unwrap();

    let workflow = format!(
        r#"
provider ollama1 {{
    driver <- "ollama"
    models <- ["qwen3:8b"]
}}

input {{
    topic: string
}}

agent test {{
    model <- "ollama1/qwen3:8b"

    prompt <- file "{}" {{
        name <- "Alice"
        topic <- input.topic
    }}
}}
"#,
        template_path.display()
    );

    let builder = AstBuilder::new("test.ai".to_string());
    let result = builder.parse(&workflow);

    assert!(result.is_ok());
    let workflow = result.unwrap();

    assert!(workflow.input.is_some());

    let prompt_property = workflow.agents[0].properties.iter().find_map(|prop| {
        if let engine_ai_core::ast::AgentProperty::Prompt { value, .. } = prop {
            Some(value)
        } else {
            None
        }
    });

    assert!(prompt_property.is_some());

    if let Some(engine_ai_core::ast::Value::FunctionCall(function_call)) = prompt_property {
        assert_eq!(function_call.name, "file");
        assert!(function_call.arguments.contains_key("name"));
        assert!(function_call.arguments.contains_key("topic"));

        if let Some(engine_ai_core::ast::Value::Reference(reference)) = function_call.arguments.get("topic") {
            if let engine_ai_core::ast::Reference::Input { field } = reference {
                assert_eq!(field, "topic");
            } else {
                panic!("Expected Input reference");
            }
        } else {
            panic!("Expected reference value for topic");
        }
    } else {
        panic!("Expected function call value");
    }

    fs::remove_file(template_path).ok();
}

#[test]
fn test_static_interpolation_workflow_preserves_spaces() {
    let workflow = workflow! {
        provider ollama1 {
            driver <- "ollama"
            models <- ["qwen3:8b"]
        }

        agent test {
            model <- "ollama1/qwen3:8b"
            prompt <- "What is {{ input.num }} multiplied by 2?"
        }
    };

    let prompt_property = workflow.agents[0].properties.iter().find_map(|prop| {
        if let engine_ai_core::ast::AgentProperty::Prompt { value, .. } = prop {
            Some(value)
        } else {
            None
        }
    });

    assert_eq!(
        prompt_property,
        Some(&engine_ai_core::ast::Value::Interpolated(
            "What is {{ input.num }} multiplied by 2?".to_string(),
        ))
    );

    let mut runtime_context = RuntimeContext::new();
    runtime_context.set_input_value("num".to_string(), json!(3));

    let resolved = runtime_context
        .resolve_value(&engine_ai_core::ast::Value::Interpolated(
            "What is {{ input.num }} multiplied by 2?".to_string(),
        ))
        .unwrap();

    assert_eq!(resolved, json!("What is 3 multiplied by 2?"));
}
