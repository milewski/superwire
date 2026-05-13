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
  if (event.message) {
    return event.message;
  }

  if (event.data === undefined) {
    return 'No payload.';
  }

  return JSON.stringify(event.data, null, 2);
}
