<script setup lang="ts">
import type { WorkflowTab } from '../types';

defineProps<{
  tabs: WorkflowTab[];
  activeTabId: string;
}>();

defineEmits<{
  select: [tabId: string];
  close: [tabId: string];
  add: [];
  rename: [];
}>();
</script>

<template>
  <nav class="tabbar" aria-label="Open workflows">
    <button
      v-for="tab in tabs"
      :key="tab.id"
      :class="['workflow-tab', { active: tab.id === activeTabId }]"
      type="button"
      @click="$emit('select', tab.id)"
    >
      <span class="tab-dot" />
      <span class="truncate" @dblclick.stop="$emit('rename')">{{ tab.name }}</span>
      <span :class="['mini-status', tab.runState]">{{ tab.runState }}</span>
      <span class="tab-close" @click.stop="$emit('close', tab.id)">×</span>
    </button>
    <button class="new-tab" type="button" @click="$emit('add')">＋ New tab</button>
  </nav>
</template>
