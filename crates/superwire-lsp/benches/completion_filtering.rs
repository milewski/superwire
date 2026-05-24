use lsp_types::Position;
use std::fmt::Write;
use std::hint::black_box;
use std::time::Instant;
use superwire_lsp::document::DocumentState;

const CURSOR_MARKER: &str = "__SUPERWIRE_CURSOR__";
const DEFAULT_CANDIDATE_COUNT: usize = 1_500;
const DEFAULT_ITERATIONS: usize = 200;
const DEFAULT_WARMUP_ITERATIONS: usize = 10;
const CANDIDATE_COUNT_ENVIRONMENT_VARIABLE: &str = "SUPERWIRE_COMPLETION_BENCH_CANDIDATES";
const ITERATIONS_ENVIRONMENT_VARIABLE: &str = "SUPERWIRE_COMPLETION_BENCH_ITERATIONS";
const WARMUP_ITERATIONS_ENVIRONMENT_VARIABLE: &str = "SUPERWIRE_COMPLETION_BENCH_WARMUP_ITERATIONS";

#[derive(Debug, Clone, Copy)]
enum CompletionBenchmarkCase {
    ModelProfiles,
    ToolReferences,
    SchemaReferences,
}

impl CompletionBenchmarkCase {
    fn as_str(self) -> &'static str {
        match self {
            Self::ModelProfiles => "model_profiles",
            Self::ToolReferences => "tool_references",
            Self::SchemaReferences => "schema_references",
        }
    }

    fn completion_source(self, candidate_count: usize) -> String {
        let mut benchmark_source_builder = BenchmarkSourceBuilder::new(candidate_count);

        benchmark_source_builder.push_provider();
        benchmark_source_builder.push_models();
        benchmark_source_builder.push_schemas();
        benchmark_source_builder.push_tools();
        benchmark_source_builder.push_completion_target(self);
        benchmark_source_builder.finish()
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
            iterations: Self::read_count(ITERATIONS_ENVIRONMENT_VARIABLE, DEFAULT_ITERATIONS),
            warmup_iterations: Self::read_count(WARMUP_ITERATIONS_ENVIRONMENT_VARIABLE, DEFAULT_WARMUP_ITERATIONS),
        }
    }

    fn run_case(&self, benchmark_document: &BenchmarkDocument) {
        for _ in 0..self.warmup_iterations {
            black_box(benchmark_document.completion_count());
        }

        let started_at = Instant::now();

        for _ in 0..self.iterations {
            black_box(benchmark_document.completion_count());
        }

        let elapsed = started_at.elapsed();
        let iteration_count = u32::try_from(self.iterations).expect("iteration count should fit into u32");
        let average_duration = elapsed / iteration_count;

        println!(
            "{:<20} candidates={:<6} suggestions={:<6} iterations={:<5} total={:>10.3?} average={:>10.3?}",
            benchmark_document.benchmark_case.as_str(),
            benchmark_document.candidate_count,
            benchmark_document.expected_suggestion_count,
            self.iterations,
            elapsed,
            average_duration,
        );
    }

    fn read_count(environment_variable_name: &str, default_count: usize) -> usize {
        std::env::var(environment_variable_name)
            .ok()
            .and_then(|environment_variable_value| environment_variable_value.parse::<usize>().ok())
            .filter(|iteration_count| *iteration_count > 0)
            .unwrap_or(default_count)
    }
}

#[derive(Debug)]
struct BenchmarkDocument {
    benchmark_case: CompletionBenchmarkCase,
    candidate_count: usize,
    cursor_position: Position,
    document_state: DocumentState,
    expected_suggestion_count: usize,
}

impl BenchmarkDocument {
    fn new(benchmark_case: CompletionBenchmarkCase, candidate_count: usize) -> Self {
        let source_with_cursor = benchmark_case.completion_source(candidate_count);
        let (source, cursor_position) = SourceWithCursor::new(source_with_cursor).split();
        let document_state = DocumentState::new(source, None);
        let expected_suggestion_count = document_state.completion_suggestions(cursor_position).len();

        assert_eq!(
            expected_suggestion_count, candidate_count,
            "benchmark source should expose exactly the generated candidates for {benchmark_case:?}"
        );

        Self {
            benchmark_case,
            candidate_count,
            cursor_position,
            document_state,
            expected_suggestion_count,
        }
    }

    fn completion_count(&self) -> usize {
        self.document_state.completion_suggestions(black_box(self.cursor_position)).len()
    }
}

