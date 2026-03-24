use crate::runtime::{AgentExecutionRequest, AgentExecutionResult, AgentRunner, WorkflowRuntime, WorkflowRuntimeError};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
struct ScriptedRunner {
    queued_outputs: Arc<Mutex<VecDeque<Value>>>,
    captured_prompts: Arc<Mutex<Vec<String>>>,
}

impl ScriptedRunner {
    fn from_outputs(outputs: Vec<Value>) -> Self {
        Self {
            queued_outputs: Arc::new(Mutex::new(VecDeque::from(outputs))),
            captured_prompts: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn prompts(&self) -> Vec<String> {
        self.captured_prompts.lock().expect("prompt lock should not be poisoned").clone()
    }
}

#[async_trait]
impl AgentRunner for ScriptedRunner {
    async fn run_agent(&self, request: &AgentExecutionRequest) -> Result<AgentExecutionResult, WorkflowRuntimeError> {
        self.captured_prompts
            .lock()
            .expect("prompt lock should not be poisoned")
            .push(request.prompt.clone());

        let output = self
            .queued_outputs
            .lock()
            .expect("output lock should not be poisoned")
            .pop_front()
            .expect("scripted runner should contain enough queued outputs");

        Ok(AgentExecutionResult {
            output,
            context: json!({
                "agent": request.agent_name,
                "prompt": request.prompt,
            }),
        })
    }
}

#[test]
fn fails_preflight_when_input_type_does_not_match_dsl() {
    #[derive(Debug, Serialize, JsonSchema)]
    struct WrongInput {
        value: String,
    }

    #[allow(dead_code)]
    #[derive(Debug, Deserialize, JsonSchema)]
    struct Output {
        value: i64,
    }

    let workflow = crate::parse_inline_workflow! {
        input {
            value: number
        }

        output {
            value: input.value
        }
    };

    let runtime_result = WorkflowRuntime::<WrongInput, Output>::new(workflow);

    assert!(matches!(runtime_result, Err(WorkflowRuntimeError::InputTypeMismatch { .. })));
}

#[test]
fn fails_preflight_when_output_type_does_not_match_dsl() {
    #[allow(dead_code)]
    #[derive(Debug, Deserialize, JsonSchema)]
    struct WrongOutput {
        greeting: i64,
    }

    let workflow = crate::parse_inline_workflow! {
        provider openai {
            driver: "openai"
            api_endpoint: "http://localhost:1234/v1"
            models: ["model-a"]
        }

        agent greeting {
            model: openai("model-a")
            prompt: "hello"
            output: string
        }

        output {
            greeting: agent.greeting
        }
    };

    let runtime_result = WorkflowRuntime::<(), WrongOutput>::new(workflow);

    assert!(matches!(runtime_result, Err(WorkflowRuntimeError::OutputTypeMismatch { .. })));
}

#[tokio::test]
async fn executes_string_workflow_and_returns_typed_output() {
    #[derive(Debug, Deserialize, JsonSchema, PartialEq)]
    struct Output {
        greeting: String,
    }

    let workflow = crate::parse_inline_workflow! {
        provider openai {
            driver: "openai"
            api_endpoint: "http://localhost:1234/v1"
            models: ["model-a"]
        }

        agent greeting {
            model: openai("model-a")
            prompt: "say hello"
            output: string
        }

        output {
            greeting: agent.greeting
        }
    };

    let runtime = WorkflowRuntime::<(), Output>::new(workflow).expect("runtime should compile");
    let runner = ScriptedRunner::from_outputs(vec![json!("hello world")]);
    let output = runtime
        .run_with_runner((), &runner)
        .await
        .expect("workflow should run successfully");

    assert_eq!(
        output,
        Output {
            greeting: "hello world".to_string(),
        }
    );
}

#[tokio::test]
async fn executes_number_workflow_and_returns_typed_output() {
    #[derive(Debug, Deserialize, JsonSchema, PartialEq)]
    struct Output {
        answer: i64,
    }

    let workflow = crate::parse_inline_workflow! {
        provider openai {
            driver: "openai"
            api_endpoint: "http://localhost:1234/v1"
            models: ["model-a"]
        }

        agent number_agent {
            model: openai("model-a")
            prompt: "return a number"
            output: number
        }

        output {
            answer: agent.number_agent
        }
    };

    let runtime = WorkflowRuntime::<(), Output>::new(workflow).expect("runtime should compile");
    let runner = ScriptedRunner::from_outputs(vec![json!(42)]);
    let output = runtime
        .run_with_runner((), &runner)
        .await
        .expect("workflow should run successfully");

    assert_eq!(output, Output { answer: 42 });
}

#[tokio::test]
async fn executes_number_workflow_with_usize_output_type() {
    #[derive(Debug, Deserialize, JsonSchema, PartialEq)]
    struct Output {
        answer: usize,
    }

    let workflow = crate::parse_inline_workflow! {
        provider openai {
            driver: "openai"
            api_endpoint: "http://localhost:1234/v1"
            models: ["model-a"]
        }

        agent number_agent {
            model: openai("model-a")
            prompt: "return a number"
            output: number
        }

        output {
            answer: agent.number_agent
        }
    };

    let runtime = WorkflowRuntime::<(), Output>::new(workflow).expect("runtime should compile");
    let runner = ScriptedRunner::from_outputs(vec![json!(42)]);
    let output = runtime
        .run_with_runner((), &runner)
        .await
        .expect("workflow should run successfully");

    assert_eq!(output, Output { answer: 42 });
}

#[tokio::test]
async fn rejects_string_value_for_number_schema() {
    #[derive(Debug, Deserialize, JsonSchema, PartialEq)]
    struct Output {
        answer: i64,
    }

    let workflow = crate::parse_inline_workflow! {
        provider openai {
            driver: "openai"
            api_endpoint: "http://localhost:1234/v1"
            models: ["model-a"]
        }

        agent number_agent {
            model: openai("model-a")
            prompt: "return a number"
            output: number
        }

        output {
            answer: agent.number_agent
        }
    };

    let runtime = WorkflowRuntime::<(), Output>::new(workflow).expect("runtime should compile");
    let runner = ScriptedRunner::from_outputs(vec![json!("42")]);

    let execution_result = runtime.run_with_runner((), &runner).await;

    assert!(matches!(
        execution_result,
        Err(WorkflowRuntimeError::AgentOutputTypeMismatch { .. })
    ));
}

#[tokio::test]
async fn executes_object_workflow_and_returns_typed_output() {
    #[derive(Debug, Deserialize, JsonSchema, PartialEq)]
    struct Output {
        profile: Profile,
    }

    #[derive(Debug, Deserialize, JsonSchema, PartialEq)]
    struct Profile {
        name: String,
        age: i64,
    }

    let workflow = crate::parse_inline_workflow! {
        provider openai {
            driver: "openai"
            api_endpoint: "http://localhost:1234/v1"
            models: ["model-a"]
        }

        agent profile_agent {
            model: openai("model-a")
            prompt: "return an object"
            output: {
                name: string
                age: number
            }
        }

        output {
            profile: agent.profile_agent
        }
    };

    let runtime = WorkflowRuntime::<(), Output>::new(workflow).expect("runtime should compile");
    let runner = ScriptedRunner::from_outputs(vec![json!({ "name": "Ada", "age": 35 })]);
    let output = runtime
        .run_with_runner((), &runner)
        .await
        .expect("workflow should run successfully");

    assert_eq!(
        output,
        Output {
            profile: Profile {
                name: "Ada".to_string(),
                age: 35,
            },
        }
    );
}

#[tokio::test]
async fn resolves_agent_dependency_interpolation_in_prompt() {
    #[derive(Debug, Deserialize, JsonSchema, PartialEq)]
    struct Output {
        final_message: String,
    }

    let workflow = crate::parse_inline_workflow! {
        provider openai {
            driver: "openai"
            api_endpoint: "http://localhost:1234/v1"
            models: ["model-a"]
        }

        agent first {
            model: openai("model-a")
            prompt: "generate profile"
            output: {
                name: string
            }
        }

        agent second {
            model: openai("model-a")
            prompt: "Say hi to {{ agent.first.name }}"
            output: string
        }

        output {
            final_message: agent.second
        }
    };

    let runtime = WorkflowRuntime::<(), Output>::new(workflow).expect("runtime should compile");
    let runner = ScriptedRunner::from_outputs(vec![json!({ "name": "Ada" }), json!("hello Ada")]);

    let output = runtime
        .run_with_runner((), &runner)
        .await
        .expect("workflow should run successfully");

    assert_eq!(
        output,
        Output {
            final_message: "hello Ada".to_string(),
        }
    );

    let prompts = runner.prompts();

    assert!(prompts[1].contains("Ada"));
}

#[tokio::test]
async fn executes_for_loop_agent_and_returns_array_output() {
    #[derive(Debug, Deserialize, JsonSchema, PartialEq)]
    struct Output {
        values: Vec<i64>,
    }

    let workflow = crate::parse_inline_workflow! {
        provider openai {
            driver: "openai"
            api_endpoint: "http://localhost:1234/v1"
            models: ["model-a"]
        }

        agent collect_numbers for item in [1, 2, 3] {
            model: openai("model-a")
            prompt: "echo {{ item }}"
            output: number
        }

        output {
            values: agent.collect_numbers
        }
    };

    let runtime = WorkflowRuntime::<(), Output>::new(workflow).expect("runtime should compile");
    let runner = ScriptedRunner::from_outputs(vec![json!(1), json!(2), json!(3)]);
    let output = runtime
        .run_with_runner((), &runner)
        .await
        .expect("workflow should run successfully");

    assert_eq!(output, Output { values: vec![1, 2, 3] });
}
