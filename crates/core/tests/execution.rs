use std::sync::Arc;

use async_trait::async_trait;
use engine_ai_core::execution::orchestrator::execute_workflow;
use engine_ai_core::parse_workflow;
use engine_ai_core::providers::error::ProviderError;
use engine_ai_core::providers::provider::{Provider, ProviderModelConfig, ProviderRequest, ProviderResponse, ToolCall};
use engine_ai_core::providers::registry::ProviderRegistry;
use engine_ai_core::validation::validate_workflow;
use serde_json::json;
use std::sync::Mutex;

#[derive(Default)]
struct MockProvider {
    last_prompt: Arc<Mutex<String>>,
}

#[async_trait]
impl Provider for MockProvider {
    async fn chat(
        &self,
        _model: &ProviderModelConfig,
        request: &ProviderRequest,
    ) -> Result<ProviderResponse, ProviderError> {
        *self.last_prompt.lock().unwrap() = request.prompt.clone();

        let output = if request.prompt.contains("Summarize") {
            json!("summary")
        } else if request.prompt.contains("Return only the result") {
            json!(request.prompt.clone())
        } else if request.prompt.contains("Create a task") {
            json!({ "title": "Team Meeting", "priority": "high", "completed": false })
        } else {
            json!({ "name": "Ada Lovelace", "hobbies": ["math"] })
        };

        Ok(ProviderResponse {
            message: "assistant message".into(),
            tool_calls: vec![ToolCall {
                name: "done".into(),
                arguments: json!({
                    "status": "success",
                    "output": output,
                }),
            }],
        })
    }

    fn driver(&self) -> &'static str {
        "mock"
    }
}

#[tokio::test]
async fn executes_terminal_workflow() {
    let source = r#"
provider local {
    driver <- "mock"
    models <- ["demo"]
}

schema person {
    name: string
    hobbies: [string]
}

<- agent collect {
    model <- "local/demo"
    output <- schema {
        name: string
        hobbies: [string]
    }
    prompt <- "Generate a person"
}
"#;

    let document = parse_workflow(source).expect("workflow should parse");
    validate_workflow(&document).expect("workflow should validate");

    let mut registry = ProviderRegistry::default();
    registry.register(
        "mock",
        Arc::new(MockProvider {
            last_prompt: Arc::new(Mutex::new(String::new())),
        }),
    );

    let output = execute_workflow(&document, &registry)
        .await
        .expect("workflow should execute");

    assert_eq!(output["collect"]["name"], "Ada Lovelace");
}

#[tokio::test]
async fn executes_for_each_workflow() {
    let source = r#"
provider local {
    driver <- "mock"
    models <- ["demo"]
}

<- agent multiply {
    model <- "local/demo"
    for_each <- [1, 2, 3] as index
    prompt <- "Return only the result of {{ index }} * 5"
}
"#;

    let document = parse_workflow(source).expect("workflow should parse");
    validate_workflow(&document).expect("workflow should validate");

    let mut registry = ProviderRegistry::default();
    registry.register(
        "mock",
        Arc::new(MockProvider {
            last_prompt: Arc::new(Mutex::new(String::new())),
        }),
    );

    let output = execute_workflow(&document, &registry)
        .await
        .expect("workflow should execute");

    assert_eq!(output["multiply"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn injects_schema_into_agent_prompt() {
    let source = r#"
provider local {
    driver <- "mock"
    models <- ["demo"]
}

schema task {
    title: string "The task title"
    priority: "low" | "medium" | "high"
    completed: boolean
}

<- agent create_task {
    model <- "local/demo"
    output <- schema.task
    prompt <- "Create a task for organizing a team meeting"
}
"#;

    let document = parse_workflow(source).expect("workflow should parse");
    validate_workflow(&document).expect("workflow should validate");

    let provider = Arc::new(MockProvider::default());
    let last_prompt = provider.last_prompt.clone();

    let mut registry = ProviderRegistry::default();
    registry.register("mock", provider);

    let _output = execute_workflow(&document, &registry)
        .await
        .expect("workflow should execute");

    let prompt = last_prompt.lock().unwrap();
    assert!(prompt.contains("You must return your response as JSON following this exact schema"));
    assert!(prompt.contains("\"type\": \"object\""));
    assert!(prompt.contains("\"title\""));
    assert!(prompt.contains("\"priority\""));
    assert!(prompt.contains("\"completed\""));
    assert!(prompt.contains("The task title"));
}
