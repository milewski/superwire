<script setup lang="ts">
defineProps<{
  logoSource: string;
}>();

defineEmits<{
  addTab: [];
}>();
</script>

<template>
  <aside class="workspace-rail" aria-label="Playground navigation">
    <img class="rail-logo" :src="logoSource" alt="Superwire" />
    <button class="rail-button active" type="button" title="Workflows">□</button>
    <button class="rail-button" type="button" title="Runtime">⌘</button>
    <button class="rail-button" type="button" title="Events">⌁</button>
    <button class="rail-button mt-auto" type="button" title="New workflow" @click="$emit('addTab')">＋</button>
  </aside>
</template>
