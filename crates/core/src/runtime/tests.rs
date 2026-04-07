use crate::parse_inline_workflow;
use crate::runtime::{AgentExecutionRequest, AgentExecutionResult, AgentRunner, RequestedAgentTool, WorkflowRuntime, WorkflowRuntimeError};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;
use std::{env, fs};
use tokio::time::sleep;

static TEMPORARY_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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
    captured_contexts: Arc<Mutex<Vec<Option<Value>>>>,
    captured_tools: Arc<Mutex<Vec<Vec<RequestedAgentTool>>>>,
}

impl ScriptedRunner {
    pub fn from_outputs(outputs: Vec<Value>) -> Self {
        Self {
            queued_outputs: Arc::new(Mutex::new(VecDeque::from(outputs))),
            captured_prompts: Arc::new(Mutex::new(Vec::new())),
            captured_contexts: Arc::new(Mutex::new(Vec::new())),
            captured_tools: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn prompts(&self) -> Vec<String> {
        self.captured_prompts.lock().expect("prompt lock should not be poisoned").clone()
    }

    pub fn captured_tools(&self) -> Vec<Vec<RequestedAgentTool>> {
        self.captured_tools
            .lock()
            .expect("captured tools lock should not be poisoned")
            .clone()
    }

    pub fn contexts(&self) -> Vec<Option<Value>> {
        self.captured_contexts
            .lock()
            .expect("captured contexts lock should not be poisoned")
            .clone()
    }
}

#[async_trait]
impl AgentRunner for ScriptedRunner {
    async fn run_agent(&self, request: &AgentExecutionRequest) -> Result<AgentExecutionResult, WorkflowRuntimeError> {
        self.captured_prompts
            .lock()
            .expect("prompt lock should not be poisoned")
            .push(request.prompt.clone());

        self.captured_contexts
            .lock()
            .expect("captured contexts lock should not be poisoned")
            .push(request.context.clone());

        self.captured_tools
            .lock()
            .expect("captured tools lock should not be poisoned")
            .push(request.requested_tools.clone());

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

#[derive(Debug, Clone, Default)]
pub struct ParallelProbeRunner {
    current_inflight_agents: Arc<AtomicUsize>,
    max_inflight_agents: Arc<AtomicUsize>,
}

impl ParallelProbeRunner {
    pub fn max_inflight_agents(&self) -> usize {
        self.max_inflight_agents.load(Ordering::SeqCst)
    }

    fn record_inflight_peak(&self, inflight_agent_count: usize) {
        loop {
            let current_peak = self.max_inflight_agents.load(Ordering::SeqCst);

            if inflight_agent_count <= current_peak {
                return;
            }

            if self
                .max_inflight_agents
                .compare_exchange(current_peak, inflight_agent_count, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                return;
            }
        }
    }
}

#[async_trait]
impl AgentRunner for ParallelProbeRunner {
    async fn run_agent(&self, request: &AgentExecutionRequest) -> Result<AgentExecutionResult, WorkflowRuntimeError> {
        let inflight_agent_count = self.current_inflight_agents.fetch_add(1, Ordering::SeqCst) + 1;

        self.record_inflight_peak(inflight_agent_count);

        sleep(Duration::from_millis(40)).await;

        self.current_inflight_agents.fetch_sub(1, Ordering::SeqCst);

        Ok(AgentExecutionResult {
            output: json!(request.agent_name),
            context: json!({
                "agent": request.agent_name,
            }),
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct EchoModelRunner;

#[async_trait]
impl AgentRunner for EchoModelRunner {
    async fn run_agent(&self, request: &AgentExecutionRequest) -> Result<AgentExecutionResult, WorkflowRuntimeError> {
        Ok(AgentExecutionResult {
            output: json!(request.model_name),
            context: json!({
                "agent": request.agent_name,
                "model": request.model_name,
            }),
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct SchemaProbeRunner {
    captured_output_schemas: Arc<Mutex<Vec<Value>>>,
}

impl SchemaProbeRunner {
    pub fn captured_output_schemas(&self) -> Vec<Value> {
        self.captured_output_schemas
            .lock()
            .expect("captured output schemas lock should not be poisoned")
            .clone()
    }
}

#[async_trait]
impl AgentRunner for SchemaProbeRunner {
    async fn run_agent(&self, request: &AgentExecutionRequest) -> Result<AgentExecutionResult, WorkflowRuntimeError> {
        let output_schema_value = serde_json::to_value(&request.output_schema).expect("output schema should serialize to JSON value");

        self.captured_output_schemas
            .lock()
            .expect("captured output schemas lock should not be poisoned")
            .push(output_schema_value);

        Ok(AgentExecutionResult {
            output: json!("ok"),
            context: json!({
                "agent": request.agent_name,
            }),
        })
    }
}

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

fn static_types_workflow() -> crate::dsl::Workflow {
    parse_inline_workflow! {
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
    }
}

fn static_types_payload() -> WorkflowPayload {
    WorkflowPayload {
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
        Err(WorkflowRuntimeError::InvalidWorkflow { issues })
            if issues.contains("workflow_compilation_error")
                && issues.contains("Workflow input type mismatch")
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
        Err(WorkflowRuntimeError::InvalidWorkflow { issues })
            if issues.contains("workflow_compilation_error")
                && issues.contains("Workflow output type mismatch")
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
async fn try_workflow_macro_executes_workflow_from_path_literal_without_input() {
    #[derive(Debug, Deserialize, JsonSchema, PartialEq)]
    struct Output {
        greeting: String,
    }

    let workflow_output: Output = crate::try_workflow!("fixtures/path_literal_output.wire")
        .await
        .expect("path-literal workflow should execute without input");

    assert_eq!(
        workflow_output,
        Output {
            greeting: "hello from path workflow".to_string(),
        }
    );
}

#[tokio::test]
async fn try_workflow_macro_executes_workflow_from_path_literal_with_input() {
    #[derive(Debug, Serialize, JsonSchema)]
    struct WorkflowInput {
        topic: String,
    }

    #[derive(Debug, Deserialize, JsonSchema, PartialEq)]
    struct WorkflowOutput {
        greeting: String,
    }

    let workflow_output: WorkflowOutput = crate::try_workflow!(
        "fixtures/path_literal_with_input.wire",
        WorkflowInput {
            topic: "hello from input".to_string(),
        }
    )
    .await
    .expect("path-literal workflow should execute with input");

    assert_eq!(
        workflow_output,
        WorkflowOutput {
            greeting: "hello from input".to_string(),
        }
    );
}

#[test]
fn tool_macro_loads_component_path_relative_to_callsite() {
    let tool_load_result = crate::tool!("fixtures/path_literal_output.wasm");

    match tool_load_result {
        Ok(_) => panic!("missing fixture component should return an error"),
        Err(workflow_runtime_error) => {
            assert!(workflow_runtime_error.to_string().contains("path_literal_output.wasm"));
        }
    }
}

#[tokio::test]
async fn supports_all_static_input_and_output_types_in_preflight_and_execution() {
    let workflow = static_types_workflow();

    let runtime = WorkflowRuntime::<WorkflowPayload, WorkflowPayload>::new(workflow).unwrap();
    let payload = static_types_payload();

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

#[tokio::test]
async fn executes_for_loop_iterations_in_parallel() {
    #[derive(Debug, Deserialize, JsonSchema, PartialEq)]
    struct Output {
        values: Vec<String>,
    }

    let workflow = parse_inline_workflow! {
        #BASE_PROVIDER_WORKFLOW;

        agent collect_values for item in [1, 2, 3] {
            model: openai("model-a")
            prompt: "echo {{ item }}"
            output: string
        }

        output {
            values: agent.collect_values
        }
    };

    let runtime = WorkflowRuntime::<(), Output>::new(workflow).expect("runtime should compile");
    let runner = ParallelProbeRunner::default();

    let output = runtime
        .run_with_runner((), &runner)
        .await
        .expect("workflow should run successfully");

    assert_eq!(output.values, vec!["collect_values", "collect_values", "collect_values"]);
    assert!(runner.max_inflight_agents() >= 2);
}

#[tokio::test]
async fn evaluates_agent_tools_entries_and_binds_named_tool_arguments() {
    #[derive(Debug, Serialize, JsonSchema)]
    struct Input {
        country: String,
    }

    #[derive(Debug, Deserialize, JsonSchema, PartialEq)]
    struct Output {
        weather: String,
    }

    let workflow = parse_inline_workflow! {
        #BASE_PROVIDER_WORKFLOW;

        input {
            country: string
        }

        agent assistant {
            model: openai("model-a")
            tools: [
                tool.weather,
                tool.lookup_weather(country: input.country, static_mode: true)
            ]
            prompt: "Use tools"
            output: string
        }

        output {
            weather: agent.assistant
        }
    };

    let runtime = WorkflowRuntime::<Input, Output>::new(workflow).expect("runtime should compile");
    let runner = ScriptedRunner::from_outputs(vec![json!("sunny")]);
    let output = runtime
        .run_with_runner(
            Input {
                country: "Spain".to_string(),
            },
            &runner,
        )
        .await
        .expect("workflow should run successfully");

    assert_eq!(
        output,
        Output {
            weather: "sunny".to_string(),
        }
    );

    let captured_tools = runner.captured_tools();
    assert_eq!(captured_tools.len(), 1);
    assert_eq!(captured_tools[0].len(), 2);
    assert_eq!(captured_tools[0][0].name, "weather");
    assert_eq!(captured_tools[0][0].bound_arguments, serde_json::Map::new());
    assert_eq!(captured_tools[0][1].name, "lookup_weather");
    assert_eq!(
        captured_tools[0][1].bound_arguments,
        json!({
            "country": "Spain",
            "static_mode": true
        })
        .as_object()
        .expect("bound arguments expectation should be an object")
        .clone()
    );
}

#[tokio::test]
async fn evaluates_tool_bound_arguments_from_secrets_context() {
    #[derive(Debug, Serialize, JsonSchema)]
    struct Input {
        country: String,
    }

    #[derive(Debug, Serialize)]
    struct Secrets {
        key: String,
    }

    #[derive(Debug, Deserialize, JsonSchema, PartialEq)]
    struct Output {
        weather: String,
    }

    let workflow = parse_inline_workflow! {
        #BASE_PROVIDER_WORKFLOW;

        input {
            country: string
        }

        secrets {
            key: string
        }

        agent assistant {
            model: openai("model-a")
            tools: [tool.weather(key: secrets.key)]
            prompt: "Use tools"
            output: string
        }

        output {
            weather: agent.assistant
        }
    };

    let runtime = WorkflowRuntime::<Input, Output>::new(workflow).expect("runtime should compile");
    let runner = ScriptedRunner::from_outputs(vec![json!("sunny")]);
    let output = runtime
        .run_with_runner_and_secrets(
            Input {
                country: "Spain".to_string(),
            },
            Secrets {
                key: "secret-key".to_string(),
            },
            &runner,
        )
        .await
        .expect("workflow should run successfully");

    assert_eq!(
        output,
        Output {
            weather: "sunny".to_string(),
        }
    );

    let captured_tools = runner.captured_tools();
    assert_eq!(captured_tools.len(), 1);
    assert_eq!(captured_tools[0].len(), 1);
    assert_eq!(captured_tools[0][0].name, "weather");
    assert_eq!(
        captured_tools[0][0].bound_arguments,
        json!({
            "key": "secret-key"
        })
        .as_object()
        .expect("bound arguments expectation should be an object")
        .clone()
    );
}

#[tokio::test]
async fn executes_independent_agents_in_parallel_batches() {
    #[derive(Debug, Deserialize, JsonSchema, PartialEq)]
    struct Output {
        review: String,
    }

    let workflow = parse_inline_workflow! {
        #BASE_PROVIDER_WORKFLOW;

        agent customer_story {
            model: openai("model-a")
            prompt: "customer"
            output: string
        }

        agent investor_story {
            model: openai("model-a")
            prompt: "investor"
            output: string
        }

        agent review {
            model: openai("model-a")
            prompt: "{{ agent.customer_story }} + {{ agent.investor_story }}"
            output: string
        }

        output {
            review: agent.review
        }
    };

    let runtime = WorkflowRuntime::<(), Output>::new(workflow).expect("runtime should compile");
    let runner = ParallelProbeRunner::default();

    let output = runtime
        .run_with_runner((), &runner)
        .await
        .expect("workflow should run successfully");

    assert_eq!(output.review, "review".to_string());
    assert!(runner.max_inflight_agents() >= 2);
}

#[tokio::test]
async fn resolves_provider_and_model_values_from_secrets() {
    #[derive(Debug, Serialize)]
    struct Secrets {
        endpoint: String,
        api_key: String,
        model: String,
    }

    #[derive(Debug, Deserialize, JsonSchema, PartialEq)]
    struct Output {
        resolved_model: String,
    }

    let workflow = parse_inline_workflow! {
        secrets {
            endpoint: string
            api_key: string
            model: string
        }

        provider openai {
            driver: "openai"
            endpoint: secrets.endpoint
            api_key: secrets.api_key
            models: [secrets.model]
        }

        agent resolver {
            model: openai(secrets.model)
            prompt: "resolve model"
            output: string
        }

        output {
            resolved_model: agent.resolver
        }
    };

    let runtime = WorkflowRuntime::<(), Output>::new(workflow).expect("runtime should compile");
    let runner = EchoModelRunner;
    let output = runtime
        .run_with_runner_and_secrets(
            (),
            Secrets {
                endpoint: "http://localhost:1234/v1".to_string(),
                api_key: "test-key".to_string(),
                model: "model-a".to_string(),
            },
            &runner,
        )
        .await
        .expect("workflow should run successfully");

    assert_eq!(output.resolved_model, "model-a".to_string());
}

#[tokio::test]
async fn includes_agent_output_description_in_generated_output_schema() {
    #[derive(Debug, Deserialize, JsonSchema, PartialEq)]
    struct Output {
        value: String,
    }

    let workflow = parse_inline_workflow! {
        #BASE_PROVIDER_WORKFLOW;

        agent greeting {
            model: openai("model-a")
            prompt: "test"
            output: string "example"
        }

        output {
            value: agent.greeting
        }
    };

    let runtime = WorkflowRuntime::<(), Output>::new(workflow).expect("runtime should compile");
    let runner = SchemaProbeRunner::default();

    let workflow_output = runtime
        .run_with_runner((), &runner)
        .await
        .expect("workflow should run successfully");

    assert_eq!(workflow_output.value, "ok".to_string());

    let captured_output_schemas = runner.captured_output_schemas();
    let first_agent_output_schema = captured_output_schemas
        .first()
        .expect("schema probe runner should capture at least one output schema");

    assert_eq!(first_agent_output_schema.get("description"), Some(&json!("example")));
}

#[tokio::test]
async fn creates_runtime_from_workflow_file_without_tools_directory() {
    #[derive(Debug, Deserialize, JsonSchema, PartialEq)]
    struct Output {
        value: String,
    }

    let project_directory = create_temporary_project_directory("runtime-from-file-without-tools");
    let workflow_path = project_directory.join("my_workflow.wire");

    let workflow_source = crate::workflow_source! {
        provider openai {
            driver: "openai"
            endpoint: "http://localhost:1234/v1"
            api_key: "test-api-key"
            models: ["model-a"]
        }

        agent assistant {
            model: openai("model-a")
            prompt: "hello"
            output: string
        }

        output {
            value: agent.assistant
        }
    };

    fs::write(&workflow_path, workflow_source).expect("workflow file should be written");

    let workflow_runtime = WorkflowRuntime::<(), Output>::from_file(&workflow_path).expect("runtime should compile from file");
    let scripted_runner = ScriptedRunner::from_outputs(vec![json!("ok")]);

    let workflow_output = workflow_runtime
        .run_with_runner((), &scripted_runner)
        .await
        .expect("workflow execution should succeed");

    assert_eq!(workflow_output.value, "ok".to_string());

    fs::remove_dir_all(project_directory).expect("temporary project directory should be removed");
}

fn create_temporary_project_directory(prefix: &str) -> PathBuf {
    let sequence_value = TEMPORARY_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary_directory = env::temp_dir().join(format!("superwire-{prefix}-{}-{sequence_value}", std::process::id()));

    fs::create_dir_all(&temporary_directory).expect("temporary directory should be created");

    temporary_directory
}
