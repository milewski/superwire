<script setup lang="ts">
import { ref } from 'vue';

type RuntimeFieldKind = 'string' | 'number' | 'boolean' | 'json';

interface RuntimeField {
  id: string;
  name: string;
  value: string;
  kind: RuntimeFieldKind;
}

defineProps<{
  title: string;
  fields: RuntimeField[];
  secret?: boolean;
}>();

const fieldKinds: RuntimeFieldKind[] = ['string', 'number', 'boolean', 'json'];
const emit = defineEmits<{
  add: [field: Omit<RuntimeField, 'id'>];
  remove: [fieldId: string];
}>();
const draftName = ref('');
const draftValue = ref('');
const draftKind = ref<RuntimeFieldKind>('string');

function addDraftField() {
  const name = draftName.value.trim();

  if (!name) {
    return;
  }

  emit('add', {
    name,
    value: draftValue.value,
    kind: draftKind.value,
  });
  draftName.value = '';
  draftValue.value = '';
  draftKind.value = 'string';
}
</script>

<template>
  <section class="runtime-group">
    <div class="runtime-group-header">
      <div>
        <h3>{{ title }}</h3>
        <p>{{ secret ? 'Sensitive values sent to secrets.' : 'Values sent to workflow input.' }}</p>
      </div>
      <span class="text-[10px] uppercase tracking-[0.16em] text-[#827a8e]">{{ fields.length }} set</span>
    </div>

    <form class="runtime-draft-card" @submit.prevent="addDraftField">
      <input v-model="draftName" class="field-input name" placeholder="name" spellcheck="false" />
      <select v-model="draftKind" class="field-input kind">
        <option v-for="fieldKind in fieldKinds" :key="fieldKind" :value="fieldKind">{{ fieldKind }}</option>
      </select>
      <select v-if="draftKind === 'boolean'" v-model="draftValue" class="field-input value">
        <option value="true">true</option>
        <option value="false">false</option>
      </select>
      <input
        v-else
        v-model="draftValue"
        :type="secret ? 'password' : 'text'"
        class="field-input value"
        :placeholder="draftKind === 'json' ? '{&quot;key&quot;: &quot;value&quot;}' : 'value'"
        spellcheck="false"
      />
      <button class="confirm-field" type="submit">Add</button>
    </form>

    <div class="runtime-fields">
      <article v-for="field in fields" :key="field.id" class="runtime-field-card">
        <input v-model="field.name" class="field-input name" placeholder="name" spellcheck="false" />
        <select v-model="field.kind" class="field-input kind">
          <option v-for="fieldKind in fieldKinds" :key="fieldKind" :value="fieldKind">{{ fieldKind }}</option>
        </select>
        <select v-if="field.kind === 'boolean'" v-model="field.value" class="field-input value">
          <option value="true">true</option>
          <option value="false">false</option>
        </select>
        <input
          v-else
          v-model="field.value"
          :type="secret ? 'password' : 'text'"
          class="field-input value"
          :placeholder="field.kind === 'json' ? '{&quot;key&quot;: &quot;value&quot;}' : 'value'"
          spellcheck="false"
        />
        <button class="remove-field" type="button" @click="emit('remove', field.id)">×</button>
      </article>

      <div v-if="fields.length === 0" class="empty-state compact">No fields yet. Add one to send runtime data.</div>
    </div>
  </section>
</template>
