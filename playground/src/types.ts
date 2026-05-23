export type ValidationState = 'idle' | 'valid' | 'invalid' | 'running';
export type RunState = 'idle' | 'running' | 'failed' | 'completed';
export type GraphState = 'idle' | 'loading' | 'failed' | 'ready';
export type PlaygroundView = 'workflow' | 'graph';
export type WorkflowEditorView = 'code' | 'input' | 'secrets';

export interface ExecutorEvent {
  kind: string;
  timestamp_ms?: number;
  agent_name?: string;
  message?: string;
  data?: unknown;
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
  validationState: ValidationState;
  runState: RunState;
  message: string;
  outputJson: string;
  eventLog: ExecutorEvent[];
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

export type WorkflowExecutionGraphNodeKind = 'provider' | 'model' | 'mcp' | 'input' | 'dynamic' | 'agent' | 'output';

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
