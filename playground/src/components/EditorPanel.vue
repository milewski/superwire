<script setup lang="ts">
import WireEditor from '../WireEditor.vue';

const workflowSource = defineModel<string>({ required: true });

defineProps<{
  title: string;
  darkMode: boolean;
}>();

defineEmits<{
  rename: [];
}>();
</script>

<template>
  <section class="editor-card">
    <div class="editor-card-header">
      <div>
        <h2>{{ title }}</h2>
        <p>Wire source. Larger editing surface with tab-local state.</p>
      </div>
      <button class="rename-button" type="button" @click="$emit('rename')">Rename</button>
    </div>
    <WireEditor v-model="workflowSource" :dark="darkMode" />
  </section>
</template>
