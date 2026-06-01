import { Bot, Workflow } from 'lucide-react';
import { useLayoutEffect, useMemo, useRef, useState } from 'react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import JsonCodeEditor from '@/components/json-code-editor';
import { eventTone, formatEventData, formatEventDuration, formatEventSummary, formatEventTimestamp } from '../../eventFormatting';
import type { ExecutorEvent } from '../../types';

export enum EventGroupingMode {
  Chronological = 'chronological',
  Agent = 'agent',
}

type EventLogProps = {
  events: ExecutorEvent[];
  eventGroupingMode: EventGroupingMode;
  onEventGroupingModeChange: (eventGroupingMode: EventGroupingMode) => void;
};

type EventWithIndex = {
  event: ExecutorEvent;
  eventIndex: number;
};

type AgentEventGroup = {
  kind: 'agent';
  agentName: string;
  events: EventWithIndex[];
};

type WorkflowEventGroup = {
  kind: 'workflow';
  blockIndex: number;
  events: EventWithIndex[];
};

type GroupedEventBlock = AgentEventGroup | WorkflowEventGroup;

type EventLogHeaderRow = {
  kind: 'header';
  key: string;
  label: string;
  count: number;
  icon: 'agent' | 'workflow';
};

type EventLogEventRow = {
  kind: 'event';
  key: string;
  event: ExecutorEvent;
  eventIndex: number;
  showAgentBadge: boolean;
};

type EventLogRow = EventLogHeaderRow | EventLogEventRow;

type EventLogVirtualRowsProps = {
  rows: EventLogRow[];
  selectedEventKey: string | null;
  onSelectEvent: (eventKey: string) => void;
};

const eventRowHeight = 54;
const headerRowHeight = 42;
const virtualListOverscanRows = 8;

export default function EventLog({ events, eventGroupingMode, onEventGroupingModeChange }: EventLogProps) {
  const [selectedEventKey, setSelectedEventKey] = useState<string | null>(null);
  const rows = useMemo(() => eventLogRows(events, eventGroupingMode), [events, eventGroupingMode]);
  const selectedEventRow = useMemo(() => selectedEventRowFromRows(rows, selectedEventKey), [rows, selectedEventKey]);

  if (events.length === 0) {
    return <div className="empty-state compact">Run a workflow to stream server events.</div>;
  }

  return (
    <div className="events-log">
      <div className="events-log__toolbar">
        <span className="events-log__toolbar-label">Group by</span>
        <div className="events-log__toolbar-toggle" role="tablist" aria-label="Event grouping mode">
          <Button
            type="button"
            size="sm"
            variant={eventGroupingMode === EventGroupingMode.Chronological ? 'secondary' : 'ghost'}
            className="events-log__toolbar-toggle-button"
            onClick={() => onEventGroupingModeChange(EventGroupingMode.Chronological)}
          >
            Chronological
          </Button>
          <Button
            type="button"
            size="sm"
            variant={eventGroupingMode === EventGroupingMode.Agent ? 'secondary' : 'ghost'}
            className="events-log__toolbar-toggle-button"
            onClick={() => onEventGroupingModeChange(EventGroupingMode.Agent)}
          >
            By agent
          </Button>
        </div>
      </div>

      <EventLogVirtualRows rows={rows} selectedEventKey={selectedEventRow?.key ?? selectedEventKey} onSelectEvent={setSelectedEventKey} />

      {selectedEventRow ? (
        <section className="events-log__details" aria-label="Selected event payload">
          <div className="events-log__details-header">
            <span className="events-log__details-title">#{selectedEventRow.eventIndex + 1}</span>
            <Badge variant="outline" className={eventTone(selectedEventRow.event.kind)}>{selectedEventRow.event.kind}</Badge>
            {selectedEventRow.event.agent_name ? <Badge variant="secondary">{selectedEventRow.event.agent_name}</Badge> : null}
          </div>
          <JsonCodeEditor value={formatEventData(selectedEventRow.event)} readOnly className="events-log__item-data" />
        </section>
      ) : null}
    </div>
  );
}

