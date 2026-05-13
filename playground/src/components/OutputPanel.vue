<script setup lang="ts">
import type { RunState } from '../types';

defineProps<{
  open: boolean;
  runState: RunState;
  outputJson: string;
}>();

defineEmits<{
  toggle: [];
}>();
</script>

<template>
  <section class="side-panel">
    <button class="panel-toggle" type="button" @click="$emit('toggle')">
      <span>Output</span>
      <span :class="['status-pill', runState]">{{ runState }}</span>
    </button>
    <div v-if="open" class="panel-body">
      <pre class="output-box">{{ outputJson || 'No output yet.' }}</pre>
    </div>
  </section>
</template>
