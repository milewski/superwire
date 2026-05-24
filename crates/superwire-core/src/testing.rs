use crate::diagnostic::DiagnosticCode;
use crate::dsl::{
    format_workflow_source, parse_workflow, DeclarationKeyword, DslFormatError, DslParseError, Reference, ReferenceAccess,
    ReferenceKeyword, ReferenceRoot, Workflow,
};
use crate::semantic::support::types::WorkflowType;
use crate::semantic::{
    ReferenceResolution, ReferenceResolutionError, ReferenceResolutionRoot, ReferenceResolutionScope, WorkflowExecutionGraph,
    WorkflowSemanticIndex,
};
use serde_json::Value;
use std::collections::{BTreeMap, VecDeque};
use std::fmt::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use superwire_mcp::{
    McpClientBackend, McpClientFactory, McpError, McpLock, McpPromptArgumentLock, McpServerConfig, McpServerLock, McpToolLock,
    ProjectMcpLock, PROJECT_MCP_LOCK_FILE_NAME,
};

pub const COMPACT_CURSOR_MARKER: &str = "<cursor>";
pub const SPACED_CURSOR_MARKER: &str = "< cursor >";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InlineCursorPosition {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowSourceTemplate {
    source_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowSourceWithCursor {
    source_text: String,
    cursor_position: InlineCursorPosition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowSource {
    Inline(String),
    File(PathBuf),
}

#[derive(Debug)]
pub enum WorkflowSourceReadError {
    Io { path: PathBuf, error: std::io::Error },
    Format(DslFormatError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedDiagnostic {
    pub code: DiagnosticCode,
    pub message_contains: Option<String>,
    pub span_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExpectedOutput {
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedProviderRequest {
    pub provider: String,
    pub model: Option<String>,
    pub prompt_contains: Option<String>,
    pub tools: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExpectedMcpRequest {
    pub server: String,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedEventKind {
    WorkflowStarted,
    WorkflowPlanned,
    WorkflowCompleted,
    WorkflowFailed,
    AgentStarted,
    AgentCompleted,
    ToolCallStarted,
    ToolCallCompleted,
    ToolCallFailed,
    McpCallStarted,
    McpCallCompleted,
    McpCallFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedEvent {
    pub kind: ExpectedEventKind,
    pub agent_name: Option<String>,
    pub tool_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedCompletion {
    pub label: String,
    pub kind: Option<ExpectedCompletionKind>,
    pub detail_contains: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedCompletionKind {
    Keyword,
    Function,
    Field,
    Variable,
    Value,
    Module,
    Struct,
    Enum,
    Property,
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotAssertion {
    pub name: String,
    pub expected: String,
    pub actual: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatterSnapshotAssertion {
    snapshot: SnapshotAssertion,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphJsonSnapshotAssertion {
    snapshot: SnapshotAssertion,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticIndexSnapshotAssertion {
    snapshot: SnapshotAssertion,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockFileSnapshotAssertion {
    snapshot: SnapshotAssertion,
}

#[derive(Debug, Clone)]
pub struct SemanticFixture {
    workflow: Workflow,
    semantic_index: WorkflowSemanticIndex,
    resolution_scope: ReferenceResolutionScope,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticTypeRoot {
    Input,
    Secrets,
    Schema(String),
    AgentOutput(String),
    ToolInput(String),
    ToolBinding(String),
    ToolOutput(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticCompletionSource {
    ReferenceRoots,
    Providers,
    Models,
    McpServers,
    Schemas,
    Agents,
    Tools,
    Resources,
    Prompts,
    Fields(SemanticTypeRoot),
}

#[derive(Debug, Clone, Default)]
pub struct FakeMcpClientFactory {
    servers_by_name: Arc<Mutex<BTreeMap<String, FakeMcpServer>>>,
}

#[derive(Debug, Clone, Default)]
pub struct FakeMcpServerBuilder {
    tools: BTreeMap<String, FakeMcpTool>,
    resources: BTreeMap<String, FakeMcpResource>,
    prompts: BTreeMap<String, FakeMcpPrompt>,
}

#[derive(Debug, Clone)]
pub struct FakeMcpToolBuilder {
    description: Option<String>,
    input_schema: Value,
    output_schema: Option<Value>,
    responses: VecDeque<Value>,
}

#[derive(Debug, Clone, Default)]
pub struct FakeMcpResourceBuilder {
    uri: Option<String>,
    mime_type: Option<String>,
    text: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct FakeMcpPromptBuilder {
    text: Option<String>,
    arguments: Vec<McpPromptArgumentLock>,
    responses: VecDeque<Value>,
}

#[derive(Debug, Clone)]
struct FakeMcpServer {
    name: String,
    tools: BTreeMap<String, FakeMcpTool>,
    resources: BTreeMap<String, FakeMcpResource>,
    prompts: BTreeMap<String, FakeMcpPrompt>,
    requests: Arc<Mutex<Vec<FakeMcpRequest>>>,
}

#[derive(Debug, Clone)]
struct FakeMcpTool {
    description: Option<String>,
    input_schema: Value,
    output_schema: Option<Value>,
    responses: Arc<Mutex<VecDeque<Value>>>,
}

#[derive(Debug, Clone)]
struct FakeMcpResource {
    uri: String,
    mime_type: String,
    text: String,
}

#[derive(Debug, Clone)]
struct FakeMcpPrompt {
    arguments: Vec<McpPromptArgumentLock>,
    responses: Arc<Mutex<VecDeque<Value>>>,
    default_response: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FakeMcpRequest {
    pub server_name: String,
    pub method: String,
    pub name: Option<String>,
    pub arguments: Value,
}

#[derive(Debug)]
struct FakeMcpClient {
    server: FakeMcpServer,
}

impl WorkflowSourceTemplate {
    #[must_use]
    pub fn from_inline(source_text: impl Into<String>) -> Self {
        Self {
            source_text: normalize_rust_doc_comment_tokens(&source_text.into()),
        }
    }

    #[must_use]
    pub fn source(&self) -> &str {
        &self.source_text
    }

    #[must_use]
    pub fn into_source(self) -> String {
        self.source_text
    }

    pub fn parse_workflow(&self) -> Result<Workflow, DslParseError> {
        parse_workflow(&self.source_text)
    }

    #[must_use]
    pub fn normalized_cursor_layout(&self) -> Self {
        Self {
            source_text: normalize_inline_cursor_layout(&self.source_text),
        }
    }

    #[must_use]
    pub fn without_cursor_normalization(&self) -> WorkflowSourceWithCursor {
        source_without_cursor_normalization(&self.source_text)
    }

    #[must_use]
    pub fn with_cursor(&self) -> WorkflowSourceWithCursor {
        self.normalized_cursor_layout().without_cursor_normalization()
    }
}

impl WorkflowSourceWithCursor {
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source_text
    }

    #[must_use]
    pub fn into_source(self) -> String {
        self.source_text
    }

    #[must_use]
    pub fn cursor_position(&self) -> InlineCursorPosition {
        self.cursor_position
    }
}

impl WorkflowSource {
    #[must_use]
    pub fn inline(source_text: impl Into<String>) -> Self {
        Self::Inline(source_text.into())
    }

    #[must_use]
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self::File(path.into())
    }

    #[must_use]
    pub fn fixture(root: impl AsRef<Path>, relative_path: impl AsRef<Path>) -> Self {
        Self::File(root.as_ref().join(relative_path))
    }

    #[must_use]
    pub fn fixture_or_inline(fixture_root: impl AsRef<Path>, source_text: impl Into<String>) -> Self {
        let source_text = source_text.into();

        if Self::looks_like_inline_source(&source_text) {
            return Self::inline(source_text);
        }

        let source_path = PathBuf::from(&source_text);

        if source_path.exists() {
            return Self::file(source_path);
        }

        Self::fixture(fixture_root, source_text)
    }

    pub fn read(&self) -> Result<String, WorkflowSourceReadError> {
        match self {
            Self::Inline(source_text) => Ok(source_text.clone()),
            Self::File(path) => std::fs::read_to_string(path).map_err(|error| WorkflowSourceReadError::Io { path: path.clone(), error }),
        }
    }

    pub fn read_formatted(&self) -> Result<String, WorkflowSourceReadError> {
        let source_text = self.read()?;
        format_workflow_source(&source_text).map_err(WorkflowSourceReadError::Format)
    }

    pub fn read_formatted_or_original(&self) -> Result<String, WorkflowSourceReadError> {
        let source_text = self.read()?;
        Ok(format_workflow_source(&source_text).unwrap_or(source_text))
    }

    fn looks_like_inline_source(source_text: &str) -> bool {
        source_text.contains('\n')
            || source_text.trim_start().starts_with(DeclarationKeyword::Provider.as_str())
            || source_text.trim_start().starts_with(DeclarationKeyword::Mcp.as_str())
    }
}

impl fmt::Display for WorkflowSourceReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, error } => write!(formatter, "failed to read workflow fixture {}: {error}", path.display()),
            Self::Format(format_error) => write!(formatter, "{format_error}"),
        }
    }
}

impl std::error::Error for WorkflowSourceReadError {}

impl ExpectedDiagnostic {
    #[must_use]
    pub fn code(code: DiagnosticCode) -> Self {
        Self {
            code,
            message_contains: None,
            span_text: None,
        }
    }

    #[must_use]
    pub fn message_contains(mut self, message_contains: impl Into<String>) -> Self {
        self.message_contains = Some(message_contains.into());
        self
    }

    #[must_use]
    pub fn span_text(mut self, span_text: impl Into<String>) -> Self {
        self.span_text = Some(span_text.into());
        self
    }
}

impl ExpectedOutput {
    #[must_use]
    pub fn new(value: Value) -> Self {
        Self { value }
    }
}

impl ExpectedProviderRequest {
    #[must_use]
    pub fn new(provider: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: None,
            prompt_contains: None,
            tools: Vec::new(),
        }
    }

    #[must_use]
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    #[must_use]
    pub fn prompt_contains(mut self, prompt_contains: impl Into<String>) -> Self {
        self.prompt_contains = Some(prompt_contains.into());
        self
    }

    #[must_use]
    pub fn tools(mut self, tools: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.tools = tools.into_iter().map(Into::into).collect();
        self
    }
}

impl ExpectedMcpRequest {
    #[must_use]
    pub fn new(server: impl Into<String>, method: impl Into<String>) -> Self {
        Self {
            server: server.into(),
            method: method.into(),
            params: None,
        }
    }

    #[must_use]
    pub fn params(mut self, params: Value) -> Self {
        self.params = Some(params);
        self
    }
}

impl ExpectedEvent {
    #[must_use]
    pub fn new(kind: ExpectedEventKind) -> Self {
        Self {
            kind,
            agent_name: None,
            tool_name: None,
        }
    }

    #[must_use]
    pub fn agent_name(mut self, agent_name: impl Into<String>) -> Self {
        self.agent_name = Some(agent_name.into());
        self
    }

    #[must_use]
    pub fn tool_name(mut self, tool_name: impl Into<String>) -> Self {
        self.tool_name = Some(tool_name.into());
        self
    }
}

impl ExpectedCompletion {
    #[must_use]
    pub fn label(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            kind: None,
            detail_contains: None,
        }
    }

    #[must_use]
    pub fn kind(mut self, kind: ExpectedCompletionKind) -> Self {
        self.kind = Some(kind);
        self
    }

    #[must_use]
    pub fn detail_contains(mut self, detail_contains: impl Into<String>) -> Self {
        self.detail_contains = Some(detail_contains.into());
        self
    }
}

impl SnapshotAssertion {
    #[must_use]
    pub fn new(name: impl Into<String>, expected: impl Into<String>, actual: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            expected: expected.into(),
            actual: actual.into(),
        }
    }

    pub fn assert_matches(&self) {
        assert!(
            self.expected == self.actual,
            "snapshot `{}` did not match\n{}",
            self.name,
            stable_text_diff(&self.expected, &self.actual)
        );
    }
}

impl FormatterSnapshotAssertion {
    pub fn from_source(name: impl Into<String>, expected: impl Into<String>, source_text: &str) -> Result<Self, DslFormatError> {
        let formatted_source = format_workflow_source(source_text)?;

        Ok(Self::from_formatted_output(name, expected, formatted_source))
    }

    pub fn from_source_template(
        name: impl Into<String>,
        expected: impl Into<String>,
        source_template: &WorkflowSourceTemplate,
    ) -> Result<Self, DslFormatError> {
        Self::from_source(name, expected, source_template.source())
    }

    #[must_use]
    pub fn from_formatted_output(name: impl Into<String>, expected: impl Into<String>, formatted_output: impl Into<String>) -> Self {
        Self {
            snapshot: SnapshotAssertion::new(name, expected, formatted_output),
        }
    }

    pub fn assert_matches(&self) {
        self.snapshot.assert_matches();
    }
}

impl GraphJsonSnapshotAssertion {
    #[must_use]
    pub fn from_graph(name: impl Into<String>, expected: impl Into<String>, graph: &WorkflowExecutionGraph) -> Self {
        Self {
            snapshot: SnapshotAssertion::new(name, expected, graph.stable_json()),
        }
    }

    pub fn assert_matches(&self) {
        self.snapshot.assert_matches();
    }
}

impl SemanticIndexSnapshotAssertion {
    #[must_use]
    pub fn from_index(name: impl Into<String>, expected: impl Into<String>, semantic_index: &WorkflowSemanticIndex) -> Self {
        Self {
            snapshot: SnapshotAssertion::new(name, expected, semantic_index.stable_summary()),
        }
    }

    pub fn assert_matches(&self) {
        self.snapshot.assert_matches();
    }
}

impl LockFileSnapshotAssertion {
    #[must_use]
    pub fn from_workflow_lock(name: impl Into<String>, expected: impl Into<String>, lock: &McpLock) -> Self {
        let snapshot_path = Path::new(PROJECT_MCP_LOCK_FILE_NAME);
        let lock_text = lock.file_text(snapshot_path).expect("workflow MCP lock should serialize");

        Self {
            snapshot: SnapshotAssertion::new(name, expected, lock_text),
        }
    }

    #[must_use]
    pub fn from_project_lock(name: impl Into<String>, expected: impl Into<String>, lock: &ProjectMcpLock) -> Self {
        let snapshot_path = Path::new(PROJECT_MCP_LOCK_FILE_NAME);
        let lock_text = lock.file_text(snapshot_path).expect("project MCP lock should serialize");

        Self {
            snapshot: SnapshotAssertion::new(name, expected, lock_text),
        }
    }

    pub fn assert_matches(&self) {
        self.snapshot.assert_matches();
    }
}

impl SemanticFixture {
    #[must_use]
    pub fn from_source_template(source_template: WorkflowSourceTemplate) -> Self {
        let workflow = source_template.parse_workflow().unwrap_or_else(|parse_error| {
            panic!(
                "semantic fixture workflow failed to parse:\n{}",
                parse_error.render_with_source(source_template.source(), "<semantic fixture>")
            )
        });

        Self::from_workflow(workflow)
    }

    #[must_use]
    pub fn from_workflow(workflow: Workflow) -> Self {
        let semantic_index = WorkflowSemanticIndex::from_workflow(&workflow);

        Self {
            workflow,
            semantic_index,
            resolution_scope: ReferenceResolutionScope::new(),
        }
    }

    #[must_use]
    pub fn workflow(&self) -> &Workflow {
        &self.workflow
    }

    #[must_use]
    pub fn semantic_index(&self) -> &WorkflowSemanticIndex {
        &self.semantic_index
    }

    #[must_use]
    pub fn with_resolution_scope(mut self, resolution_scope: ReferenceResolutionScope) -> Self {
        self.resolution_scope = resolution_scope;

        self
    }

    #[must_use]
    pub fn with_dynamic_field_type(mut self, field_name: impl Into<String>, field_type: WorkflowType) -> Self {
        self.resolution_scope = self.resolution_scope.with_dynamic_field_type(field_name, field_type);

        self
    }

    #[must_use]
    pub fn with_local_binding_type(mut self, binding_name: impl Into<String>, binding_type: WorkflowType) -> Self {
        self.resolution_scope = self.resolution_scope.with_local_binding_type(binding_name, binding_type);

        self
    }

    #[must_use]
    pub fn keyword_reference(reference_keyword: ReferenceKeyword, access_fields: impl IntoIterator<Item = impl Into<String>>) -> Reference {
        Reference {
            root: ReferenceRoot::Keyword(reference_keyword),
            accesses: Self::reference_accesses(access_fields),
            span: crate::dsl::SourceSpan::generated(),
        }
    }

    #[must_use]
    pub fn local_reference(binding_name: impl Into<String>, access_fields: impl IntoIterator<Item = impl Into<String>>) -> Reference {
        Reference {
            root: ReferenceRoot::Identifier(binding_name.into()),
            accesses: Self::reference_accesses(access_fields),
            span: crate::dsl::SourceSpan::generated(),
        }
    }

    pub fn resolve_reference(&self, reference: &Reference) -> Result<ReferenceResolution<'_>, ReferenceResolutionError> {
        self.semantic_index.reference_resolver().resolve(reference, &self.resolution_scope)
    }

    pub fn assert_has_declaration(&self, declaration_source: SemanticCompletionSource, expected_name: impl AsRef<str>) {
        let expected_name = expected_name.as_ref();
        let completion_labels = declaration_source.completion_labels(&self.semantic_index);

        assert!(
            completion_labels.iter().any(|completion_label| completion_label == expected_name),
            "expected semantic labels to contain `{expected_name}`; got {completion_labels:?}"
        );
    }

    pub fn assert_type(&self, type_root: SemanticTypeRoot, expected_type: WorkflowType) {
        let actual_type = type_root
            .workflow_type(&self.semantic_index)
            .unwrap_or_else(|| panic!("expected semantic type root `{type_root:?}` to exist"));

        assert_eq!(actual_type, expected_type);
    }

    pub fn assert_field_type(&self, type_root: SemanticTypeRoot, field_path: &[&str], expected_type: WorkflowType) {
        let root_type = type_root
            .workflow_type(&self.semantic_index)
            .unwrap_or_else(|| panic!("expected semantic type root `{type_root:?}` to exist"));
        let actual_type = root_type
            .field_type_at_path(field_path)
            .unwrap_or_else(|| panic!("expected field path `{field_path:?}` to exist in `{type_root:?}`"));

        assert_eq!(actual_type, expected_type);
    }

    pub fn assert_reference_resolves_to(
        &self,
        reference: &Reference,
        expected_root: ReferenceResolutionRoot,
        expected_name: impl AsRef<str>,
    ) {
        let resolution = self
            .resolve_reference(reference)
            .unwrap_or_else(|resolution_error| panic!("expected `{}` to resolve: {resolution_error:?}", reference.render_path()));

        assert_eq!(resolution.root(), expected_root);
        assert_eq!(resolution.name(), Some(expected_name.as_ref()));
    }

    pub fn assert_reference_type(&self, reference: &Reference, expected_type: WorkflowType) {
        let resolution = self
            .resolve_reference(reference)
            .unwrap_or_else(|resolution_error| panic!("expected `{}` to resolve: {resolution_error:?}", reference.render_path()));
        let actual_type = resolution
            .resolved_type()
            .unwrap_or_else(|| panic!("expected `{}` to resolve to a value type", reference.render_path()));

        assert_eq!(actual_type, &expected_type);
    }

    pub fn assert_reference_error(&self, reference: &Reference, expected_error: ReferenceResolutionError) {
        let actual_error = self
            .resolve_reference(reference)
            .expect_err("expected reference resolution to fail");

        assert_eq!(actual_error, expected_error);
    }

    pub fn assert_completion_contains(
        &self,
        completion_source: SemanticCompletionSource,
        expected_labels: impl IntoIterator<Item = impl AsRef<str>>,
    ) {
        let completion_labels = completion_source.completion_labels(&self.semantic_index);

        for expected_label in expected_labels {
            let expected_label = expected_label.as_ref();

            assert!(
                completion_labels.iter().any(|completion_label| completion_label == expected_label),
                "expected semantic completions to contain `{expected_label}`; got {completion_labels:?}"
            );
        }
    }

    pub fn assert_completion_excludes(
        &self,
        completion_source: SemanticCompletionSource,
        unexpected_labels: impl IntoIterator<Item = impl AsRef<str>>,
    ) {
        let completion_labels = completion_source.completion_labels(&self.semantic_index);

        for unexpected_label in unexpected_labels {
            let unexpected_label = unexpected_label.as_ref();

            assert!(
                !completion_labels
                    .iter()
                    .any(|completion_label| completion_label == unexpected_label),
                "expected semantic completions to exclude `{unexpected_label}`; got {completion_labels:?}"
            );
        }
    }

    #[must_use]
    pub fn completion_labels(&self, completion_source: &SemanticCompletionSource) -> Vec<String> {
        completion_source.completion_labels(&self.semantic_index)
    }

    fn reference_accesses(access_fields: impl IntoIterator<Item = impl Into<String>>) -> Vec<ReferenceAccess> {
        access_fields.into_iter().map(ReferenceAccess::required).collect()
    }
}

impl SemanticTypeRoot {
    #[must_use]
    pub fn workflow_type(&self, semantic_index: &WorkflowSemanticIndex) -> Option<WorkflowType> {
        match self {
            Self::Input => semantic_index.input_type().cloned(),
            Self::Secrets => semantic_index.secrets_type().cloned(),
            Self::Schema(schema_name) => semantic_index.schema(schema_name)?.workflow_type.clone(),
            Self::AgentOutput(agent_name) => semantic_index.agent_output_workflow_type(agent_name).cloned(),
            Self::ToolInput(tool_name) => semantic_index.tool_input_type(tool_name).cloned(),
            Self::ToolBinding(tool_name) => semantic_index.tool_binding_type(tool_name).cloned(),
            Self::ToolOutput(tool_name) => semantic_index.tool_output_type(tool_name).cloned(),
        }
    }
}

impl SemanticCompletionSource {
    #[must_use]
    pub fn completion_labels(&self, semantic_index: &WorkflowSemanticIndex) -> Vec<String> {
        let mut completion_labels = match self {
            Self::ReferenceRoots => [
                ReferenceKeyword::Agent,
                ReferenceKeyword::Dynamic,
                ReferenceKeyword::Input,
                ReferenceKeyword::Model,
                ReferenceKeyword::Secrets,
                ReferenceKeyword::Tool,
                ReferenceKeyword::Resource,
                ReferenceKeyword::Prompt,
            ]
            .into_iter()
            .map(|reference_keyword| reference_keyword.as_str().to_string())
            .collect(),
            Self::Providers => semantic_index.provider_names().map(str::to_string).collect(),
            Self::Models => semantic_index.model_names().map(str::to_string).collect(),
            Self::McpServers => semantic_index.mcp_server_names().map(str::to_string).collect(),
            Self::Schemas => semantic_index.schema_names().map(str::to_string).collect(),
            Self::Agents => semantic_index.agent_names().map(str::to_string).collect(),
            Self::Tools => semantic_index.tool_names().map(str::to_string).collect(),
            Self::Resources => semantic_index.resource_names().map(str::to_string).collect(),
            Self::Prompts => semantic_index.prompt_names().map(str::to_string).collect(),
            Self::Fields(type_root) => type_root
                .workflow_type(semantic_index)
                .and_then(|workflow_type| workflow_type.field_names())
                .unwrap_or_default(),
        };

        completion_labels.sort();
        completion_labels
    }
}

impl FakeMcpClientFactory {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_server(mut self, server_name: impl Into<String>, configure: impl FnOnce(&mut FakeMcpServerBuilder)) -> Self {
        self.add_server(server_name, configure);
        self
    }

    pub fn add_server(&mut self, server_name: impl Into<String>, configure: impl FnOnce(&mut FakeMcpServerBuilder)) -> &mut Self {
        let server_name = server_name.into();
        let mut builder = FakeMcpServerBuilder::default();

        configure(&mut builder);

        self.servers_by_name
            .lock()
            .expect("fake MCP server registry lock poisoned")
            .insert(server_name.clone(), builder.build(server_name));

        self
    }

    #[must_use]
    pub fn requests(&self, server_name: &str) -> Vec<FakeMcpRequest> {
        let servers_by_name = self.servers_by_name.lock().expect("fake MCP server registry lock poisoned");

        servers_by_name.get(server_name).map(FakeMcpServer::requests).unwrap_or_default()
    }

    #[must_use]
    pub fn unused_tool_response_counts(&self, server_name: &str) -> BTreeMap<String, usize> {
        let servers_by_name = self.servers_by_name.lock().expect("fake MCP server registry lock poisoned");

        servers_by_name
            .get(server_name)
            .map(FakeMcpServer::unused_tool_response_counts)
            .unwrap_or_default()
    }

    fn server_for_config(&self, server_config: &McpServerConfig) -> Result<FakeMcpServer, McpError> {
        self.servers_by_name
            .lock()
            .expect("fake MCP server registry lock poisoned")
            .get(&server_config.name)
            .cloned()
            .ok_or_else(|| McpError::Http {
                server_name: server_config.name.clone(),
                method: "fake".to_string(),
                message: format!("fake MCP server `{}` is not registered", server_config.name),
            })
    }
}

impl McpClientFactory for FakeMcpClientFactory {
    fn client_for_config(&self, server_config: McpServerConfig) -> Result<Arc<dyn McpClientBackend>, McpError> {
        let server = self.server_for_config(&server_config)?;

        Ok(Arc::new(FakeMcpClient { server }))
    }
}

impl FakeMcpServerBuilder {
    pub fn tool(&mut self, tool_name: impl Into<String>, configure: impl FnOnce(&mut FakeMcpToolBuilder)) -> &mut Self {
        let tool_name = tool_name.into();
        let mut builder = FakeMcpToolBuilder::default();

        configure(&mut builder);
        self.tools.insert(tool_name, builder.build());

        self
    }

    pub fn resource(&mut self, resource_name: impl Into<String>, configure: impl FnOnce(&mut FakeMcpResourceBuilder)) -> &mut Self {
        let resource_name = resource_name.into();
        let mut builder = FakeMcpResourceBuilder::default();

        configure(&mut builder);
        self.resources.insert(resource_name.clone(), builder.build(resource_name));

        self
    }

    pub fn prompt(&mut self, prompt_name: impl Into<String>, configure: impl FnOnce(&mut FakeMcpPromptBuilder)) -> &mut Self {
        let prompt_name = prompt_name.into();
        let mut builder = FakeMcpPromptBuilder::default();

        configure(&mut builder);
        self.prompts.insert(prompt_name, builder.build());

        self
    }

    fn build(self, server_name: String) -> FakeMcpServer {
        FakeMcpServer {
            name: server_name,
            tools: self.tools,
            resources: self.resources,
            prompts: self.prompts,
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl Default for FakeMcpToolBuilder {
    fn default() -> Self {
        Self {
            description: None,
            input_schema: empty_object_schema(),
            output_schema: None,
            responses: VecDeque::new(),
        }
    }
}

impl FakeMcpToolBuilder {
    pub fn description(&mut self, description: impl Into<String>) -> &mut Self {
        self.description = Some(description.into());
        self
    }

    pub fn input_schema(&mut self, input_schema: Value) -> &mut Self {
        self.input_schema = input_schema;
        self
    }

    pub fn output_schema(&mut self, output_schema: Value) -> &mut Self {
        self.output_schema = Some(output_schema);
        self
    }

    pub fn respond_json(&mut self, response: Value) -> &mut Self {
        self.responses.push_back(response);
        self
    }

    fn build(self) -> FakeMcpTool {
        FakeMcpTool {
            description: self.description,
            input_schema: self.input_schema,
            output_schema: self.output_schema,
            responses: Arc::new(Mutex::new(self.responses)),
        }
    }
}

impl FakeMcpResourceBuilder {
    pub fn uri(&mut self, uri: impl Into<String>) -> &mut Self {
        self.uri = Some(uri.into());
        self
    }

    pub fn mime_type(&mut self, mime_type: impl Into<String>) -> &mut Self {
        self.mime_type = Some(mime_type.into());
        self
    }

    pub fn text(&mut self, text: impl Into<String>) -> &mut Self {
        self.text = Some(text.into());
        self
    }

    fn build(self, resource_name: String) -> FakeMcpResource {
        FakeMcpResource {
            uri: self.uri.unwrap_or_else(|| format!("file:///{resource_name}")),
            mime_type: self.mime_type.unwrap_or_else(|| "text/plain".to_string()),
            text: self.text.unwrap_or_default(),
        }
    }
}

impl FakeMcpPromptBuilder {
    pub fn description(&mut self, _description: impl Into<String>) -> &mut Self {
        self
    }

    pub fn text(&mut self, text: impl Into<String>) -> &mut Self {
        self.text = Some(text.into());
        self
    }

    pub fn argument(&mut self, argument_name: impl Into<String>, required: bool) -> &mut Self {
        let argument_name = argument_name.into();

        self.arguments.push(McpPromptArgumentLock {
            name: argument_name.clone(),
            required,
            description: Some(format!("Test prompt argument {argument_name}")),
        });

        self
    }

    pub fn respond_json(&mut self, response: Value) -> &mut Self {
        self.responses.push_back(response);
        self
    }

    fn build(self) -> FakeMcpPrompt {
        let text = self.text.unwrap_or_default();
        let default_response = serde_json::json!({
            "messages": [{
                "role": "user",
                "content": {
                    "type": "text",
                    "text": text,
                },
            }],
        });
        FakeMcpPrompt {
            arguments: self.arguments,
            responses: Arc::new(Mutex::new(self.responses)),
            default_response,
        }
    }
}

impl FakeMcpServer {
    fn requests(&self) -> Vec<FakeMcpRequest> {
        self.requests.lock().expect("fake MCP request log lock poisoned").clone()
    }

    fn unused_tool_response_counts(&self) -> BTreeMap<String, usize> {
        self.tools
            .iter()
            .filter_map(|(tool_name, tool)| {
                let remaining_response_count = tool.responses.lock().expect("fake MCP tool response lock poisoned").len();

                (remaining_response_count > 0).then(|| (tool_name.clone(), remaining_response_count))
            })
            .collect()
    }

    fn record_request(&self, method: &str, name: Option<&str>, arguments: Value) {
        self.requests
            .lock()
            .expect("fake MCP request log lock poisoned")
            .push(FakeMcpRequest {
                server_name: self.name.clone(),
                method: method.to_string(),
                name: name.map(str::to_string),
                arguments,
            });
    }

    fn server_lock(&self) -> Result<McpServerLock, McpError> {
        let mut server_lock = McpServerLock::default();

        for (tool_name, tool) in &self.tools {
            let Some(tool_lock) = McpToolLock::from_json_schema_values(
                tool_name.clone(),
                tool.description.clone(),
                tool.input_schema.clone(),
                tool.output_schema.clone(),
            ) else {
                return Err(McpError::InvalidResponse {
                    server_name: self.name.clone(),
                    method: "tools/list".to_string(),
                    message: format!("fake MCP tool `{tool_name}` has an invalid schema"),
                });
            };

            server_lock.tools.insert(tool_name.clone(), tool_lock);
        }

        server_lock.resources = self.resources.keys().cloned().collect();
        server_lock.prompts = self.prompts.keys().cloned().collect();
        server_lock.prompt_arguments = self
            .prompts
            .iter()
            .map(|(prompt_name, prompt)| (prompt_name.clone(), prompt.arguments.clone()))
            .collect();

        Ok(server_lock)
    }
}

impl McpClientBackend for FakeMcpClient {
    fn list_tools(&self) -> Result<McpServerLock, McpError> {
        self.server.record_request("tools/list", None, Value::Null);
        self.server.server_lock()
    }

    fn call_tool(&self, tool_name: &str, arguments: Value) -> Result<Value, McpError> {
        self.server.record_request("tools/call", Some(tool_name), arguments);
        let tool = self.server.tools.get(tool_name).ok_or_else(|| McpError::Rpc {
            server_name: self.server.name.clone(),
            method: "tools/call".to_string(),
            message: format!("tool `{tool_name}` not found"),
        })?;

        tool.responses
            .lock()
            .expect("fake MCP tool response lock poisoned")
            .pop_front()
            .ok_or_else(|| McpError::Rpc {
                server_name: self.server.name.clone(),
                method: "tools/call".to_string(),
                message: format!("unexpected extra call to MCP tool `{tool_name}`"),
            })
    }

    fn read_resource(&self, resource_name: &str, arguments: Value) -> Result<Value, McpError> {
        self.server.record_request("resources/read", Some(resource_name), arguments);
        let resource = self.server.resources.get(resource_name).ok_or_else(|| McpError::Rpc {
            server_name: self.server.name.clone(),
            method: "resources/read".to_string(),
            message: format!("resource `{resource_name}` not found"),
        })?;

        Ok(serde_json::json!({
            "contents": [{
                "uri": resource.uri,
                "mimeType": resource.mime_type,
                "text": resource.text,
            }],
        }))
    }

    fn get_prompt(&self, prompt_name: &str, arguments: Value) -> Result<Value, McpError> {
        self.server.record_request("prompts/get", Some(prompt_name), arguments);
        let prompt = self.server.prompts.get(prompt_name).ok_or_else(|| McpError::Rpc {
            server_name: self.server.name.clone(),
            method: "prompts/get".to_string(),
            message: format!("prompt `{prompt_name}` not found"),
        })?;

        Ok(prompt
            .responses
            .lock()
            .expect("fake MCP prompt response lock poisoned")
            .pop_front()
            .unwrap_or_else(|| prompt.default_response.clone()))
    }
}

#[must_use]
pub fn empty_object_schema() -> Value {
    serde_json::to_value(schemars::json_schema!({
        "type": "object",
        "properties": {},
        "required": [],
        "additionalProperties": false,
    }))
    .expect("empty object schema should serialize")
}

#[must_use]
pub fn schema_for_type<Type>() -> Value
where
    Type: schemars::JsonSchema,
{
    let mut schema = serde_json::to_value(schemars::schema_for!(Type)).expect("test schema should serialize");

    if let Some(schema_object) = schema.as_object_mut() {
        schema_object.remove("$schema");
        schema_object.remove("title");
    }

    schema
}

#[must_use]
pub fn stable_text_diff(expected: &str, actual: &str) -> String {
    let expected_lines = expected.lines().collect::<Vec<_>>();
    let actual_lines = actual.lines().collect::<Vec<_>>();
    let line_count = expected_lines.len().max(actual_lines.len());
    let mut difference_text = String::new();

    for line_index in 0..line_count {
        let expected_line = expected_lines.get(line_index).copied();
        let actual_line = actual_lines.get(line_index).copied();

        if expected_line == actual_line {
            continue;
        }

        let _ = writeln!(difference_text, "line {}:", line_index + 1);

        match expected_line {
            Some(line) => {
                let _ = writeln!(difference_text, "  expected: {line}");
            }
            None => difference_text.push_str("  expected: <missing>\n"),
        }

        match actual_line {
            Some(line) => {
                let _ = writeln!(difference_text, "  actual:   {line}");
            }
            None => difference_text.push_str("  actual:   <missing>\n"),
        }
    }

    difference_text
}

#[must_use]
pub fn normalize_rust_doc_comment_tokens(source_template: &str) -> String {
    let mut normalized_source = String::new();
    let mut remaining_source = source_template;

    while let Some(doc_attribute_start) = remaining_source.find("#[doc = r\"") {
        normalized_source.push_str(&remaining_source[..doc_attribute_start]);
        remaining_source = &remaining_source[doc_attribute_start + "#[doc = r\"".len()..];

        let Some(doc_attribute_end) = remaining_source.find("\"]") else {
            normalized_source.push_str("#[doc = r\"");
            normalized_source.push_str(remaining_source);

            return normalized_source;
        };

        normalized_source.push_str("///");
        normalized_source.push_str(&remaining_source[..doc_attribute_end]);
        normalized_source.push('\n');
        remaining_source = &remaining_source[doc_attribute_end + "\"]".len()..];
    }

    normalized_source.push_str(remaining_source);
    normalized_source
}

#[must_use]
pub fn normalize_inline_cursor_layout(source_template: &str) -> String {
    let compact_marker_offset = source_template.find(COMPACT_CURSOR_MARKER);
    let spaced_marker_offset = source_template.find(SPACED_CURSOR_MARKER);

    let (marker, marker_offset) = match (compact_marker_offset, spaced_marker_offset) {
        (Some(compact_offset), Some(spaced_offset)) => {
            if compact_offset <= spaced_offset {
                (COMPACT_CURSOR_MARKER, compact_offset)
            } else {
                (SPACED_CURSOR_MARKER, spaced_offset)
            }
        }
        (Some(compact_offset), None) => (COMPACT_CURSOR_MARKER, compact_offset),
        (None, Some(spaced_offset)) => (SPACED_CURSOR_MARKER, spaced_offset),
        (None, None) => {
            return source_template.to_string();
        }
    };

    if is_inside_string_literal(source_template, marker_offset) {
        return source_template.to_string();
    }

    let previous_character = source_template[..marker_offset]
        .chars()
        .rev()
        .find(|character| !character.is_whitespace());

    if previous_character == Some('.') || previous_character == Some(':') || previous_character == Some('(') {
        return source_template.to_string();
    }

    let mut normalized_source = String::new();
    normalized_source.push_str(&source_template[..marker_offset]);

    if !normalized_source.ends_with('\n') {
        normalized_source.push('\n');
    }

    normalized_source.push_str(marker);

    let marker_end_offset = marker_offset + marker.len();
    let remaining_source = &source_template[marker_end_offset..];
    let next_character = remaining_source.chars().find(|character| !character.is_whitespace());

    if next_character == Some('{') {
        return source_template.to_string();
    }

    if next_character == Some('}') {
        normalized_source.push('\n');
    }

    normalized_source.push_str(remaining_source);

    merge_lone_opening_brace_lines(&normalized_source)
}

fn source_without_cursor_normalization(source_template: &str) -> WorkflowSourceWithCursor {
    let (cursor_marker, cursor_byte_offset) = if let Some(marker_offset) = source_template.find(COMPACT_CURSOR_MARKER) {
        (COMPACT_CURSOR_MARKER, marker_offset)
    } else {
        panic!("cursor marker should exist in test source");
    };

    let mut line = 0_u32;
    let mut character = 0_u32;

    for character_in_source in source_template[..cursor_byte_offset].chars() {
        if character_in_source == '\n' {
            line += 1;
            character = 0;

            continue;
        }

        character += 1;
    }

    let source_text = source_template.replacen(cursor_marker, "", 1);

    WorkflowSourceWithCursor {
        source_text,
        cursor_position: InlineCursorPosition { line, character },
    }
}

fn merge_lone_opening_brace_lines(source_text: &str) -> String {
    let mut source_lines = source_text.lines().map(str::to_string).collect::<Vec<_>>();
    let mut line_index = 0_usize;

    while line_index < source_lines.len() {
        if line_index == 0 {
            line_index += 1;

            continue;
        }

        if source_lines[line_index].trim() != "{" {
            line_index += 1;

            continue;
        }

        if !source_lines[line_index - 1].is_empty() {
            source_lines[line_index - 1].push(' ');
        }

        source_lines[line_index - 1].push('{');
        let _ = source_lines.remove(line_index);
    }

    source_lines.join("\n")
}

fn is_inside_string_literal(source_text: &str, byte_offset: usize) -> bool {
    let mut inside_string = false;
    let mut escaping = false;

    for character in source_text[..byte_offset].chars() {
        if escaping {
            escaping = false;

            continue;
        }

        if inside_string {
            if character == '\\' {
                escaping = true;

                continue;
            }

            if character == '"' {
                inside_string = false;
            }

            continue;
        }

        if character == '"' {
            inside_string = true;
        }
    }

    inside_string
}

#[cfg(test)]
mod tests {
    use super::{
        empty_object_schema, stable_text_diff, FormatterSnapshotAssertion, GraphJsonSnapshotAssertion, LockFileSnapshotAssertion,
        SemanticCompletionSource, SemanticFixture, SemanticIndexSnapshotAssertion, SemanticTypeRoot, WorkflowSource,
        WorkflowSourceTemplate,
    };
    use crate::dsl::ReferenceKeyword;
    use crate::semantic::support::types::WorkflowType;
    use crate::semantic::{ReferenceResolutionError, ReferenceResolutionRoot, WorkflowExecutionGraph, WorkflowSemanticIndex};
    use serde_json::json;
    use std::collections::BTreeMap;
    use superwire_mcp::{McpLock, McpServerLock, ProjectMcpLock};

    #[test]
    fn inline_cursor_layout_normalizes_cursor_before_block_close() {
        let source_template = WorkflowSourceTemplate::from_inline(crate::workflow_source! {
            agent worker { <cursor> }
        });
        let source_with_cursor = source_template.with_cursor();

        assert_eq!(source_with_cursor.cursor_position().line, 1);
        assert_eq!(source_with_cursor.cursor_position().character, 0);
        assert_eq!(source_with_cursor.source(), "agent worker { \n\n }");
    }

    #[test]
    fn stable_text_diff_reports_changed_lines() {
        let difference_text = stable_text_diff("alpha\nbeta", "alpha\ngamma");

        assert!(difference_text.contains("line 2:"));
        assert!(difference_text.contains("expected: beta"));
        assert!(difference_text.contains("actual:   gamma"));
    }

    #[test]
    fn inline_workflow_source_reads_formatted_source() {
        let workflow_source = WorkflowSource::inline(crate::workflow_source! {
            input {
                project_id:number
            }
        });

        let formatted_source = workflow_source.read_formatted().expect("inline workflow source should format");

        assert!(formatted_source.contains("project_id: number"));
    }

    #[test]
    fn workflow_source_template_macro_parses_inline_source() {
        let source_template = crate::workflow_source_template! {
            input {
                project_id: number
            }
        };

        let workflow = source_template.parse_workflow().expect("inline workflow source should parse");

        assert_eq!(workflow.declarations().len(), 1);
    }

    #[test]
    fn empty_object_schema_returns_stable_schema_value() {
        let schema = empty_object_schema();

        assert_eq!(
            schema,
            json!({
                "type": "object",
                "properties": {},
                "required": [],
                "additionalProperties": false,
            })
        );
    }

    #[test]
    fn formatter_snapshot_assertion_compares_formatted_output() {
        let source_template = WorkflowSourceTemplate::from_inline(crate::workflow_source! {
            output { value:"ok" }
        });
        let expected_output = concat!("output {\n", "    value: \"ok\"\n", "}\n");

        FormatterSnapshotAssertion::from_source_template("formatter output", expected_output, &source_template)
            .expect("workflow source should format")
            .assert_matches();
    }

    #[test]
    fn graph_json_snapshot_assertion_compares_stable_json() {
        let graph = WorkflowExecutionGraph {
            nodes: Vec::new(),
            edges: Vec::new(),
            agent_execution_order: Vec::new(),
        };
        let expected_output = concat!(
            "{\n",
            "  \"nodes\": [],\n",
            "  \"edges\": [],\n",
            "  \"agent_execution_order\": []\n",
            "}\n"
        );

        GraphJsonSnapshotAssertion::from_graph("graph json", expected_output, &graph).assert_matches();
    }

    #[test]
    fn semantic_index_snapshot_assertion_compares_stable_summary() {
        let workflow = crate::parse_inline_workflow! {
            input {
                topic: string
            }

            tool web_search {
                input {
                    query: string
                }

                bindings {
                    api_key: input.topic
                }

                output {
                    summary: string
                }
            }

            agent researcher {
                uses: [tool.web_search]
                instruction: input.topic
                output {
                    summary: string
                }
            }

            output {
                summary: agent.researcher.summary
            }
        };
        let semantic_index = WorkflowSemanticIndex::from_workflow(&workflow);
        let expected_output = concat!(
            "providers:\n",
            "  - none\n",
            "\n",
            "models:\n",
            "  - none\n",
            "\n",
            "schemas:\n",
            "  - none\n",
            "\n",
            "tools:\n",
            "  - web_search\n",
            "\n",
            "resources:\n",
            "  - none\n",
            "\n",
            "prompts:\n",
            "  - none\n",
            "\n",
            "agents:\n",
            "  - researcher\n",
            "\n",
            "schema types:\n",
            "  - none\n",
            "\n",
            "input fields:\n",
            "  - topic: string\n",
            "\n",
            "secrets fields:\n",
            "  - none\n",
            "\n",
            "agent output types:\n",
            "  - researcher: { summary: string }\n",
            "\n",
            "tool input types:\n",
            "  - web_search: { query: string }\n",
            "\n",
            "tool binding types:\n",
            "  - web_search: {  }\n",
            "\n",
            "tool output types:\n",
            "  - web_search: { summary: string }\n",
            "\n",
            "tool fixed bindings:\n",
            "  - web_search: api_key\n",
        );

        SemanticIndexSnapshotAssertion::from_index("semantic index summary", expected_output, &semantic_index).assert_matches();
    }

    #[test]
    fn semantic_fixture_builds_index_and_asserts_types() {
        let fixture = SemanticFixture::from_source_template(crate::workflow_source_template! {
            input {
                profile: {
                    title: string
                    score: number
                }
            }

            tool web_search {
                input {
                    query: string
                }

                output {
                    summary: string
                }
            }

            agent researcher {
                uses: [tool.web_search]
                instruction: input.profile.title

                output {
                    summary: string
                }
            }
        });

        fixture.assert_has_declaration(SemanticCompletionSource::Agents, "researcher");
        fixture.assert_has_declaration(SemanticCompletionSource::Tools, "web_search");
        fixture.assert_field_type(SemanticTypeRoot::Input, &["profile", "title"], WorkflowType::String);
        fixture.assert_field_type(
            SemanticTypeRoot::ToolOutput("web_search".to_string()),
            &["summary"],
            WorkflowType::String,
        );
        fixture.assert_completion_contains(SemanticCompletionSource::Fields(SemanticTypeRoot::Input), ["profile"]);
        fixture.assert_completion_excludes(SemanticCompletionSource::Fields(SemanticTypeRoot::Input), ["missing"]);
    }

    #[test]
    fn semantic_fixture_asserts_reference_resolution_and_errors() {
        let fixture = SemanticFixture::from_source_template(crate::workflow_source_template! {
            input {
                profile: {
                    title: string
                }
            }

            agent worker {
                instruction: input.profile.title

                output {
                    summary: string
                }
            }
        })
        .with_dynamic_field_type("topic", WorkflowType::String);

        let input_reference = SemanticFixture::keyword_reference(ReferenceKeyword::Input, ["profile", "title"]);
        let agent_reference = SemanticFixture::keyword_reference(ReferenceKeyword::Agent, ["worker", "summary"]);
        let dynamic_reference = SemanticFixture::keyword_reference(ReferenceKeyword::Dynamic, ["topic"]);
        let missing_reference = SemanticFixture::keyword_reference(ReferenceKeyword::Input, ["missing"]);

        fixture.assert_reference_resolves_to(&input_reference, ReferenceResolutionRoot::Input, "profile");
        fixture.assert_reference_type(&input_reference, WorkflowType::String);
        fixture.assert_reference_resolves_to(&agent_reference, ReferenceResolutionRoot::Agent, "worker");
        fixture.assert_reference_type(&agent_reference, WorkflowType::String);
        fixture.assert_reference_type(&dynamic_reference, WorkflowType::String);
        fixture.assert_reference_error(
            &missing_reference,
            ReferenceResolutionError::UnknownInputField {
                field_name: "missing".to_string(),
            },
        );
    }

    #[test]
    fn semantic_fixture_exposes_core_completion_labels() {
        let fixture = SemanticFixture::from_source_template(crate::workflow_source_template! {
            provider openai from openai {}

            model fast from openai {
                id: "gpt-4.1-mini"
            }

            schema Task {
                name: string
                priority: number
            }

            mcp local {
                endpoint: "http://localhost:3000/mcp"
            }

            resource project_readme from mcp.local.resource.project_readme
            prompt system_prompt from mcp.local.prompt.system_prompt
        });

        fixture.assert_completion_contains(
            SemanticCompletionSource::ReferenceRoots,
            [ReferenceKeyword::Agent.as_str(), ReferenceKeyword::Input.as_str()],
        );
        fixture.assert_completion_contains(SemanticCompletionSource::Providers, ["openai"]);
        fixture.assert_completion_contains(SemanticCompletionSource::Models, ["fast"]);
        fixture.assert_completion_contains(SemanticCompletionSource::McpServers, ["local"]);
        fixture.assert_completion_contains(SemanticCompletionSource::Schemas, ["Task"]);
        fixture.assert_completion_contains(SemanticCompletionSource::Resources, ["project_readme"]);
        fixture.assert_completion_contains(SemanticCompletionSource::Prompts, ["system_prompt"]);
        fixture.assert_completion_contains(
            SemanticCompletionSource::Fields(SemanticTypeRoot::Schema("Task".to_string())),
            ["name", "priority"],
        );
    }

    #[test]
    fn lock_file_snapshot_assertion_compares_stable_lock_text() {
        let workflow_lock = McpLock {
            servers: BTreeMap::from([("local".to_string(), McpServerLock::default())]),
        };
        let project_lock = ProjectMcpLock::empty();
        let expected_workflow_lock = concat!(
            "{\n",
            "  \"servers\": {\n",
            "    \"local\": {\n",
            "      \"tools\": {}\n",
            "    }\n",
            "  }\n",
            "}\n"
        );
        let expected_project_lock = concat!("{\n", "  \"version\": 1,\n", "  \"workflows\": {}\n", "}\n");

        LockFileSnapshotAssertion::from_workflow_lock("workflow lock", expected_workflow_lock, &workflow_lock).assert_matches();
        LockFileSnapshotAssertion::from_project_lock("project lock", expected_project_lock, &project_lock).assert_matches();
    }
}
