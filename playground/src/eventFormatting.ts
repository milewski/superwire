import { ExecutorEventKind, type ExecutionDiagnostic, type ExecutorEvent } from './types';

export function eventTone(kind: ExecutorEventKind) {
  if (kind === ExecutorEventKind.WorkflowCancelled || kind === ExecutorEventKind.AgentCancelled || kind === ExecutorEventKind.AgentLoopCancelled) {
    return 'event-cancelled';
  }

  if (kind === ExecutorEventKind.CacheDegraded) {
    return 'event-warning';
  }

  if (kind === ExecutorEventKind.StreamGap) {
    return 'event-gap';
  }

  if (kind.endsWith('_failed')) {
    return 'event-failed';
  }

  if (kind.endsWith('_completed')) {
    return 'event-completed';
  }

  if (kind.endsWith('_started')) {
    return 'event-started';
  }

  return 'event-planned';
}

export function formatEventData(event: ExecutorEvent) {
  return JSON.stringify({
    kind: event.kind,
    timestamp_ms: event.timestamp_ms,
    agent_name: event.agent_name,
    message: event.diagnostic?.message ?? event.message,
    diagnostic: event.diagnostic ? safeExecutionDiagnostic(event.diagnostic) : undefined,
    data: safeEventData(event.data),
  }, null, 2);
}

export function formatExecutionDiagnosticData(diagnostic: ExecutionDiagnostic) {
  return JSON.stringify(safeExecutionDiagnostic(diagnostic), null, 2);
}

export function formatEventSummary(event: ExecutorEvent) {
  if (event.diagnostic?.message || event.message) {
    return event.diagnostic?.message ?? event.message ?? 'View payload';
  }

  if ((event.kind === ExecutorEventKind.AgentFileCreated || event.kind === ExecutorEventKind.AgentFileDeleted) && isRecord(event.data)) {
    const filename = event.data.filename;
    const purpose = event.data.purpose;

    if (typeof filename === 'string') {
      const action = event.kind === ExecutorEventKind.AgentFileCreated ? 'Created' : 'Deleted';
      const purposeText = typeof purpose === 'string' ? ` for ${purpose}` : '';

      return `${action} file ${filename}${purposeText}`;
    }
  }

  return 'View payload';
}

export function formatEventTimestamp(event: ExecutorEvent) {
  if (typeof event.timestamp_ms !== 'number') {
    return null;
  }

  return new Date(event.timestamp_ms).toLocaleTimeString(undefined, {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    fractionalSecondDigits: 3,
  });
}

export function formatEventDuration(event: ExecutorEvent) {
  const durationMs = eventDurationMs(event);

  if (durationMs === null) {
    return null;
  }

  if (durationMs < 1000) {
    return `${durationMs}ms`;
  }

  return `${(durationMs / 1000).toFixed(2)}s`;
}

function eventDurationMs(event: ExecutorEvent) {
  if (!isRecord(event.data) || !('duration_ms' in event.data)) {
    return null;
  }

  const durationMilliseconds = event.data.duration_ms;

  return typeof durationMilliseconds === 'number' ? durationMilliseconds : null;
}

function safeExecutionDiagnostic(diagnostic: ExecutionDiagnostic): Record<string, unknown> {
  return {
    code: diagnostic.code,
    stage: diagnostic.stage,
    severity: diagnostic.severity,
    retryability: diagnostic.retryability,
    message: diagnostic.message,
    subject: { ...diagnostic.subject },
    retry_after_ms: diagnostic.retry_after_ms,
    cause: diagnostic.cause ? safeExecutionDiagnostic(diagnostic.cause) : undefined,
  };
}


function safeEventData(value: unknown, fieldName = ''): unknown {
  if (sensitiveFieldName(fieldName)) {
    return '<redacted>';
  }

  if (hiddenPayloadFieldName(fieldName)) {
    return '<hidden from event details>';
  }

  if (Array.isArray(value)) {
    return value.map((entry) => safeEventData(entry));
  }

  if (!isRecord(value)) {
    return value;
  }

  const safeEntries = Object.entries(value).map(([entryName, entryValue]) => [entryName, safeEventData(entryValue, entryName)]);

  return Object.fromEntries(safeEntries);
}

function sensitiveFieldName(fieldName: string) {
  return /secret|token|password|credential|authorization|api[_-]?key/i.test(fieldName);
}

function hiddenPayloadFieldName(fieldName: string) {
  return fieldName === 'arguments'
    || fieldName === 'params'
    || fieldName === 'error'
    || fieldName === 'input_schema'
    || fieldName === 'output'
    || fieldName === 'result'
    || fieldName === 'raw_result';
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
