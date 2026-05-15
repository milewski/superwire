export type ValidationState = 'idle' | 'valid' | 'invalid' | 'running';
export type RunState = 'idle' | 'running' | 'failed' | 'completed';
export type PlaygroundView = 'workflow' | 'runtime';

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
  updatedAt: number;
}