function EventLogVirtualRows({ rows, selectedEventKey, onSelectEvent }: EventLogVirtualRowsProps) {
  const viewportRef = useRef<HTMLDivElement | null>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewportHeight, setViewportHeight] = useState(360);
  const rowOffsets = useMemo(() => eventLogRowOffsets(rows), [rows]);
  const totalHeight = rowOffsets[rowOffsets.length - 1] ?? 0;
  const visibleRange = useMemo(() => visibleEventLogRange(rowOffsets, scrollTop, viewportHeight), [rowOffsets, scrollTop, viewportHeight]);
  const visibleRows = rows.slice(visibleRange.startIndex, visibleRange.endIndex);

  useLayoutEffect(() => {
    const viewportElement = viewportRef.current;

    if (!viewportElement) {
      return undefined;
    }

    const updateViewportHeight = () => setViewportHeight(viewportElement.clientHeight);
    updateViewportHeight();

    const resizeObserver = new ResizeObserver(updateViewportHeight);
    resizeObserver.observe(viewportElement);

    return () => resizeObserver.disconnect();
  }, []);

  return (
    <div
      ref={viewportRef}
      className="events-log__virtual-viewport"
      onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
    >
      <div className="events-log__virtual-spacer" style={{ height: totalHeight }}>
        {visibleRows.map((row, visibleRowIndex) => {
          const rowIndex = visibleRange.startIndex + visibleRowIndex;
          const rowTop = rowOffsets[rowIndex] ?? 0;

          return (
            <div key={row.key} className="events-log__virtual-row" style={{ transform: `translateY(${rowTop}px)` }}>
              {row.kind === 'header'
                ? <EventLogHeader row={row} />
                : (
                  <EventLogEvent
                    row={row}
                    selected={row.key === selectedEventKey}
                    onSelect={() => onSelectEvent(row.key)}
                  />
                )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

function EventLogHeader({ row }: { row: EventLogHeaderRow }) {
  const HeaderIcon = row.icon === 'agent' ? Bot : Workflow;

  return (
    <div className="events-log__group-header events-log__group-header--virtual">
      <span className="events-log__group-label">
        <HeaderIcon />
        <Badge variant="secondary">{row.label}</Badge>
      </span>
      <span>{row.count} events</span>
    </div>
  );
}

function EventLogEvent({ row, selected, onSelect }: { row: EventLogEventRow; selected: boolean; onSelect: () => void }) {
  const eventTimestamp = formatEventTimestamp(row.event);
  const eventDuration = formatEventDuration(row.event);

  return (
    <button type="button" className="events-log__item events-log__item-trigger" data-selected={selected ? 'true' : 'false'} onClick={onSelect}>
      <span className="events-log__item-meta">
        <span className="events-log__item-index">#{row.eventIndex + 1}</span>
        <Badge variant="outline" className={eventTone(row.event.kind)}>{row.event.kind}</Badge>
        {row.showAgentBadge && row.event.agent_name ? <Badge variant="secondary">{row.event.agent_name}</Badge> : null}
        {eventTimestamp ? <span className="events-log__item-time">{eventTimestamp}</span> : null}
        {eventDuration ? <Badge variant="outline" className="event-duration">{eventDuration}</Badge> : null}
      </span>
      <span className="events-log__item-summary">{formatEventSummary(row.event)}</span>
      <span className="events-log__item-expand">View</span>
    </button>
  );
}

function eventLogRows(events: ExecutorEvent[], eventGroupingMode: EventGroupingMode): EventLogRow[] {
  if (eventGroupingMode === EventGroupingMode.Chronological) {
    return events.map((event, eventIndex) => eventLogEventRow(event, eventIndex, true));
  }

  return groupEventsForAgentView(events).flatMap<EventLogRow>((groupedEventBlock) => {
    if (groupedEventBlock.kind === 'agent') {
      return [
        {
          kind: 'header',
          key: `agent-${groupedEventBlock.agentName}`,
          label: groupedEventBlock.agentName,
          count: groupedEventBlock.events.length,
          icon: 'agent',
        },
        ...groupedEventBlock.events.map((eventWithIndex) => eventLogEventRow(eventWithIndex.event, eventWithIndex.eventIndex, false)),
      ];
    }

    return [
      {
        kind: 'header',
        key: `workflow-${groupedEventBlock.blockIndex}`,
        label: 'workflow',
        count: groupedEventBlock.events.length,
        icon: 'workflow',
      },
      ...groupedEventBlock.events.map((eventWithIndex) => eventLogEventRow(eventWithIndex.event, eventWithIndex.eventIndex, false)),
    ];
  });
}

function eventLogEventRow(event: ExecutorEvent, eventIndex: number, showAgentBadge: boolean): EventLogEventRow {
  return {
    kind: 'event',
    key: `${event.kind}-${eventIndex}-${event.agent_name ?? 'workflow'}`,
    event,
    eventIndex,
    showAgentBadge,
  };
}

function selectedEventRowFromRows(rows: EventLogRow[], selectedEventKey: string | null) {
  if (!selectedEventKey) {
    return null;
  }

  const selectedRow = rows.find((row) => row.kind === 'event' && row.key === selectedEventKey);

  return selectedRow?.kind === 'event' ? selectedRow : null;
}

function eventLogRowOffsets(rows: EventLogRow[]) {
  const rowOffsets: number[] = [0];

  for (const row of rows) {
    rowOffsets.push(rowOffsets[rowOffsets.length - 1] + eventLogRowHeight(row));
  }

  return rowOffsets;
}

function eventLogRowHeight(row: EventLogRow) {
  return row.kind === 'header' ? headerRowHeight : eventRowHeight;
}

function visibleEventLogRange(rowOffsets: number[], scrollTop: number, viewportHeight: number) {
  const rowCount = Math.max(rowOffsets.length - 1, 0);
  const visibleStart = Math.max(0, scrollTop);
  const visibleEnd = visibleStart + viewportHeight;
  const startIndex = Math.max(0, rowIndexForOffset(rowOffsets, visibleStart) - virtualListOverscanRows);
  const endIndex = Math.min(rowCount, rowIndexForOffset(rowOffsets, visibleEnd) + virtualListOverscanRows + 1);

  return { startIndex, endIndex };
}

function rowIndexForOffset(rowOffsets: number[], offset: number) {
  let lowerBound = 0;
  let upperBound = Math.max(rowOffsets.length - 2, 0);

  while (lowerBound <= upperBound) {
    const midpoint = Math.floor((lowerBound + upperBound) / 2);
    const rowStart = rowOffsets[midpoint] ?? 0;
    const rowEnd = rowOffsets[midpoint + 1] ?? rowStart;

    if (offset < rowStart) {
      upperBound = midpoint - 1;

      continue;
    }

    if (offset >= rowEnd) {
      lowerBound = midpoint + 1;

      continue;
    }

    return midpoint;
  }

  return Math.max(0, Math.min(lowerBound, Math.max(rowOffsets.length - 2, 0)));
}

function groupEventsForAgentView(events: ExecutorEvent[]): GroupedEventBlock[] {
  const eventsByAgentName = new Map<string, EventWithIndex[]>();
  const displayedAgentNames = new Set<string>();
  const groupedEventBlocks: GroupedEventBlock[] = [];

  for (const [eventIndex, event] of events.entries()) {
    if (event.agent_name) {
      const agentName = event.agent_name;
      const existingAgentEvents = eventsByAgentName.get(agentName) ?? [];
      existingAgentEvents.push({ event, eventIndex });
      eventsByAgentName.set(agentName, existingAgentEvents);

      if (!displayedAgentNames.has(agentName)) {
        displayedAgentNames.add(agentName);
        groupedEventBlocks.push({
          kind: 'agent',
          agentName,
          events: existingAgentEvents,
        });
      }

      continue;
    }

    const lastGroupedEventBlock = groupedEventBlocks[groupedEventBlocks.length - 1];

    if (lastGroupedEventBlock?.kind === 'workflow') {
      lastGroupedEventBlock.events.push({ event, eventIndex });

      continue;
    }

    groupedEventBlocks.push({
      kind: 'workflow',
      blockIndex: groupedEventBlocks.length,
      events: [{ event, eventIndex }],
    });
  }

  return groupedEventBlocks;
}
