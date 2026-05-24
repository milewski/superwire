use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::hint::black_box;
use std::time::Instant;
use superwire_dsl::{parse_workflow, validate_workflow, AgentExpressionPropertyName, Workflow};
use superwire_executor::model::{ModelProvider, ModelRequest, ModelResponse};
use superwire_executor::{ExecutorError, WorkflowExecutor};
use superwire_semantic::support::expression::{evaluate_expression, EvaluationContext};
use superwire_semantic::support::types::WorkflowSchemaCache;
use superwire_semantic::{build_dynamic_typed_workflow_ir, build_execution_plan, ExecutionPlan};
use tokio::runtime::{Builder, Runtime};

const DEFAULT_ITERATIONS: usize = 50;
const DEFAULT_WARMUP_ITERATIONS: usize = 5;
const ITERATIONS_ENVIRONMENT_VARIABLE: &str = "SUPERWIRE_BENCH_ITERATIONS";
const WARMUP_ITERATIONS_ENVIRONMENT_VARIABLE: &str = "SUPERWIRE_BENCH_WARMUP_ITERATIONS";
const SMALL_WORKFLOW_SOURCE: &str = include_str!("../tests/fixtures/001_minimum.wire");
const MEDIUM_WORKFLOW_SOURCE: &str = include_str!("../tests/fixtures/019_diamond_dependency.wire");
const LARGE_WORKFLOW_SOURCE: &str = include_str!("../tests/fixtures/037_schema_types.wire");

#[derive(Debug, Clone, Copy)]
enum BenchmarkStage {
    Parsing,
    Validation,
    Planning,
    PromptRendering,
    SchemaResolution,
    FakeProviderExecution,
}

impl BenchmarkStage {
    fn as_str(self) -> &'static str {
        match self {
            Self::Parsing => "parsing",
            Self::Validation => "validation",
            Self::Planning => "planning",
            Self::PromptRendering => "prompt_rendering",
            Self::SchemaResolution => "schema_resolution",
            Self::FakeProviderExecution => "fake_provider_execution",
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum WorkflowScale {
    Small,
    Medium,
    Large,
}

impl WorkflowScale {
    fn as_str(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Large => "large",
        }
    }
}

#[derive(Debug)]
struct BenchmarkRunner {
    iterations: usize,
    warmup_iterations: usize,
}

impl BenchmarkRunner {
    fn from_environment() -> Self {
        Self {
            iterations: read_iteration_count(ITERATIONS_ENVIRONMENT_VARIABLE, DEFAULT_ITERATIONS),
            warmup_iterations: read_iteration_count(WARMUP_ITERATIONS_ENVIRONMENT_VARIABLE, DEFAULT_WARMUP_ITERATIONS),
        }
    }

    fn run_stage<Operation, Output>(&self, workflow: &BenchmarkWorkflow, benchmark_stage: BenchmarkStage, mut operation: Operation)
    where
        Operation: FnMut(&BenchmarkWorkflow) -> Output,
    {
        for _ in 0..self.warmup_iterations {
            black_box(operation(black_box(workflow)));
        }

        let started_at = Instant::now();

        for _ in 0..self.iterations {
            black_box(operation(black_box(workflow)));
        }

        let elapsed = started_at.elapsed();
        let iteration_count = u32::try_from(self.iterations).expect("iteration count should fit into u32");
        let average_duration = elapsed / iteration_count;

        println!(
            "{:<24} {:<8} iterations={:<5} total={:>10.3?} average={:>10.3?}",
            benchmark_stage.as_str(),
            workflow.scale.as_str(),
            self.iterations,
            elapsed,
            average_duration,
        );
    }
}

#[derive(Debug)]
struct BenchmarkWorkflow {
    scale: WorkflowScale,
    source: &'static str,
    input: Value,
    secrets: Value,
    model_outputs: HashMap<String, Value>,
    workflow: Workflow,
    execution_plan: ExecutionPlan,
    executor: WorkflowExecutor,
}

impl BenchmarkWorkflow {
    fn small() -> Self {
        Self::new(
            WorkflowScale::Small,
            SMALL_WORKFLOW_SOURCE,
            Value::Null,
            Value::Null,
            HashMap::from([("greeting".to_string(), json!({ "value": "welcome" }))]),
        )
    }

    fn medium() -> Self {
        Self::new(
            WorkflowScale::Medium,
            MEDIUM_WORKFLOW_SOURCE,
            json!({ "topic": "runtime performance" }),
            Value::Null,
            HashMap::from([
                ("branch_a".to_string(), json!({ "value": "analysis a" })),
                ("branch_b".to_string(), json!({ "value": "analysis b" })),
                ("merger".to_string(), json!({ "value": "merged analysis" })),
            ]),
        )
    }

