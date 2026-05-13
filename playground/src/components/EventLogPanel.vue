<script setup lang="ts">
import { computed } from 'vue';
import { eventTone, formatEventData } from '../eventFormatting';
import type { ExecutorEvent } from '../types';

const props = defineProps<{
  open: boolean;
  events: ExecutorEvent[];
}>();

defineEmits<{
  toggle: [];
}>();

const eventCounts = computed(() => {
  const counts = new Map<string, number>();

  for (const event of props.events) {
    counts.set(event.kind, (counts.get(event.kind) ?? 0) + 1);
  }

  return Array.from(counts.entries()).map(([kind, count]) => ({ kind, count }));
});
</script>

<template>
  <section class="event-panel">
    <button class="panel-toggle" type="button" @click="$emit('toggle')">
      <span>Server events</span>
      <span>{{ events.length }}</span>
    </button>
    <div v-if="open" class="panel-body">
      <div class="event-counts">
        <span v-for="eventCount in eventCounts" :key="eventCount.kind" :class="['event-chip', eventTone(eventCount.kind)]">
          {{ eventCount.kind }} {{ eventCount.count }}
        </span>
      </div>

      <div class="event-list">
        <article v-for="(event, eventIndex) in events" :key="`${event.kind}-${eventIndex}`" class="event-card">
          <div class="flex flex-wrap items-center gap-2">
            <span :class="['event-chip', eventTone(event.kind)]">{{ event.kind }}</span>
            <span v-if="event.agent_name" class="agent-chip">{{ event.agent_name }}</span>
          </div>
          <pre class="event-data">{{ formatEventData(event) }}</pre>
        </article>

        <div v-if="events.length === 0" class="empty-state">Run a workflow to stream executor events here.</div>
      </div>
    </div>
  </section>
</template>
