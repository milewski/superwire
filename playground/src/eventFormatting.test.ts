import { describe, expect, it } from 'vitest';
import { formatEventData, formatEventSummary } from './eventFormatting';
import { EventValueKind, ExecutorEventKind, McpOperation, type ExecutorEvent } from './types';

const sensitivePayloadSentinel = 'superwire-sensitive-payload-sentinel';

describe('event formatting', () => {
  it('preserves public MCP metadata without exposing legacy payload values', () => {
    const publicEvent: ExecutorEvent = {
      kind: ExecutorEventKind.McpCallCompleted,
      timestamp_ms: 1_750_000_000_000,
      data: {
        operation: McpOperation.Call,
        target_name: 'search',
        server_name: 'local',
        item_name: 'search',
        argument_names: ['api_key', 'query'],
        result_kind: EventValueKind.Object,
        item_count: 2,
        duration_ms: 5,
      },
    };
    const legacyUnsafeEvent = {
      ...publicEvent,
      data: {
        ...publicEvent.data,
        arguments: { api_key: sensitivePayloadSentinel },
        raw_result: sensitivePayloadSentinel,
      },
    } as ExecutorEvent;
    const formattedEvent = formatEventData(legacyUnsafeEvent);
    const formattedPayload = JSON.parse(formattedEvent) as Record<string, unknown>;
    const formattedData = formattedPayload.data as Record<string, unknown>;

    expect(formattedEvent).not.toContain(sensitivePayloadSentinel);
    expect(formattedData.argument_names).toEqual(['api_key', 'query']);
    expect(formattedData.result_kind).toBe('object');
    expect(formattedData.item_count).toBe(2);
  });

  it('summarizes public file metadata without a provider file capability', () => {
    const event: ExecutorEvent = {
      kind: ExecutorEventKind.AgentFileCreated,
      timestamp_ms: 1_750_000_000_000,
      agent_name: 'writer',
      data: {
        filename: 'report.txt',
        purpose: 'analysis',
        bytes: 128,
      },
    };

    expect(formatEventSummary(event)).toBe('Created file report.txt for analysis');
    expect(formatEventData(event)).not.toContain('file_id');
  });
});
