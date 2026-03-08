use engine_ai_core::parser::AstBuilder;

#[test]
fn test_parse_enum_schema() {
    let workflow = r#"
provider ollama1 {
    driver <- "ollama"
    models <- ["qwen3:8b"]
}

agent test {
    model <- "ollama1/qwen3:8b"

    output <- {
        status: "active" | "inactive" | "pending"
        priority: "low" | "medium" | "high"
    }

    prompt <- "Generate a task"
}
"#;

    let builder = AstBuilder::new("test.ai".to_string());
    let result = builder.parse(workflow);

    assert!(result.is_ok());
    let workflow = result.unwrap();

    let output_property = workflow.agents[0]
        .properties
        .iter()
        .find_map(|p| {
            if let engine_ai_core::ast::AgentProperty::Output { value, .. } = p {
                Some(value)
            } else {
                None
            }
        })
        .unwrap();

    if let engine_ai_core::ast::SchemaReference::Inline(schema) = output_property {
        assert_eq!(schema.fields.len(), 2);

        let status_field = schema.fields.iter().find(|f| f.name == "status").unwrap();
        if let engine_ai_core::ast::SchemaType::Enum(values) = &status_field.field_type {
            assert_eq!(values.len(), 3);
            assert!(values.contains(&"active".to_string()));
            assert!(values.contains(&"inactive".to_string()));
            assert!(values.contains(&"pending".to_string()));
        } else {
            panic!("Expected enum type for status field");
        }

        let priority_field = schema.fields.iter().find(|f| f.name == "priority").unwrap();
        if let engine_ai_core::ast::SchemaType::Enum(values) = &priority_field.field_type {
            assert_eq!(values.len(), 3);
            assert!(values.contains(&"low".to_string()));
            assert!(values.contains(&"medium".to_string()));
            assert!(values.contains(&"high".to_string()));
        } else {
            panic!("Expected enum type for priority field");
        }
    }
}

#[test]
fn test_enum_schema_compilation() {
    use engine_ai_core::schemas::SchemaCompiler;

    let workflow = r#"
provider ollama1 {
    driver <- "ollama"
    models <- ["qwen3:8b"]
}

agent test {
    model <- "ollama1/qwen3:8b"

    output <- {
        status: "active" | "inactive"
    }

    prompt <- "Generate a task"
}
"#;

    let builder = AstBuilder::new("test.ai".to_string());
    let workflow = builder.parse(workflow).unwrap();

    let output_property = workflow.agents[0]
        .properties
        .iter()
        .find_map(|p| {
            if let engine_ai_core::ast::AgentProperty::Output { value, .. } = p {
                Some(value)
            } else {
                None
            }
        })
        .unwrap();

    if let engine_ai_core::ast::SchemaReference::Inline(schema) = output_property {
        let json_schema = SchemaCompiler::compile(schema).unwrap();

        let properties = json_schema.get("properties").unwrap().as_object().unwrap();
        let status_schema = properties.get("status").unwrap();

        let enum_values = status_schema.get("enum").unwrap().as_array().unwrap();
        assert_eq!(enum_values.len(), 2);
        assert!(enum_values.contains(&serde_json::json!("active")));
        assert!(enum_values.contains(&serde_json::json!("inactive")));
    }
}
