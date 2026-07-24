export type ValidationState = 'idle' | 'valid' | 'invalid' | 'running';
export type RunState = 'idle' | 'running' | 'failed' | 'cancelled' | 'completed';
export type GraphState = 'idle' | 'loading' | 'failed' | 'ready';
export type PlaygroundView = 'workflow' | 'graph';
export type WorkflowEditorView = 'code' | 'input' | 'secrets';

export enum ExecutorDiagnosticCode {
  InvalidWorkflow = 'invalid_workflow',
  InvalidInput = 'invalid_input',
  InvalidSecrets = 'invalid_secrets',
  InvalidOutput = 'invalid_output',
  InvalidConfiguration = 'invalid_configuration',
  ModelProviderFailed = 'model_provider_failed',
  ModelRejected = 'model_rejected',
  ProviderRateLimited = 'provider_rate_limited',
  ProviderRetriesExhausted = 'provider_retries_exhausted',
  ToolFailed = 'tool_failed',
  McpFailed = 'mcp_failed',
  CacheUnavailable = 'cache_unavailable',
  StreamGap = 'stream_gap',
  StreamExpired = 'stream_expired',
  UnknownRun = 'unknown_run',
  CancellationConflict = 'cancellation_conflict',
  Cancelled = 'cancelled',
  InternalPanic = 'internal_panic',
  InternalError = 'internal_error',
}

export enum ExecutorStage {
  Planning = 'planning',
  Input = 'input',
  Secrets = 'secrets',
  Agent = 'agent',
  Model = 'model',
  Tool = 'tool',
  Mcp = 'mcp',
  Cache = 'cache',
  Output = 'output',
  Stream = 'stream',
  Cancellation = 'cancellation',
  Internal = 'internal',
}

export enum ExecutorDiagnosticSeverity {
  Warning = 'warning',
  Error = 'error',
}

export enum ExecutorDiagnosticRetryability {
  Never = 'never',
  Unknown = 'unknown',
  Safe = 'safe',
  AfterDelay = 'after_delay',
}

export enum ExecutorDiagnosticSubjectType {
  Workflow = 'workflow',
  Agent = 'agent',
  Provider = 'provider',
  Tool = 'tool',
  Mcp = 'mcp',
  Cache = 'cache',
  Stream = 'stream',
}

export enum ExecutorCacheOperation {
  Connect = 'connect',
  Read = 'read',
  Write = 'write',
  Purge = 'purge',
}

export type ExecutionDiagnosticSubject =
  | { type: ExecutorDiagnosticSubjectType.Workflow }
  | { type: ExecutorDiagnosticSubjectType.Agent; agent_name: string; iteration_index?: number }
  | { type: ExecutorDiagnosticSubjectType.Provider; agent_name: string; provider_name?: string; model_name?: string; attempt?: number; http_status?: number }
  | { type: ExecutorDiagnosticSubjectType.Tool; agent_name?: string; tool_name: string }
  | { type: ExecutorDiagnosticSubjectType.Mcp; agent_name?: string; server_name?: string; target_name?: string }
  | { type: ExecutorDiagnosticSubjectType.Cache; operation: ExecutorCacheOperation }
  | { type: ExecutorDiagnosticSubjectType.Stream; requested_after?: string; oldest_available?: string };

export interface ExecutionDiagnostic {
  code: ExecutorDiagnosticCode;
  stage: ExecutorStage;
  severity: ExecutorDiagnosticSeverity;
  retryability: ExecutorDiagnosticRetryability;
  message: string;
  subject: ExecutionDiagnosticSubject;
  retry_after_ms?: number;
  cause?: ExecutionDiagnostic;
}

export enum ExecutorEventKind {
  WorkflowStarted = 'workflow_started',
  WorkflowPlanned = 'workflow_planned',
  AgentLoopStarted = 'agent_loop_started',
  AgentLoopCompleted = 'agent_loop_completed',
  AgentLoopFailed = 'agent_loop_failed',
  AgentLoopCancelled = 'agent_loop_cancelled',
  ContextCompactionStarted = 'context_compaction_started',
  ContextCompactionCompleted = 'context_compaction_completed',
  ContextCompactionFailed = 'context_compaction_failed',
  AgentFileCreated = 'agent_file_created',
  AgentFileDeleted = 'agent_file_deleted',
  AgentStarted = 'agent_started',
  AgentCompleted = 'agent_completed',
  AgentFailed = 'agent_failed',
  AgentCancelled = 'agent_cancelled',
  ProviderAttemptStarted = 'provider_attempt_started',
  ProviderAttemptCompleted = 'provider_attempt_completed',
  ProviderAttemptFailed = 'provider_attempt_failed',
  ToolCallStarted = 'tool_call_started',
  ToolCallFailed = 'tool_call_failed',
  ToolCallCompleted = 'tool_call_completed',
  McpToolSchemaFetchStarted = 'mcp_tool_schema_fetch_started',
  McpToolSchemaFetchFailed = 'mcp_tool_schema_fetch_failed',
  McpToolSchemaFetchCompleted = 'mcp_tool_schema_fetch_completed',
  McpToolValidationStarted = 'mcp_tool_validation_started',
  McpToolValidationFailed = 'mcp_tool_validation_failed',
  McpToolValidationCompleted = 'mcp_tool_validation_completed',
  McpCallStarted = 'mcp_call_started',
  McpCallFailed = 'mcp_call_failed',
  McpCallCompleted = 'mcp_call_completed',
  CacheDegraded = 'cache_degraded',
  StreamGap = 'stream_gap',
  WorkflowCompleted = 'workflow_completed',
  WorkflowFailed = 'workflow_failed',
  WorkflowCancelled = 'workflow_cancelled',
}

