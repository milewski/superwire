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
    prompts: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl Provider for MockProvider {
    async fn chat(
        &self,
        _model: &ProviderModelConfig,
        request: &ProviderRequest,
    ) -> Result<ProviderResponse, ProviderError> {
        *self.last_prompt.lock().unwrap() = request.prompt.clone();
        self.prompts.lock().unwrap().push(request.prompt.clone());

        let output = if request
            .prompt
            .contains("Summarize the following agent conversation history")
        {
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
            prompts: Arc::new(Mutex::new(Vec::new())),
        }),
    );

    let output = execute_workflow(&document, &registry, Some(&json!({})))
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
            prompts: Arc::new(Mutex::new(Vec::new())),
        }),
    );

    let output = execute_workflow(&document, &registry, Some(&json!({})))
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

    let _output = execute_workflow(&document, &registry, Some(&json!({})))
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

#[tokio::test]
async fn interpolates_runtime_inputs_into_prompts() {
    let source = r#"
provider local {
    driver <- "mock"
    models <- ["demo"]
}

input {
    user_name: string
}

<- agent greet {
    model <- "local/demo"
    prompt <- "Return only the result: Hello {{ input.user_name }}"
}
"#;

    let document = parse_workflow(source).expect("workflow should parse");
    validate_workflow(&document).expect("workflow should validate");

    let provider = Arc::new(MockProvider::default());
    let last_prompt = provider.last_prompt.clone();

    let mut registry = ProviderRegistry::default();
    registry.register("mock", provider);

    let _output = execute_workflow(&document, &registry, Some(&json!({ "user_name": "Rafael" })))
        .await
        .expect("workflow should execute");

    let prompt = last_prompt.lock().unwrap();
    assert!(prompt.contains("Hello Rafael"));
}

#[tokio::test]
async fn returns_workflow_level_output_without_terminal_agents() {
    let source = r#"
provider local {
    driver <- "mock"
    models <- ["demo"]
}

input {
    user_name: string
}

output {
    greeting <- input.user_name
}

agent helper {
    model <- "local/demo"
    prompt <- "Generate a person"
}
"#;

    let document = parse_workflow(source).expect("workflow should parse");
    validate_workflow(&document).expect("workflow should validate");

    let mut registry = ProviderRegistry::default();
    registry.register("mock", Arc::new(MockProvider::default()));

    let output = execute_workflow(&document, &registry, Some(&json!({ "user_name": "Rafael" })))
        .await
        .expect("workflow should execute");

    assert_eq!(output, json!({ "greeting": "Rafael" }));
}

#[tokio::test]
async fn merges_workflow_output_with_terminal_agent_outputs() {
    let source = r#"
provider local {
    driver <- "mock"
    models <- ["demo"]
}

input {
    user_name: string
}

output {
    requested_by <- input.user_name
}

<- agent collect {
    model <- "local/demo"
    prompt <- "Generate a person"
}
"#;

    let document = parse_workflow(source).expect("workflow should parse");
    validate_workflow(&document).expect("workflow should validate");

    let mut registry = ProviderRegistry::default();
    registry.register("mock", Arc::new(MockProvider::default()));

    let output = execute_workflow(&document, &registry, Some(&json!({ "user_name": "Rafael" })))
        .await
        .expect("workflow should execute");

    assert_eq!(output["requested_by"], "Rafael");
    assert_eq!(output["collect"]["name"], "Ada Lovelace");
}

#[tokio::test]
async fn returns_agent_context_summary_from_workflow_output() {
    let source = r#"
provider local {
    driver <- "mock"
    models <- ["demo"]
}

output {
    context <- agent.collect.context
    summary <- agent.collect.context.summary
}

agent collect {
    model <- "local/demo"
    prompt <- "Generate a person"
}

agent consume_context {
    model <- "local/demo"
    context <- agent.collect.context.summary
    prompt <- "Return only the result"
}
"#;

    let document = parse_workflow(source).expect("workflow should parse");
    validate_workflow(&document).expect("workflow should validate");

    let provider = Arc::new(MockProvider::default());
    let prompts = provider.prompts.clone();

    let mut registry = ProviderRegistry::default();
    registry.register("mock", provider);

    let output = execute_workflow(&document, &registry, Some(&json!({})))
        .await
        .expect("workflow should execute");

    assert!(output["context"].is_array());
    assert_eq!(output["context"][0]["type"], "user");
    assert!(output["context"][0]["value"]
        .as_str()
        .unwrap()
        .contains("Generate a person"));
    assert!(output["context"]
        .as_array()
        .unwrap()
        .iter()
        .any(|message| message["type"] == "assistant"
            && message["value"].as_str().unwrap().contains("assistant message")));
    assert!(output["context"]
        .as_array()
        .unwrap()
        .iter()
        .any(|message| message["type"] == "tool_call" && message["name"] == "done"));
    assert_eq!(output["summary"], "assistant message");

    let all_prompts = prompts.lock().unwrap();
    let summary_prompt = all_prompts
        .iter()
        .find(|prompt| prompt.contains("Summarize the following agent conversation history"))
        .expect("summary prompt should be issued");
    assert!(summary_prompt.contains("User: Generate a person"));
    assert!(summary_prompt.contains("Assistant: assistant message"));
    assert!(summary_prompt.contains("Tool Call: done"));
}

#[tokio::test]
async fn validates_runtime_input_payloads_against_declared_schema() {
    let source = r#"
input {
    user_name: string
}
"#;

    let document = parse_workflow(source).expect("workflow should parse");
    validate_workflow(&document).expect("workflow should validate");

    let registry = ProviderRegistry::default();
    let error = execute_workflow(&document, &registry, Some(&json!({})))
        .await
        .expect_err("workflow should fail validation");

    assert!(error.to_string().contains("workflow input validation failed"));
}
