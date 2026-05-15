import { Bot, Workflow } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible';
import JsonCodeEditor from '@/components/json-code-editor';
import { eventTone, formatEventData, formatEventDuration, formatEventTimestamp } from '../../eventFormatting';
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

export default function EventLog({ events, eventGroupingMode, onEventGroupingModeChange }: EventLogProps) {
  if (events.length === 0) {
    return <div className="empty-state compact">Run a workflow to stream server events.</div>;
  }

  const groupedEventBlocks = groupEventsForAgentView(events);

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

      <div className="space-y-2 pr-2">
        {eventGroupingMode === EventGroupingMode.Chronological
          ? events.map((event, eventIndex) => renderEventItem(event, eventIndex))
          : groupedEventBlocks.map((groupedEventBlock) => {
              if (groupedEventBlock.kind === 'agent') {
                return (
                  <section key={`agent-${groupedEventBlock.agentName}`} className="events-log__group">
                    <div className="events-log__group-header">
                      <span className="events-log__group-label">
                        <Bot />
                        <Badge variant="secondary">{groupedEventBlock.agentName}</Badge>
                      </span>
                      <span>{groupedEventBlock.events.length} events</span>
                    </div>

                    <div className="space-y-2">
                      {groupedEventBlock.events.map((eventWithIndex) => renderEventItem(eventWithIndex.event, eventWithIndex.eventIndex, false))}
                    </div>
                  </section>
                );
              }

              return (
                <section key={`workflow-${groupedEventBlock.blockIndex}`} className="events-log__group events-log__group--workflow">
                  <div className="events-log__group-header">
                    <span className="events-log__group-label">
                      <Workflow />
                      <Badge variant="secondary">workflow</Badge>
                    </span>
                    <span>{groupedEventBlock.events.length} events</span>
                  </div>

                  <div className="space-y-2">
                    {groupedEventBlock.events.map((eventWithIndex) => renderEventItem(eventWithIndex.event, eventWithIndex.eventIndex, false))}
                  </div>
                </section>
              );
            })}
      </div>
    </div>
  );
}

function renderEventItem(event: ExecutorEvent, eventIndex: number, showAgentBadge = true) {
  const eventTimestamp = formatEventTimestamp(event);
  const eventDuration = formatEventDuration(event);

  return (
    <Collapsible key={`${event.kind}-${eventIndex}-${event.agent_name ?? 'workflow'}`} defaultOpen={false} className="events-log__item">
      <CollapsibleTrigger asChild>
        <Button variant="ghost" className="events-log__item-trigger">
          <span className="events-log__item-meta">
            <span className="events-log__item-index">#{eventIndex + 1}</span>
            <Badge variant="outline" className={eventTone(event.kind)}>{event.kind}</Badge>
            {showAgentBadge && event.agent_name ? <Badge variant="secondary">{event.agent_name}</Badge> : null}
            {eventTimestamp ? <span className="events-log__item-time">{eventTimestamp}</span> : null}
            {eventDuration ? <Badge variant="outline" className="event-duration">{eventDuration}</Badge> : null}
          </span>
          <span className="events-log__item-summary">{event.message ?? 'View payload'}</span>
          <span className="events-log__item-expand">Expand</span>
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="events-log__item-content">
          <JsonCodeEditor value={formatEventData(event)} readOnly className="events-log__item-data" />
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
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