export enum McpImportKind {
  Prompt = 'prompt',
  Resource = 'resource',
}

export enum McpOperation {
  Call = 'call',
  Read = 'read',
  Render = 'render',
}

export enum EventValueKind {
  Null = 'null',
  Boolean = 'boolean',
  Number = 'number',
  String = 'string',
  Array = 'array',
  Object = 'object',
}

export interface ExecutorMcpImport {
  name: string;
  kind: McpImportKind;
  server_name: string;
  item_name: string;
}

interface ExecutorProviderAttemptData {
  provider_name: string;
  model_name: string;
  attempt: number;
  total_attempts: number;
}

export interface ExecutorMcpCallData {
  operation: McpOperation;
  target_name: string;
  server_name: string;
  item_name: string;
  argument_names: string[];
}

export interface ExecutorEventDataByKind {
  [ExecutorEventKind.WorkflowStarted]: Record<string, never>;
  [ExecutorEventKind.WorkflowPlanned]: { agent_execution_order: string[]; mcp_imports: ExecutorMcpImport[]; steps: unknown[] };
  [ExecutorEventKind.AgentLoopStarted]: { iteration_count: number; binding_names: string[] };
  [ExecutorEventKind.AgentLoopCompleted]: { result_kind: EventValueKind; item_count: number; duration_ms: number; iteration_count: number };
  [ExecutorEventKind.AgentLoopFailed]: { duration_ms: number };
  [ExecutorEventKind.AgentLoopCancelled]: { duration_ms: number };
  [ExecutorEventKind.ContextCompactionStarted]: { model: string; source_agent_name?: string };
  [ExecutorEventKind.ContextCompactionCompleted]: { result_kind: EventValueKind; item_count?: number; duration_ms: number };
  [ExecutorEventKind.ContextCompactionFailed]: { duration_ms: number };
  [ExecutorEventKind.AgentFileCreated]: { filename: string; purpose: string; bytes?: number };
  [ExecutorEventKind.AgentFileDeleted]: { filename: string; purpose: string };
  [ExecutorEventKind.AgentStarted]: { model: string; tools: unknown[]; iteration_index?: number };
  [ExecutorEventKind.AgentCompleted]: { result_kind: EventValueKind; item_count?: number; duration_ms: number; cache_hit: boolean; iteration_index?: number };
  [ExecutorEventKind.AgentFailed]: { duration_ms: number; iteration_index?: number };
  [ExecutorEventKind.AgentCancelled]: { duration_ms: number; iteration_index?: number };
  [ExecutorEventKind.ProviderAttemptStarted]: ExecutorProviderAttemptData;
  [ExecutorEventKind.ProviderAttemptCompleted]: ExecutorProviderAttemptData & { duration_ms: number };
  [ExecutorEventKind.ProviderAttemptFailed]: ExecutorProviderAttemptData;
  [ExecutorEventKind.ToolCallStarted]: { tool_name: string; argument_names: string[] };
  [ExecutorEventKind.ToolCallFailed]: { tool_name: string; duration_ms: number };
  [ExecutorEventKind.ToolCallCompleted]: { tool_name: string; result_kind: EventValueKind; item_count?: number; duration_ms: number };
  [ExecutorEventKind.McpToolSchemaFetchStarted]: { server_name: string };
  [ExecutorEventKind.McpToolSchemaFetchFailed]: { server_name: string; duration_ms: number };
  [ExecutorEventKind.McpToolSchemaFetchCompleted]: { server_name: string; tool_count: number; duration_ms: number };
  [ExecutorEventKind.McpToolValidationStarted]: { tool_name: string; argument_names: string[] };
  [ExecutorEventKind.McpToolValidationFailed]: { tool_name: string; duration_ms: number };
  [ExecutorEventKind.McpToolValidationCompleted]: { tool_name: string; duration_ms: number };
  [ExecutorEventKind.McpCallStarted]: ExecutorMcpCallData;
  [ExecutorEventKind.McpCallFailed]: ExecutorMcpCallData & { duration_ms: number };
  [ExecutorEventKind.McpCallCompleted]: ExecutorMcpCallData & { result_kind: EventValueKind; item_count?: number; duration_ms: number };
  [ExecutorEventKind.CacheDegraded]: Record<string, never>;
  [ExecutorEventKind.StreamGap]: Record<string, never>;
  [ExecutorEventKind.WorkflowCompleted]: { output: unknown; duration_ms: number };
  [ExecutorEventKind.WorkflowFailed]: { duration_ms?: number };
  [ExecutorEventKind.WorkflowCancelled]: { duration_ms?: number };
}

