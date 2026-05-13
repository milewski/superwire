<script setup lang="ts">
import type { RunState } from '../types';

defineProps<{
  logoSource: string;
  message: string;
  darkMode: boolean;
  runState: RunState;
}>();

defineEmits<{
  duplicate: [];
  format: [];
  validate: [];
  run: [];
  stop: [];
  toggleTheme: [];
}>();
</script>

<template>
  <header class="topbar">
    <div class="flex min-w-0 items-center gap-4">
      <img class="brand-logo" :src="logoSource" alt="Superwire" />
      <div class="min-w-0">
        <h1>Workflow Playground</h1>
        <p>{{ message }}</p>
      </div>
    </div>

    <div class="topbar-actions">
      <button class="ghost-action" type="button" @click="$emit('duplicate')">Duplicate</button>
      <button class="ghost-action" type="button" @click="$emit('format')">Format</button>
      <button class="ghost-action" type="button" @click="$emit('validate')">Validate</button>
      <button v-if="runState !== 'running'" class="run-action" type="button" @click="$emit('run')">Run</button>
      <button v-else class="danger-action" type="button" @click="$emit('stop')">Stop</button>
      <button class="icon-action" type="button" @click="$emit('toggleTheme')">{{ darkMode ? '☾' : '☀' }}</button>
    </div>
  </header>
</template>