    fn large() -> Self {
        let lead_participant = json!({ "name": "Ada", "role": "lead" });
        let participant_list = json!([
            lead_participant.clone(),
            { "name": "Grace", "role": "reviewer" },
        ]);
        let previous_summary = json!({
            "title": "Prior research",
            "lead": lead_participant.clone(),
            "participants": participant_list.clone(),
        });
        let typed_output = json!({
            "string_value": "alpha",
            "number_value": 42,
            "float_value": 4.2,
            "boolean_value": true,
            "nullable_value": null,
            "nullable_string": "optional",
            "nullable_number": 7,
            "array": ["one", "two"],
            "fixed_array": ["one", "two", "three"],
            "array_of_objects": [
                { "id": "item-1", "score": 10 },
                { "id": "item-2", "score": 11 },
            ],
            "enum_value": "ready",
            "nullable_enum": null,
            "tuple_value": ["tuple", 5, ["red", "green", "blue"]],
            "nullable_tuple": null,
            "object_value": {
                "string_value": "nested",
                "number_value": 3,
            },
            "nullable_object": {
                "string_value": "nullable nested",
                "number_value": 9,
            },
            "lead": lead_participant.clone(),
            "participants": participant_list.clone(),
            "summary": {
                "title": "Current research",
                "lead": lead_participant.clone(),
                "participants": participant_list.clone(),
            },
            "event": {
                "type": "created",
                "id": "event-1",
                "actor": lead_participant,
            },
            "nullable_event": {
                "type": "deleted",
                "id": "event-2",
                "reason": "archived",
            },
        });

        Self::new(
            WorkflowScale::Large,
            LARGE_WORKFLOW_SOURCE,
            json!({
                "lead_participant": previous_summary["lead"].clone(),
                "participant_list": participant_list,
                "previous_summary": previous_summary,
            }),
            Value::Null,
            HashMap::from([("typed_example".to_string(), json!({ "value": typed_output }))]),
        )
    }

    fn new(scale: WorkflowScale, source: &'static str, input: Value, secrets: Value, model_outputs: HashMap<String, Value>) -> Self {
        let workflow = parse_workflow(source).expect("benchmark workflow should parse");
        let validation_report = validate_workflow(&workflow);

        assert!(
            !validation_report.has_issues(),
            "benchmark workflow should validate:\n{}",
            validation_report.render_for_output_target(Some(source), "<benchmark workflow>")
        );

        let execution_plan = Self::build_plan(&workflow);
        let executor =
            WorkflowExecutor::from_source_with_runtime_values(source, &input, &secrets).expect("benchmark workflow executor should build");

        Self {
            scale,
            source,
            input,
            secrets,
            model_outputs,
            workflow,
            execution_plan,
            executor,
        }
    }

    fn parse(&self) -> Workflow {
        parse_workflow(black_box(self.source)).expect("benchmark workflow should parse")
    }

    fn validate(&self) -> usize {
        let validation_report = validate_workflow(black_box(&self.workflow));

        assert!(
            !validation_report.has_issues(),
            "benchmark workflow should validate:\n{}",
            validation_report.render_for_output_target(Some(self.source), "<benchmark workflow>")
        );

        validation_report.issues().len()
    }

    fn plan(&self) -> ExecutionPlan {
        Self::build_plan(black_box(&self.workflow))
    }

    fn render_prompts(&self) -> Vec<String> {
        let evaluation_context = self.evaluation_context();
        let mut prompts = Vec::new();

        for agent_name in &self.execution_plan.agent_execution_order {
            let planned_agent = self
                .execution_plan
                .planned_agents
                .get(agent_name)
                .expect("planned agent should exist for execution order entry");
            let instruction_expression = planned_agent
                .declaration
                .required_expression_property(AgentExpressionPropertyName::Instruction)
                .expect("planned agent should have instruction expression");
            let prompt_value = evaluate_expression(instruction_expression, &evaluation_context, "benchmark prompt rendering")
                .expect("benchmark prompt should render");

            prompts.push(Self::normalize_prompt(prompt_value));
        }

        prompts
    }

    fn resolve_schemas(&self) -> Vec<Value> {
        let mut schemas = Vec::new();
        let mut schema_cache = WorkflowSchemaCache::new();

        for agent_name in &self.execution_plan.agent_execution_order {
            let planned_agent = self
                .execution_plan
                .planned_agents
                .get(agent_name)
                .expect("planned agent should exist for execution order entry");

            schemas.push(planned_agent.iteration_output_schema_with_cache(&mut schema_cache));
            schemas.push(planned_agent.final_output_type.json_schema_value_with_cache(&mut schema_cache));
        }

        schemas.push(
            self.execution_plan
                .workflow_output_type
                .json_schema_value_with_cache(&mut schema_cache),
        );

        for typed_tool in self.execution_plan.tools.values() {
            schemas.push(typed_tool.input_type.json_schema_value_with_cache(&mut schema_cache));
            schemas.push(typed_tool.output_type.json_schema_value_with_cache(&mut schema_cache));
        }

        schemas
    }

