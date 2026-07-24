import { describe, expect, it, vi } from 'vitest';
import {
  readWorkflowEventStream,
  WorkflowStreamGapError,
  WorkflowStreamLimitError,
  type UpdateTab,
  type WorkflowEventStreamDependencies,
} from './App';
import {
  ExecutorDiagnosticCode,
  ExecutorDiagnosticRetryability,
  ExecutorDiagnosticSeverity,
  ExecutorDiagnosticSubjectType,
  ExecutorEventKind,
  ExecutorStage,
  type ExecutionDiagnostic,
  type ExecutorEvent,
} from './types';

function eventFrame(eventIdentifier: string, event: ExecutorEvent) {
  return `id: ${eventIdentifier}\ndata: ${JSON.stringify(event)}\n\n`;
}

function streamResponse(source: string, runIdentifier: string) {
  return new Response(source, {
    headers: {
      'content-type': 'text/event-stream',
      'x-superwire-run-id': runIdentifier,
    },
  });
}

function parseExecutorEventFrame(frame: string) {
  const data = frame
    .split('\n')
    .filter((line) => line.startsWith('data:'))
    .map((line) => line.slice('data:'.length).trimStart())
    .join('\n');

  return data ? JSON.parse(data) as ExecutorEvent : null;
}

function workflowStartedEvent(timestampMilliseconds: number): ExecutorEvent {
  return {
    kind: ExecutorEventKind.WorkflowStarted,
    timestamp_ms: timestampMilliseconds,
    data: {},
  };
}

function workflowCompletedEvent(timestampMilliseconds: number, runName: string): ExecutorEvent {
  return {
    kind: ExecutorEventKind.WorkflowCompleted,
    timestamp_ms: timestampMilliseconds,
    data: { output: { run: runName }, duration_ms: 1 },
  };
}

function streamGapDiagnostic(runIdentifier: string, requestedAfter: string, oldestAvailable: string): ExecutionDiagnostic {
  return {
    code: ExecutorDiagnosticCode.StreamGap,
    stage: ExecutorStage.Stream,
    severity: ExecutorDiagnosticSeverity.Error,
    retryability: ExecutorDiagnosticRetryability.Safe,
    message: `History for ${runIdentifier} expired.`,
    subject: {
      type: ExecutorDiagnosticSubjectType.Stream,
      requested_after: requestedAfter,
      oldest_available: oldestAvailable,
    },
  };
}

function immediateStreamDependencies(reconnect: WorkflowEventStreamDependencies['reconnect']): WorkflowEventStreamDependencies {
  return {
    reconnect,
    waitForReconnect: vi.fn().mockResolvedValue(undefined),
  };
}

const unusedUpdateTab: UpdateTab = vi.fn();

describe('workflow event stream safety limits', () => {
  it('fails an oversized terminal data line once without reconnecting or requesting consent', async () => {
    const oversizedTerminalEvent = JSON.stringify({
      kind: ExecutorEventKind.WorkflowCompleted,
      timestamp_ms: 2,
      data: { output: { value: 'x'.repeat(300 * 1024) } },
    });
    const initialResponse = streamResponse(`id: 1\ndata: ${oversizedTerminalEvent}`, 'oversized-run');
    const reconnect = vi.fn<WorkflowEventStreamDependencies['reconnect']>();
    const confirmStreamGap = vi.fn().mockResolvedValue(true);
    const acceptChunk = vi.fn(parseExecutorEventFrame);

    const readPromise = readWorkflowEventStream(
      initialResponse,
      'oversized-tab',
      new AbortController().signal,
      acceptChunk,
      unusedUpdateTab,
      confirmStreamGap,
      immediateStreamDependencies(reconnect),
    );

    await expect(readPromise).rejects.toBeInstanceOf(WorkflowStreamLimitError);
    await expect(readPromise).rejects.toMatchObject({
      diagnostic: {
        code: ExecutorDiagnosticCode.InternalError,
        stage: ExecutorStage.Stream,
        retryability: ExecutorDiagnosticRetryability.Never,
      },
    });

    expect(acceptChunk).not.toHaveBeenCalled();
    expect(reconnect).not.toHaveBeenCalled();
    expect(confirmStreamGap).not.toHaveBeenCalled();
  });

  it('retains accepted identifiers and output before a fatal limit', async () => {
    const acceptedEvent = workflowCompletedEvent(1, 'accepted-run');
    const acceptedFrame = eventFrame('7', acceptedEvent);
    const oversizedData = `id: 8\ndata: ${'x'.repeat(300 * 1024)}`;
    const initialResponse = streamResponse(`${acceptedFrame}${acceptedFrame}${oversizedData}`, 'accepted-run');
    const reconnect = vi.fn<WorkflowEventStreamDependencies['reconnect']>();
    const confirmStreamGap = vi.fn().mockResolvedValue(true);
    const acceptedEvents: ExecutorEvent[] = [];
    let acceptedOutput: unknown = null;
    const acceptChunk = vi.fn((frame: string) => {
      const event = parseExecutorEventFrame(frame);

      if (event) {
        acceptedEvents.push(event);

        if (event.kind === ExecutorEventKind.WorkflowCompleted && event.data) {
          acceptedOutput = event.data.output;
        }
      }

      return event;
    });

    await expect(readWorkflowEventStream(
      initialResponse,
      'accepted-tab',
      new AbortController().signal,
      acceptChunk,
      unusedUpdateTab,
      confirmStreamGap,
      immediateStreamDependencies(reconnect),
    )).rejects.toMatchObject({
      diagnostic: {
        code: ExecutorDiagnosticCode.InternalError,
        subject: {
          type: ExecutorDiagnosticSubjectType.Stream,
          requested_after: '7',
        },
      },
    });

    expect(acceptedEvents).toEqual([acceptedEvent]);
    expect(acceptedOutput).toEqual({ run: 'accepted-run' });
    expect(acceptChunk).toHaveBeenCalledTimes(1);
    expect(reconnect).not.toHaveBeenCalled();
    expect(confirmStreamGap).not.toHaveBeenCalled();
  });
});

