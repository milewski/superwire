import type { ExecutorEvent } from './types';

export function eventTone(kind: string) {
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
  return JSON.stringify(event, null, 2);
}

export function formatEventSummary(event: ExecutorEvent) {
  if (event.message) {
    return event.message;
  }

  if ((event.kind === 'agent_file_created' || event.kind === 'agent_file_deleted') && isRecord(event.data)) {
    const fileId = event.data.file_id;
    const filename = event.data.filename;
    const purpose = event.data.purpose;

    if (typeof fileId === 'string') {
      const action = event.kind === 'agent_file_created' ? 'Created' : 'Deleted';
      const fileNameText = typeof filename === 'string' ? ` ${filename}` : '';
      const purposeText = typeof purpose === 'string' ? ` for ${purpose}` : '';

      return `${action} file${fileNameText}: ${fileId}${purposeText}`;
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
  if (!isRecord(event.data)) {
    return null;
  }

  const durationMs = event.data.duration_ms;

  return typeof durationMs === 'number' ? durationMs : null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
