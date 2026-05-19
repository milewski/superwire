export type ValidationState = 'idle' | 'valid' | 'invalid' | 'running';
export type RunState = 'idle' | 'running' | 'failed' | 'completed';
export type GraphState = 'idle' | 'loading' | 'failed' | 'ready';
export type PlaygroundView = 'workflow' | 'runtime' | 'graph';

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
  source: string;
  inputJson: string;
  secretsJson: string;
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
  tools: WorkflowExecutionGraphTool[];
  execution_index: number | null;
}

export type WorkflowExecutionGraphNodeKind = 'input' | 'agent' | 'output';

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
}

export type WorkflowExecutionGraphToolKind = 'local_tool' | 'mcp_tool' | 'mcp_prompt' | 'mcp_resource';

export interface WorkflowExecutionGraphEdge {
  id: string;
  source: string;
  target: string;
  label: string;
  kind: WorkflowExecutionGraphEdgeKind;
}

export type WorkflowExecutionGraphEdgeKind = 'input' | 'agent_dependency' | 'workflow_output';