describe('workflow event stream recovery', () => {
  it('resumes a true server history gap from the oldest retained event', async () => {
    const diagnostic = streamGapDiagnostic('gap-run', '1', '5');
    const completedEvent = workflowCompletedEvent(5, 'gap-run');
    const reconnect = vi.fn<WorkflowEventStreamDependencies['reconnect']>()
      .mockRejectedValueOnce(new WorkflowStreamGapError(diagnostic))
      .mockResolvedValueOnce(streamResponse(eventFrame('5', completedEvent), 'gap-run'));
    const confirmStreamGap = vi.fn().mockResolvedValue(true);

    const events = await readWorkflowEventStream(
      streamResponse(eventFrame('1', workflowStartedEvent(1)), 'gap-run'),
      'gap-tab',
      new AbortController().signal,
      parseExecutorEventFrame,
      vi.fn(),
      confirmStreamGap,
      immediateStreamDependencies(reconnect),
    );

    expect(events.map((event) => event.kind)).toEqual([
      ExecutorEventKind.WorkflowStarted,
      ExecutorEventKind.WorkflowCompleted,
    ]);
    expect(reconnect.mock.calls.map(([, replayCursor]) => replayCursor)).toEqual(['1', '4']);
    expect(confirmStreamGap).toHaveBeenCalledOnce();
    expect(confirmStreamGap).toHaveBeenCalledWith(diagnostic, '4', true);
  });

  it('allows concurrent runs to resolve history consent independently', async () => {
    const firstDiagnostic = streamGapDiagnostic('first-run', '1', '5');
    const secondDiagnostic = streamGapDiagnostic('second-run', '1', '9');
    const firstReconnect = vi.fn<WorkflowEventStreamDependencies['reconnect']>()
      .mockRejectedValueOnce(new WorkflowStreamGapError(firstDiagnostic))
      .mockResolvedValueOnce(streamResponse(eventFrame('5', workflowCompletedEvent(5, 'first-run')), 'first-run'));
    const secondReconnect = vi.fn<WorkflowEventStreamDependencies['reconnect']>()
      .mockRejectedValueOnce(new WorkflowStreamGapError(secondDiagnostic))
      .mockResolvedValueOnce(streamResponse(eventFrame('9', workflowCompletedEvent(9, 'second-run')), 'second-run'));
    const confirmFirstGap = vi.fn().mockResolvedValue(true);
    const confirmSecondGap = vi.fn().mockResolvedValue(true);
    const firstRun = readWorkflowEventStream(
      streamResponse(eventFrame('1', workflowStartedEvent(1)), 'first-run'),
      'first-tab',
      new AbortController().signal,
      parseExecutorEventFrame,
      vi.fn(),
      confirmFirstGap,
      immediateStreamDependencies(firstReconnect),
    );
    const secondRun = readWorkflowEventStream(
      streamResponse(eventFrame('1', workflowStartedEvent(1)), 'second-run'),
      'second-tab',
      new AbortController().signal,
      parseExecutorEventFrame,
      vi.fn(),
      confirmSecondGap,
      immediateStreamDependencies(secondReconnect),
    );
    const [firstEvents, secondEvents] = await Promise.all([firstRun, secondRun]);

    expect(firstEvents.at(-1)?.data).toMatchObject({ output: { run: 'first-run' } });
    expect(secondEvents.at(-1)?.data).toMatchObject({ output: { run: 'second-run' } });
    expect(confirmFirstGap).toHaveBeenCalledOnce();
    expect(confirmSecondGap).toHaveBeenCalledOnce();
    expect(firstReconnect).toHaveBeenCalledTimes(2);
    expect(secondReconnect).toHaveBeenCalledTimes(2);
  });
});
