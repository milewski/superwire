use crate::parse_inline_workflow;
use crate::runtime::{AgentExecutionRequest, AgentExecutionResult, AgentRunner, WorkflowRuntime, WorkflowRuntimeError};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::{Arc, LazyLock, Mutex};

pub static BASE_PROVIDER_WORKFLOW: LazyLock<crate::dsl::Workflow> = LazyLock::new(|| {
    parse_inline_workflow! {
        provider openai {
            driver: "openai"
            endpoint: "http://localhost:1234/v1"
            api_key: "test-api-key"
            models: ["model-a"]
        }
    }
});

#[derive(Debug, Clone)]
pub struct ScriptedRunner {
    queued_outputs: Arc<Mutex<VecDeque<Value>>>,
    captured_prompts: Arc<Mutex<Vec<String>>>,
}

impl ScriptedRunner {
    pub fn from_outputs(outputs: Vec<Value>) -> Self {
        Self {
            queued_outputs: Arc::new(Mutex::new(VecDeque::from(outputs))),
            captured_prompts: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn prompts(&self) -> Vec<String> {
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

    let workflow = parse_inline_workflow! {
        input { value: number }
        output { value: input.value }
    };

    assert!(matches!(
        WorkflowRuntime::<WrongInput, Output>::new(workflow),
        Err(WorkflowRuntimeError::InputTypeMismatch { .. })
    ));
}

#[test]
fn fails_preflight_when_output_type_does_not_match_dsl() {
    #[allow(dead_code)]
    #[derive(Debug, Deserialize, JsonSchema)]
    struct WrongOutput {
        greeting: i64,
    }

    let workflow = parse_inline_workflow! {
        #BASE_PROVIDER_WORKFLOW;

        agent greeting {
            model: openai("model-a")
            prompt: "hello"
            output: string
        }

        output {
            greeting: agent.greeting
        }
    };

    assert!(matches!(
        WorkflowRuntime::<(), WrongOutput>::new(workflow),
        Err(WorkflowRuntimeError::OutputTypeMismatch { .. })
    ));
}

#[test]
fn parse_inline_workflow_supports_composing_base_workflow_fragments() {
    let composed_workflow = parse_inline_workflow! {
        #BASE_PROVIDER_WORKFLOW;

        agent greeting {
            model: openai("model-a")
            prompt: "hello"
            output: string
        }

        output {
            greeting: agent.greeting
        }
    };

    assert!(composed_workflow.find_provider("openai").is_some());
    assert!(composed_workflow.find_agent("greeting").is_some());
    assert!(composed_workflow.find_output().is_some());
}

#[tokio::test]
async fn supports_all_static_input_and_output_types_in_preflight_and_execution() {
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
    #[serde(rename_all = "lowercase")]
    enum PublicationStatus {
        Draft,
        Ready,
        Published,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
    struct ScoredItem {
        id: String,
        score: i64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
    struct NestedObject {
        string_value: String,
        number_value: i64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
    struct WorkflowPayload {
        string_value: String,
        number_value: i64,
        float_value: f64,
        boolean_value: bool,
        explicit_null: (),
        nullable_string: Option<String>,
        nullable_number: Option<i64>,
        array: Vec<String>,
        fixed_array: [String; 3],
        array_of_objects: Vec<ScoredItem>,
        enum_value: PublicationStatus,
        nullable_enum: Option<PublicationStatus>,
        tuple_value: (String, i64, [String; 3]),
        nullable_tuple: Option<(String, i64, [String; 3])>,
        object_value: NestedObject,
        nullable_object: Option<NestedObject>,
    }

    let workflow = parse_inline_workflow! {
        input {
            string_value: string
            number_value: number
            float_value: float
            boolean_value: boolean
            explicit_null: null

            nullable_string: string | null
            nullable_number: number | null

            array: [string]
            fixed_array: [string; 3]
            array_of_objects: [{
                id: string
                score: number
            }]

            enum_value: "draft" | "ready" | "published"
            nullable_enum: "draft" | "ready" | "published" | null

            tuple_value: (string, number, [string; 3])
            nullable_tuple: (string, number, [string; 3]) | null

            object_value: {
                string_value: string
                number_value: number
            }

            nullable_object: {
                string_value: string
                number_value: number
            } | null
        }

        output {
            string_value: input.string_value
            number_value: input.number_value
            float_value: input.float_value
            boolean_value: input.boolean_value
            explicit_null: input.explicit_null

            nullable_string: input.nullable_string
            nullable_number: input.nullable_number

            array: input.array
            fixed_array: input.fixed_array
            array_of_objects: input.array_of_objects

            enum_value: input.enum_value
            nullable_enum: input.nullable_enum
            tuple_value: input.tuple_value
            nullable_tuple: input.nullable_tuple

            object_value: input.object_value
            nullable_object: input.nullable_object
        }
    };

    let runtime = WorkflowRuntime::<WorkflowPayload, WorkflowPayload>::new(workflow).unwrap();

    let payload = WorkflowPayload {
        string_value: "hello".to_string(),
        number_value: 42,
        float_value: 8.5,
        boolean_value: true,
        explicit_null: (),
        nullable_string: Some("optional text".to_string()),
        nullable_number: Some(77),
        array: vec!["one".to_string(), "two".to_string()],
        fixed_array: ["alpha".to_string(), "beta".to_string(), "gamma".to_string()],
        array_of_objects: vec![
            ScoredItem {
                id: "item-1".to_string(),
                score: 9,
            },
            ScoredItem {
                id: "item-2".to_string(),
                score: 13,
            },
        ],
        enum_value: PublicationStatus::Ready,
        nullable_enum: Some(PublicationStatus::Draft),
        tuple_value: ("tuple-label".to_string(), 3, ["x".to_string(), "y".to_string(), "z".to_string()]),
        nullable_tuple: Some(("maybe-tuple".to_string(), 11, ["m".to_string(), "n".to_string(), "o".to_string()])),
        object_value: NestedObject {
            string_value: "nested".to_string(),
            number_value: 100,
        },
        nullable_object: Some(NestedObject {
            string_value: "optional nested".to_string(),
            number_value: 200,
        }),
    };

    let result = runtime
        .run_with_runner(payload.clone(), &ScriptedRunner::from_outputs(Vec::new()))
        .await
        .expect("workflow execution should preserve all supported static types");

    assert_eq!(result, payload);
}

#[tokio::test]
async fn executes_string_workflow_and_returns_typed_output() {
    #[derive(Debug, Deserialize, JsonSchema, PartialEq)]
    struct Output {
        greeting: String,
    }

    let workflow = parse_inline_workflow! {
        #BASE_PROVIDER_WORKFLOW;

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
async fn rejects_nested_object_when_declared_output_is_string() {
    #[derive(Debug, Deserialize, JsonSchema, PartialEq)]
    struct Output {
        greeting: String,
    }

    let workflow = parse_inline_workflow! {
        #BASE_PROVIDER_WORKFLOW;

        agent greeting {
            model: openai("model-a")
            prompt: "generate message"
            output: string
        }

        output {
            greeting: agent.greeting
        }
    };

    let runtime = WorkflowRuntime::<(), Output>::new(workflow).expect("runtime should compile");
    let runner = ScriptedRunner::from_outputs(vec![json!({ "message": "hello from nested shape" })]);
    let output = runtime.run_with_runner((), &runner).await;

    assert!(matches!(output, Err(WorkflowRuntimeError::AgentOutputTypeMismatch { .. })));
}

#[tokio::test]
async fn rejects_wrapped_object_for_number_schema() {
    #[derive(Debug, Deserialize, JsonSchema, PartialEq)]
    struct Output {
        answer: i64,
    }

    let workflow = parse_inline_workflow! {
        #BASE_PROVIDER_WORKFLOW;

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
    let runner = ScriptedRunner::from_outputs(vec![json!({ "random_number": 42 })]);
    let output = runtime.run_with_runner((), &runner).await;

    assert!(matches!(output, Err(WorkflowRuntimeError::AgentOutputTypeMismatch { .. })));
}

#[tokio::test]
async fn executes_number_workflow_and_supports_multiple_integer_output_types() {
    #[derive(Debug, Deserialize, JsonSchema, PartialEq)]
    struct Output {
        signed_8: i8,
        signed_16: i16,
        signed_32: i32,
        signed_64: i64,
        signed_size: isize,
        unsigned_8: u8,
        unsigned_16: u16,
        unsigned_32: u32,
        unsigned_64: u64,
        unsigned_size: usize,
    }

    let workflow = parse_inline_workflow! {
        #BASE_PROVIDER_WORKFLOW;

        agent number_agent {
            model: openai("model-a")
            prompt: "return a number"
            output: number
        }

        output {
            signed_8: agent.number_agent
            signed_16: agent.number_agent
            signed_32: agent.number_agent
            signed_64: agent.number_agent
            signed_size: agent.number_agent
            unsigned_8: agent.number_agent
            unsigned_16: agent.number_agent
            unsigned_32: agent.number_agent
            unsigned_64: agent.number_agent
            unsigned_size: agent.number_agent
        }
    };

    let runtime = WorkflowRuntime::<(), Output>::new(workflow).expect("runtime should compile");
    let runner = ScriptedRunner::from_outputs(vec![json!(42)]);
    let output = runtime
        .run_with_runner((), &runner)
        .await
        .expect("workflow should run successfully");

    assert_eq!(
        output,
        Output {
            signed_8: 42,
            signed_16: 42,
            signed_32: 42,
            signed_64: 42,
            signed_size: 42,
            unsigned_8: 42,
            unsigned_16: 42,
            unsigned_32: 42,
            unsigned_64: 42,
            unsigned_size: 42,
        }
    );
}

#[tokio::test]
async fn rejects_string_value_for_number_schema() {
    #[derive(Debug, Deserialize, JsonSchema, PartialEq)]
    struct Output {
        answer: i64,
    }

    let workflow = parse_inline_workflow! {
        #BASE_PROVIDER_WORKFLOW;

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

    let workflow = parse_inline_workflow! {
        #BASE_PROVIDER_WORKFLOW;

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

    let workflow = parse_inline_workflow! {
        #BASE_PROVIDER_WORKFLOW;

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

    let workflow = parse_inline_workflow! {
        #BASE_PROVIDER_WORKFLOW;

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