export type ExecutorEvent = {
  [Kind in ExecutorEventKind]: {
    kind: Kind;
    timestamp_ms: number;
    agent_name?: string;
    message?: string;
    diagnostic?: ExecutionDiagnostic;
    data?: ExecutorEventDataByKind[Kind];
  };
}[ExecutorEventKind];

export enum CancellationTransition {
  Accepted = 'accepted',
  AlreadyRequested = 'already_requested',
  AlreadyTerminal = 'already_terminal',
  UnknownRun = 'unknown_run',
}

export interface CancellationResponse {
  transition: CancellationTransition;
}

export interface ExecutionSuccessResponse {
  output: unknown;
}

export interface ExecutorErrorResponse {
  error: ExecutionDiagnostic;
}

export interface WorkflowTab {
  id: string;
  name: string;
  activeView: PlaygroundView;
  activeEditorView: WorkflowEditorView;
  source: string;
  codeFragments: WorkflowCodeFragment[];
  activeCodeFragmentId: string;
  codeFragmentsUseMarkers: boolean;
  inputJson: string;
  secretsJson: string;
  useCache: boolean;
  cacheKey: string;
  validationState: ValidationState;
  runState: RunState;
  message: string;
  outputJson: string;
  eventLog: ExecutorEvent[];
  runtimeDiagnostic: ExecutionDiagnostic | null;
  graphState: GraphState;
  graphMessage: string;
  graphData: WorkflowExecutionGraph | null;
  updatedAt: number;
}

export interface WorkflowCodeFragment {
  id: string;
  name: string;
  source: string;
}

export interface WorkflowExecutionGraph {
  nodes: WorkflowExecutionGraphNode[];
  edges: WorkflowExecutionGraphEdge[];
  agent_execution_order: string[];
}

export interface WorkflowExecutionGraphNode {
  id: string;
  label: string;
  kind: WorkflowExecutionGraphNodeKind;
  inputs: WorkflowExecutionGraphPort[];
  outputs: WorkflowExecutionGraphPort[];
  dependencies: string[];
  provider_name: string | null;
  model: string | null;
  instruction: string | null;
  details: WorkflowExecutionGraphDetail[];
  bindings: WorkflowExecutionGraphBinding[];
  tools: WorkflowExecutionGraphTool[];
  execution_index: number | null;
  loop_info: WorkflowExecutionGraphLoopInfo | null;
}

export type WorkflowExecutionGraphNodeKind = 'provider' | 'model' | 'mcp' | 'input' | 'dynamic' | 'compact' | 'agent' | 'output';

export interface WorkflowExecutionGraphDetail {
  name: string;
  value: string;
  secret: boolean;
}

export interface WorkflowExecutionGraphBinding {
  name: string;
  expression: string;
}

export interface WorkflowExecutionGraphLoopInfo {
  pattern: string;
  iterable_schema: unknown;
  iteration_output_schema: unknown;
}

export interface WorkflowExecutionGraphPort {
  name: string;
  schema: unknown;
}

export interface WorkflowExecutionGraphTool {
  name: string;
  kind: WorkflowExecutionGraphToolKind;
  server_name: string | null;
  item_name: string | null;
  description: string | null;
  max_calls: number | null;
  input_schema: unknown;
  output_schema: unknown;
  bindings?: WorkflowExecutionGraphBinding[];
}

export type WorkflowExecutionGraphToolKind = 'local_tool' | 'mcp_tool' | 'mcp_prompt' | 'mcp_resource';

export interface WorkflowExecutionGraphEdge {
  id: string;
  source: string;
  target: string;
  label: string;
  kind: WorkflowExecutionGraphEdgeKind;
}

export type WorkflowExecutionGraphEdgeKind = 'provider_client' | 'model' | 'mcp_access' | 'input' | 'dynamic' | 'agent_dependency' | 'workflow_output';