    fn execute_with_fake_provider(&self, runtime: &Runtime) -> Value {
        let provider = FakeBenchmarkProvider::new(self.model_outputs.clone());

        runtime
            .block_on(self.executor.execute(self.input.clone(), self.secrets.clone(), &provider, None, 8))
            .expect("benchmark workflow should execute")
    }

    fn build_plan(workflow: &Workflow) -> ExecutionPlan {
        let typed_workflow_ir = build_dynamic_typed_workflow_ir(workflow).expect("benchmark workflow should typecheck");

        build_execution_plan(workflow, &typed_workflow_ir).expect("benchmark workflow should plan")
    }

    fn evaluation_context(&self) -> EvaluationContext {
        EvaluationContext {
            input_values: self.input.as_object().cloned().unwrap_or_default(),
            secret_values: self.secrets.as_object().cloned().unwrap_or_default(),
            agent_outputs: self.model_outputs.clone(),
            agent_contexts: HashMap::new(),
            local_bindings: HashMap::new(),
        }
    }

    fn normalize_prompt(prompt_value: Value) -> String {
        if let Some(prompt) = prompt_value.as_str() {
            return prompt.to_string();
        }

        serde_json::to_string(&prompt_value).unwrap_or_else(|_| prompt_value.to_string())
    }
}

#[derive(Debug)]
struct FakeBenchmarkProvider {
    model_outputs: HashMap<String, Value>,
}

impl FakeBenchmarkProvider {
    fn new(model_outputs: HashMap<String, Value>) -> Self {
        Self { model_outputs }
    }
}

#[async_trait]
impl ModelProvider for FakeBenchmarkProvider {
    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, ExecutorError> {
        let output = self
            .model_outputs
            .get(&request.agent_name)
            .cloned()
            .ok_or_else(|| ExecutorError::Model {
                agent_name: request.agent_name.clone(),
                message: "benchmark fake provider has no output for agent".to_string(),
            })?;

        Ok(ModelResponse {
            output,
            context: json!({ "agent": request.agent_name }),
        })
    }
}

fn read_iteration_count(environment_variable_name: &str, default_count: usize) -> usize {
    std::env::var(environment_variable_name)
        .ok()
        .and_then(|environment_variable_value| environment_variable_value.parse::<usize>().ok())
        .filter(|iteration_count| *iteration_count > 0)
        .unwrap_or(default_count)
}

fn build_runtime() -> Runtime {
    Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build")
}

fn workflow_cases() -> Vec<BenchmarkWorkflow> {
    vec![BenchmarkWorkflow::small(), BenchmarkWorkflow::medium(), BenchmarkWorkflow::large()]
}

fn benchmark_stages() -> Vec<BenchmarkStage> {
    vec![
        BenchmarkStage::Parsing,
        BenchmarkStage::Validation,
        BenchmarkStage::Planning,
        BenchmarkStage::PromptRendering,
        BenchmarkStage::SchemaResolution,
        BenchmarkStage::FakeProviderExecution,
    ]
}

fn main() {
    let runner = BenchmarkRunner::from_environment();
    let runtime = build_runtime();
    let workflows = workflow_cases();

    println!(
        "superwire runtime benchmarks; set {ITERATIONS_ENVIRONMENT_VARIABLE} and {WARMUP_ITERATIONS_ENVIRONMENT_VARIABLE} to tune run length",
    );

    for workflow in &workflows {
        for benchmark_stage in benchmark_stages() {
            match benchmark_stage {
                BenchmarkStage::Parsing => runner.run_stage(workflow, benchmark_stage, BenchmarkWorkflow::parse),
                BenchmarkStage::Validation => runner.run_stage(workflow, benchmark_stage, BenchmarkWorkflow::validate),
                BenchmarkStage::Planning => runner.run_stage(workflow, benchmark_stage, BenchmarkWorkflow::plan),
                BenchmarkStage::PromptRendering => {
                    runner.run_stage(workflow, benchmark_stage, BenchmarkWorkflow::render_prompts);
                }
                BenchmarkStage::SchemaResolution => {
                    runner.run_stage(workflow, benchmark_stage, BenchmarkWorkflow::resolve_schemas);
                }
                BenchmarkStage::FakeProviderExecution => {
                    runner.run_stage(workflow, benchmark_stage, |benchmark_workflow| {
                        benchmark_workflow.execute_with_fake_provider(&runtime)
                    });
                }
            }
        }
    }
}