#[derive(Debug)]
struct BenchmarkSourceBuilder {
    candidate_count: usize,
    source: String,
}

impl BenchmarkSourceBuilder {
    fn new(candidate_count: usize) -> Self {
        Self {
            candidate_count,
            source: String::new(),
        }
    }

    fn push_provider(&mut self) {
        self.source.push_str("provider benchmark from openai {}\n\n");
    }

    fn push_models(&mut self) {
        for candidate_index in 0..self.candidate_count {
            write!(
                self.source,
                "model model_{candidate_index:04} from benchmark {{\n    id: \"benchmark-{candidate_index:04}\"\n}}\n\n"
            )
            .expect("writing benchmark source should not fail");
        }
    }

    fn push_schemas(&mut self) {
        for candidate_index in 0..self.candidate_count {
            write!(self.source, "schema schema_{candidate_index:04} {{\n    value: string\n}}\n\n")
                .expect("writing benchmark source should not fail");
        }
    }

    fn push_tools(&mut self) {
        for candidate_index in 0..self.candidate_count {
            write!(
                self.source,
                "tool tool_{candidate_index:04} {{\n    input {{\n        query: string\n    }}\n\n    output {{\n        value: string\n    }}\n}}\n\n"
            )
            .expect("writing benchmark source should not fail");
        }
    }

    fn push_completion_target(&mut self, benchmark_case: CompletionBenchmarkCase) {
        match benchmark_case {
            CompletionBenchmarkCase::ModelProfiles => {
                write!(
                    self.source,
                    "agent completion_target {{\n    model: model.model_{CURSOR_MARKER}\n    instruction: \"benchmark\"\n    output {{\n        value: string\n    }}\n}}\n"
                )
                .expect("writing benchmark source should not fail");
            }
            CompletionBenchmarkCase::ToolReferences => {
                write!(
                    self.source,
                    "agent completion_target {{\n    uses: [tool.tool_{CURSOR_MARKER}]\n    instruction: \"benchmark\"\n    output {{\n        value: string\n    }}\n}}\n"
                )
                .expect("writing benchmark source should not fail");
            }
            CompletionBenchmarkCase::SchemaReferences => {
                write!(
                    self.source,
                    "schema completion_target {{\n    value: schema.schema_{CURSOR_MARKER}\n}}\n"
                )
                .expect("writing benchmark source should not fail");
            }
        }
    }

    fn finish(self) -> String {
        self.source
    }
}

#[derive(Debug)]
struct SourceWithCursor {
    source: String,
}

impl SourceWithCursor {
    fn new(source: String) -> Self {
        Self { source }
    }

    fn split(self) -> (String, Position) {
        let cursor_byte_offset = self
            .source
            .find(CURSOR_MARKER)
            .expect("benchmark source should include a cursor marker");
        let source_before_cursor = &self.source[..cursor_byte_offset];
        let line_number = source_before_cursor
            .chars()
            .filter(|source_character| *source_character == '\n')
            .count();
        let line_start_byte_offset = source_before_cursor
            .rfind('\n')
            .map_or(0, |newline_byte_offset| newline_byte_offset + 1);
        let character_number = source_before_cursor[line_start_byte_offset..].chars().count();
        let source = self.source.replace(CURSOR_MARKER, "");

        (
            source,
            Position {
                line: Self::usize_to_u32(line_number),
                character: Self::usize_to_u32(character_number),
            },
        )
    }

    fn usize_to_u32(value: usize) -> u32 {
        u32::try_from(value).expect("benchmark source position should fit into u32")
    }
}

fn benchmark_cases() -> Vec<CompletionBenchmarkCase> {
    vec![
        CompletionBenchmarkCase::ModelProfiles,
        CompletionBenchmarkCase::ToolReferences,
        CompletionBenchmarkCase::SchemaReferences,
    ]
}

fn main() {
    let runner = BenchmarkRunner::from_environment();
    let candidate_count = BenchmarkRunner::read_count(CANDIDATE_COUNT_ENVIRONMENT_VARIABLE, DEFAULT_CANDIDATE_COUNT);

    println!(
        "superwire lsp completion filtering benchmarks; set {CANDIDATE_COUNT_ENVIRONMENT_VARIABLE}, {ITERATIONS_ENVIRONMENT_VARIABLE}, and {WARMUP_ITERATIONS_ENVIRONMENT_VARIABLE} to tune run length",
    );

    for benchmark_case in benchmark_cases() {
        let benchmark_document = BenchmarkDocument::new(benchmark_case, candidate_count);

        runner.run_case(&benchmark_document);
    }
}
