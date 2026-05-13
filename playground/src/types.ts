export type ValidationState = 'idle' | 'valid' | 'invalid' | 'running';
export type RunState = 'idle' | 'running' | 'failed' | 'completed';
export type RuntimeFieldKind = 'string' | 'number' | 'boolean' | 'json';

export interface ExecutorEvent {
  kind: string;
  agent_name?: string;
  message?: string;
  data?: unknown;
}

export interface RuntimeField {
  id: string;
  name: string;
  value: string;
  kind: RuntimeFieldKind;
}

export interface WorkflowTab {
  id: string;
  name: string;
  source: string;
  inputFields: RuntimeField[];
  secretFields: RuntimeField[];
  validationState: ValidationState;
  runState: RunState;
  message: string;
  outputJson: string;
  eventLog: ExecutorEvent[];
  updatedAt: number;
}
