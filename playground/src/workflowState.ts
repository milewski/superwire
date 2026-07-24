import { ExecutorEventKind, type ExecutionDiagnostic, type ExecutorEvent, type WorkflowTab } from './types';
import { createWorkflowCodeFragment, parseWorkflowSourceFragments, workflowSourceFromCodeFragments } from './workflowFragments';
import { parseWorkflowSourceMetadata } from './workflowMetadata';

export function uniqueId() {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }

  return `id-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

export function createWorkflowTab(name: string): WorkflowTab {
  const codeFragment = createWorkflowCodeFragment(name);

  return {
    id: uniqueId(),
    name,
    activeView: 'workflow',
    activeEditorView: 'code',
    source: '',
    codeFragments: [codeFragment],
    activeCodeFragmentId: codeFragment.id,
    codeFragmentsUseMarkers: false,
    inputJson: '{}',
    secretsJson: '{}',
    useCache: true,
    cacheKey: uniqueId(),
    validationState: 'idle',
    runState: 'idle',
    message: 'Ready.',
    outputJson: '',
    eventLog: [],
    runtimeDiagnostic: null,
    graphState: 'idle',
    graphMessage: 'Open the graph view to generate a visual workflow plan.',
    graphData: null,
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

  const source = typeof tab.source === 'string' ? tab.source : fallbackTab.source;
  const metadata = parseWorkflowSourceMetadata(source);
  const restoredName = metadata.name ?? (typeof tab.name === 'string' ? tab.name : fallbackTab.name);
  const restoredCodeFragments = normalizeWorkflowCodeFragments(tab.codeFragments, metadata.source, restoredName);
  const activeCodeFragmentId =
    typeof tab.activeCodeFragmentId === 'string' && restoredCodeFragments.fragments.some((fragment) => fragment.id === tab.activeCodeFragmentId)
      ? tab.activeCodeFragmentId
      : restoredCodeFragments.fragments[0]?.id ?? fallbackTab.activeCodeFragmentId;

  return {
    ...fallbackTab,
    ...tab,
    name: restoredName,
    source: workflowSourceFromCodeFragments(restoredCodeFragments.fragments, restoredCodeFragments.useMarkers),
    codeFragments: restoredCodeFragments.fragments,
    activeCodeFragmentId,
    codeFragmentsUseMarkers: restoredCodeFragments.useMarkers,
    activeView: normalizePlaygroundView(tab.activeView),
    activeEditorView: normalizeWorkflowEditorView(tab.activeEditorView),
    inputJson: metadata.inputJson ?? (typeof tab.inputJson === 'string' ? tab.inputJson : JSON.stringify(fieldsToObject(tab.inputFields), null, 2)),
    secretsJson: metadata.secretsJson ?? (typeof tab.secretsJson === 'string' ? tab.secretsJson : JSON.stringify(fieldsToObject(tab.secretFields), null, 2)),
    useCache: typeof tab.useCache === 'boolean' ? tab.useCache : fallbackTab.useCache,
    cacheKey: typeof tab.cacheKey === 'string' && tab.cacheKey.trim() ? tab.cacheKey : fallbackTab.cacheKey,
    eventLog: Array.isArray(tab.eventLog) ? tab.eventLog : [],
    runtimeDiagnostic: isJsonObject(tab.runtimeDiagnostic) ? (tab.runtimeDiagnostic as unknown as ExecutionDiagnostic) : null,
    graphState: normalizeGraphState(tab.graphState),
    graphMessage: typeof tab.graphMessage === 'string' ? tab.graphMessage : fallbackTab.graphMessage,
    graphData: isJsonObject(tab.graphData) ? (tab.graphData as unknown as WorkflowTab['graphData']) : null,
  };
}

function normalizeWorkflowCodeFragments(codeFragments: unknown, source: string, fallbackName: string) {
  if (!Array.isArray(codeFragments)) {
    return parseWorkflowSourceFragments(source, fallbackName);
  }

  const fragments = codeFragments.flatMap((fragment, fragmentIndex) => {
    if (!isJsonObject(fragment)) {
      return [];
    }

    return [
      {
        id: typeof fragment.id === 'string' ? fragment.id : uniqueId(),
        name: typeof fragment.name === 'string' && fragment.name.trim() ? fragment.name.trim() : `Fragment ${fragmentIndex + 1}`,
        source: typeof fragment.source === 'string' ? fragment.source : '',
      },
    ];
  });

  if (fragments.length === 0) {
    return parseWorkflowSourceFragments(source, fallbackName);
  }

  return {
    fragments,
    useMarkers: codeFragments.length > 1 || sourceContainsMarkers(source),
  };
}

function sourceContainsMarkers(source: string) {
  return parseWorkflowSourceFragments(source, 'source').useMarkers;
}

export function recoverWorkflowTabAfterReload(tab: unknown): WorkflowTab {
  const normalizedTab = normalizeWorkflowTab(tab);

  if (normalizedTab.runState !== 'running') {
    return normalizedTab;
  }

  const terminalEvent = terminalWorkflowEvent(normalizedTab.eventLog);

  if (terminalEvent?.kind === ExecutorEventKind.WorkflowCompleted) {
    return {
      ...normalizedTab,
      runState: 'completed',
      validationState: 'valid',
      message: 'Workflow completed.',
      outputJson: eventOutputJson(terminalEvent) ?? normalizedTab.outputJson,
    };
  }

  if (terminalEvent?.kind === ExecutorEventKind.WorkflowFailed) {
    return {
      ...normalizedTab,
      runState: 'failed',
      message: terminalEvent.diagnostic?.message ?? terminalEvent.message ?? 'Workflow failed.',
      runtimeDiagnostic: terminalEvent.diagnostic ?? normalizedTab.runtimeDiagnostic,
    };
  }

  if (terminalEvent?.kind === ExecutorEventKind.WorkflowCancelled) {
    return {
      ...normalizedTab,
      runState: 'cancelled',
      message: terminalEvent.diagnostic?.message ?? terminalEvent.message ?? 'Workflow cancelled.',
      runtimeDiagnostic: terminalEvent.diagnostic ?? normalizedTab.runtimeDiagnostic,
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

    if (event.kind === ExecutorEventKind.WorkflowCompleted || event.kind === ExecutorEventKind.WorkflowFailed || event.kind === ExecutorEventKind.WorkflowCancelled) {
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
  if (value === 'graph') {
    return 'graph';
  }

  return 'workflow';
}

function normalizeWorkflowEditorView(value: unknown): WorkflowTab['activeEditorView'] {
  if (value === 'input' || value === 'secrets') {
    return value;
  }

  return 'code';
}

function normalizeGraphState(value: unknown): WorkflowTab['graphState'] {
  if (value === 'loading' || value === 'failed' || value === 'ready') {
    return value;
  }

  return 'idle';
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
