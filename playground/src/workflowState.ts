import type { ExecutorEvent, WorkflowTab } from './types';

export function uniqueId() {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }

  return `id-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

export function createWorkflowTab(name: string): WorkflowTab {
  return {
    id: uniqueId(),
    name,
    activeView: 'workflow',
    source: '',
    inputJson: '{}',
    secretsJson: '{}',
    validationState: 'idle',
    runState: 'idle',
    message: 'Ready.',
    outputJson: '',
    eventLog: [],
    updatedAt: Date.now(),
  };
}

export function parseJsonObject(source: string, label: string): Record<string, unknown> {
  const trimmedSource = source.trim();

  if (!trimmedSource) {
    return {};
  }

  const parsedValue = JSON.parse(trimmedSource) as unknown;

  if (!isJsonObject(parsedValue)) {
    throw new Error(`${label} must be a JSON object.`);
  }

  return parsedValue;
}

export function normalizeWorkflowTab(tab: unknown): WorkflowTab {
  const fallbackTab = createWorkflowTab('Launch brief');

  if (!isJsonObject(tab)) {
    return fallbackTab;
  }

  return {
    ...fallbackTab,
    ...tab,
    activeView: normalizePlaygroundView(tab.activeView),
    inputJson: typeof tab.inputJson === 'string' ? tab.inputJson : JSON.stringify(fieldsToObject(tab.inputFields), null, 2),
    secretsJson: typeof tab.secretsJson === 'string' ? tab.secretsJson : JSON.stringify(fieldsToObject(tab.secretFields), null, 2),
    eventLog: Array.isArray(tab.eventLog) ? tab.eventLog : [],
  };
}

export function recoverWorkflowTabAfterReload(tab: unknown): WorkflowTab {
  const normalizedTab = normalizeWorkflowTab(tab);

  if (normalizedTab.runState !== 'running') {
    return normalizedTab;
  }

  const terminalEvent = terminalWorkflowEvent(normalizedTab.eventLog);

  if (terminalEvent?.kind === 'workflow_completed') {
    return {
      ...normalizedTab,
      runState: 'completed',
      validationState: 'valid',
      message: 'Workflow completed.',
      outputJson: eventOutputJson(terminalEvent) ?? normalizedTab.outputJson,
    };
  }

  if (terminalEvent?.kind === 'workflow_failed') {
    return {
      ...normalizedTab,
      runState: 'failed',
      message: terminalEvent.message ?? 'Workflow failed.',
    };
  }

  return {
    ...normalizedTab,
    runState: 'failed',
    message: 'Run connection was lost during page reload. Start a new run to continue.',
  };
}

function terminalWorkflowEvent(events: ExecutorEvent[]) {
  for (let eventIndex = events.length - 1; eventIndex >= 0; eventIndex -= 1) {
    const event = events[eventIndex];

    if (event.kind === 'workflow_completed' || event.kind === 'workflow_failed') {
      return event;
    }
  }

  return null;
}

function eventOutputJson(event: ExecutorEvent) {
  if (!isJsonObject(event.data) || !('output' in event.data)) {
    return null;
  }

  return JSON.stringify(event.data.output, null, 2);
}

function normalizePlaygroundView(value: unknown): WorkflowTab['activeView'] {
  if (value === 'runtime') {
    return 'runtime';
  }

  return 'workflow';
}

function fieldsToObject(value: unknown): Record<string, unknown> {
  if (!Array.isArray(value)) {
    return {};
  }

  const result: Record<string, unknown> = {};

  for (const field of value) {
    if (!isJsonObject(field) || typeof field.name !== 'string') {
      continue;
    }

    const fieldName = field.name.trim();

    if (!fieldName) {
      continue;
    }

    result[fieldName] = parseLegacyFieldValue(field);
  }

  return result;
}

function parseLegacyFieldValue(field: Record<string, unknown>): unknown {
  const fieldValue = typeof field.value === 'string' ? field.value : '';

  if (field.kind === 'number') {
    return Number(fieldValue);
  }

  if (field.kind === 'boolean') {
    return fieldValue === 'true';
  }

  if (field.kind === 'json') {
    return JSON.parse(fieldValue || 'null') as unknown;
  }

  return fieldValue;
}

function isJsonObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
