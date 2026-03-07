#[cfg(test)]
mod tests {
    use crate::parser::parse_document;
    use crate::ast::{SchemaType, PromptValue, Expression};

    #[test]
    fn test_parse_simple_provider() {
        let input = r#"
provider ollama1 {
    driver <- "ollama"
    api_endpoint <- "http://localhost:11434"
    models <- ["qwen2.5:3b"]
}
        "#;

        let doc = parse_document(input).unwrap();
        assert_eq!(doc.providers.len(), 1);

        let provider = doc.providers.get("ollama1").unwrap();
        assert_eq!(provider.driver, "ollama");
        assert_eq!(provider.api_endpoint, "http://localhost:11434");
        assert_eq!(provider.models, vec!["qwen2.5:3b"]);
    }

    #[test]
    fn test_parse_schema() {
        let input = r#"
schema person {
    name: string
    age: number
    hobbies: [string]
}
        "#;

        let doc = parse_document(input).unwrap();
        assert_eq!(doc.schemas.len(), 1);

        let schema = doc.schemas.get("person").unwrap();
        assert_eq!(schema.fields.len(), 3);
        assert!(matches!(schema.fields.get("name"), Some(SchemaType::String)));
        assert!(matches!(schema.fields.get("age"), Some(SchemaType::Number)));
    }

    #[test]
    fn test_parse_agent() {
        let input = r#"
provider ollama1 {
    driver <- "ollama"
    api_endpoint <- "http://localhost:11434"
    models <- ["qwen2.5:3b"]
}

agent test_agent {
    model <- "ollama1/qwen2.5:3b"
    prompt <- "Hello world"
}
        "#;

        let doc = parse_document(input).unwrap();
        assert_eq!(doc.agents.len(), 1);

        let agent = doc.agents.get("test_agent").unwrap();
        assert_eq!(agent.model, Some("ollama1/qwen2.5:3b".to_string()));
        assert!(!agent.is_terminal);
    }

    #[test]
    fn test_parse_terminal_agent() {
        let input = r#"
provider ollama1 {
    driver <- "ollama"
    api_endpoint <- "http://localhost:11434"
    models <- ["qwen2.5:3b"]
}

<- agent terminal_agent {
    model <- "ollama1/qwen2.5:3b"
    prompt <- "Final output"
}
        "#;

        let doc = parse_document(input).unwrap();
        let agent = doc.agents.get("terminal_agent").unwrap();
        assert!(agent.is_terminal);
    }

    #[test]
    fn test_parse_multiline_prompt() {
        let input = r#"
provider ollama1 {
    driver <- "ollama"
    api_endpoint <- "http://localhost:11434"
    models <- ["qwen2.5:3b"]
}

agent test {
    model <- "ollama1/qwen2.5:3b"
    prompt <- """
        This is a multiline
        prompt with multiple lines
    """
}
        "#;

        let doc = parse_document(input).unwrap();
        let agent = doc.agents.get("test").unwrap();

        println!("Prompt: {:?}", agent.prompt);

        match &agent.prompt {
            PromptValue::Multiline(text) => {
                assert!(text.contains("multiline"));
                assert!(text.contains("multiple lines"));
            }
            PromptValue::Inline(text) => {
                // If it's inline, check if it contains the multiline content
                assert!(text.contains("multiline"));
                assert!(text.contains("multiple lines"));
            }
            _ => panic!("Expected multiline or inline prompt with multiline content"),
        }
    }

    #[test]
    fn test_parse_for_each() {
        let input = r#"
provider ollama1 {
    driver <- "ollama"
    api_endpoint <- "http://localhost:11434"
    models <- ["qwen2.5:3b"]
}

agent iterate {
    for_each <- [1, 2, 3] as item
    model <- "ollama1/qwen2.5:3b"
    prompt <- "Process {{ item }}"
}
        "#;

        let doc = parse_document(input).unwrap();
        let agent = doc.agents.get("iterate").unwrap();
        assert!(agent.for_each.is_some());

        let for_each = agent.for_each.as_ref().unwrap();
        assert_eq!(for_each.item_name, "item");

        if let Expression::Literal(values) = &for_each.collection {
            assert_eq!(values.len(), 3);
        } else {
            panic!("Expected literal array");
        }
    }
}
