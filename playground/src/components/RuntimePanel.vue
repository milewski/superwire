<script setup lang="ts">
import RuntimeFields from '../RuntimeFields.vue';
import type { RuntimeField } from '../types';

defineProps<{
  open: boolean;
  inputFields: RuntimeField[];
  secretFields: RuntimeField[];
}>();

defineEmits<{
  toggle: [];
  addInput: [field: Omit<RuntimeField, 'id'>];
  addSecret: [field: Omit<RuntimeField, 'id'>];
  removeInput: [fieldId: string];
  removeSecret: [fieldId: string];
}>();
</script>

<template>
  <section class="side-panel">
    <button class="panel-toggle" type="button" @click="$emit('toggle')">
      <span>Runtime fields</span>
      <span>{{ open ? '−' : '+' }}</span>
    </button>
    <div v-if="open" class="panel-body">
      <RuntimeFields title="Inputs" :fields="inputFields" @add="$emit('addInput', $event)" @remove="$emit('removeInput', $event)" />
      <RuntimeFields title="Secrets" secret :fields="secretFields" @add="$emit('addSecret', $event)" @remove="$emit('removeSecret', $event)" />
    </div>
  </section>
</template>
