<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import EditorPanel from './components/EditorPanel.vue';
import EventLogPanel from './components/EventLogPanel.vue';
import OutputPanel from './components/OutputPanel.vue';
import RuntimePanel from './components/RuntimePanel.vue';
import TopBar from './components/TopBar.vue';
import WorkflowTabs from './components/WorkflowTabs.vue';
import WorkspaceRail from './components/WorkspaceRail.vue';
import type { ExecutorEvent, RuntimeField, WorkflowTab } from './types';
import { createField, createWorkflowTab, fieldsToObject, uniqueId } from './workflowState';

const tabsStorageKey = 'superwire.playground.tabs.v2';
const activeTabStorageKey = 'superwire.playground.activeTab.v2';
const themeStorageKey = 'superwire.playground.theme';
const logoSource = `${import.meta.env.BASE_URL}logo.svg`;

const tabs = ref<WorkflowTab[]>([createWorkflowTab('Launch brief')]);
const activeTabId = ref(tabs.value[0]?.id ?? '');
const darkMode = ref(true);
const runtimeOpen = ref(true);
const outputOpen = ref(true);
const eventsOpen = ref(false);
const abortController = ref<AbortController | null>(null);

const activeTab = computed(() => tabs.value.find((tab) => tab.id === activeTabId.value) ?? tabs.value[0]);
const canRun = computed(() => activeTab.value?.runState !== 'running');
const workflowSource = computed({
  get: () => activeTab.value?.source ?? '',
  set: (source: string) => {
    if (!activeTab.value) {
      return;
    }

    activeTab.value.source = source;
    activeTab.value.updatedAt = Date.now();
  },
});

function addTab() {
  const tab = createWorkflowTab(`Workflow ${tabs.value.length + 1}`);
  tabs.value.push(tab);
  activeTabId.value = tab.id;
}

function duplicateTab() {
  const currentTab = activeTab.value;

  if (!currentTab) {
    return;
  }

  const tab: WorkflowTab = {
    ...structuredClone(currentTab),
    id: uniqueId(),
    name: `${currentTab.name} copy`,
    runState: 'idle',
    validationState: 'idle',
    message: 'Duplicated workflow.',
    outputJson: '',
    eventLog: [],
    updatedAt: Date.now(),
  };

  tabs.value.push(tab);
  activeTabId.value = tab.id;
}

function closeTab(tabId: string) {
  if (tabs.value.length === 1) {
    tabs.value = [createWorkflowTab('Launch brief')];
    activeTabId.value = tabs.value[0]?.id ?? '';

    return;
  }

  const closedIndex = tabs.value.findIndex((tab) => tab.id === tabId);
  tabs.value = tabs.value.filter((tab) => tab.id !== tabId);

  if (activeTabId.value === tabId) {
    activeTabId.value = tabs.value[Math.max(0, closedIndex - 1)]?.id ?? tabs.value[0]?.id ?? '';
  }
}

function renameActiveTab() {
  const currentTab = activeTab.value;

  if (!currentTab) {
    return;
  }

  const nextName = window.prompt('Workflow tab name', currentTab.name)?.trim();

  if (nextName) {
    currentTab.name = nextName;
  }
}

function addField(fields: RuntimeField[], field: Omit<RuntimeField, 'id'>) {
  fields.push(createField(field.name, field.value, field.kind));
}

function removeField(fields: RuntimeField[], fieldId: string) {
  const fieldIndex = fields.findIndex((field) => field.id === fieldId);

  if (fieldIndex >= 0) {
    fields.splice(fieldIndex, 1);
  }
}

function requestBody(includeInput: boolean) {
  const currentTab = requireActiveTab();
  const body: Record<string, unknown> = {
    workflow_source: currentTab.source,
    secrets: fieldsToObject(currentTab.secretFields, 'secrets'),
  };

  if (includeInput) {
    body.input = fieldsToObject(currentTab.inputFields, 'input');
    body.options = { include_events: true };
  }

  return body;
}

function requireActiveTab() {
  const currentTab = activeTab.value;

  if (!currentTab) {
    throw new Error('No active workflow tab.');
  }

  return currentTab;
}

async function validateWorkflow() {
  const currentTab = requireActiveTab();
  currentTab.validationState = 'running';
  currentTab.message = 'Validating workflow...';

  try {
    const response = await fetch('/validate', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(requestBody(false)),
    });
    const payload = await response.json();

    if (!response.ok || !payload.valid) {
      currentTab.validationState = 'invalid';
      currentTab.message = payload.details ?? payload.error ?? 'Workflow is invalid.';

      return;
    }

    currentTab.validationState = 'valid';
    currentTab.message = 'Workflow is valid.';
  } catch (error) {
    currentTab.validationState = 'invalid';
    currentTab.message = error instanceof Error ? error.message : String(error);
  }
}

async function formatWorkflow() {
  const currentTab = requireActiveTab();
  currentTab.message = 'Formatting workflow...';

  try {
    const response = await fetch('/format', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ workflow_source: currentTab.source }),
    });
    const payload = await response.json();

    if (!response.ok) {
      currentTab.validationState = 'invalid';
      currentTab.message = payload.error ?? 'Unable to format workflow.';

      return;
    }

    currentTab.source = payload.formatted_workflow_source;
    currentTab.validationState = 'valid';
    currentTab.message = 'Workflow formatted.';
  } catch (error) {
    currentTab.validationState = 'invalid';
    currentTab.message = error instanceof Error ? error.message : String(error);
  }
}

async function runWorkflow() {
  if (!canRun.value) {
    return;
  }

  const currentTab = requireActiveTab();
  currentTab.runState = 'running';
  currentTab.validationState = 'idle';
  currentTab.message = 'Running workflow...';
  currentTab.outputJson = '';
  currentTab.eventLog = [];
  eventsOpen.value = true;
  abortController.value = new AbortController();

  try {
    const response = await fetch('/execute/stream', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(requestBody(true)),
      signal: abortController.value.signal,
    });

    if (!response.ok || !response.body) {
      const payload = await response.json().catch(() => ({}));
      throw new Error(payload.error ?? `Request failed with ${response.status}`);
    }

    await readSseStream(response.body, currentTab.id);
    const failedEvent = currentTab.eventLog.find((event) => event.kind === 'workflow_failed');

    if (failedEvent) {
      currentTab.runState = 'failed';
      currentTab.message = failedEvent.message ?? 'Workflow failed.';

      return;
    }

    currentTab.runState = 'completed';
    currentTab.validationState = 'valid';
    currentTab.message = 'Workflow completed.';
  } catch (error) {
    if (error instanceof DOMException && error.name === 'AbortError') {
      currentTab.runState = 'idle';
      currentTab.message = 'Run cancelled.';

      return;
    }

    currentTab.runState = 'failed';
    currentTab.validationState = 'invalid';
    currentTab.message = error instanceof Error ? error.message : String(error);
  } finally {
    abortController.value = null;
  }
}

function stopRun() {
  abortController.value?.abort();
}

async function readSseStream(stream: ReadableStream<Uint8Array>, tabId: string) {
  const reader = stream.getReader();
  const decoder = new TextDecoder();
  let buffer = '';

  while (true) {
    const readResult = await reader.read();

    if (readResult.done) {
      break;
    }

    buffer += decoder.decode(readResult.value, { stream: true });
    const chunks = buffer.split('\n\n');
    buffer = chunks.pop() ?? '';

    for (const chunk of chunks) {
      acceptSseChunk(chunk, tabId);
    }
  }

  if (buffer.trim()) {
    acceptSseChunk(buffer, tabId);
  }
}

function acceptSseChunk(chunk: string, tabId: string) {
  const currentTab = tabs.value.find((tab) => tab.id === tabId);

  if (!currentTab) {
    return;
  }

  const dataLines = chunk
    .split('\n')
    .filter((line) => line.startsWith('data:'))
    .map((line) => line.slice('data:'.length).trimStart());

  if (dataLines.length === 0) {
    return;
  }

  const event = JSON.parse(dataLines.join('\n')) as ExecutorEvent;
  currentTab.eventLog.push(event);

  if (event.kind === 'workflow_completed' && isRecord(event.data) && 'output' in event.data) {
    currentTab.outputJson = JSON.stringify(event.data.output, null, 2);
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function restoreFromStorage() {
  darkMode.value = localStorage.getItem(themeStorageKey) !== 'light';

  const savedTabs = localStorage.getItem(tabsStorageKey);

  if (savedTabs) {
    tabs.value = JSON.parse(savedTabs) as WorkflowTab[];
  }

  if (tabs.value.length === 0) {
    tabs.value = [createWorkflowTab('Launch brief')];
  }

  activeTabId.value = localStorage.getItem(activeTabStorageKey) ?? tabs.value[0]?.id ?? '';

  if (!tabs.value.some((tab) => tab.id === activeTabId.value)) {
    activeTabId.value = tabs.value[0]?.id ?? '';
  }
}

function persistState() {
  localStorage.setItem(tabsStorageKey, JSON.stringify(tabs.value));
  localStorage.setItem(activeTabStorageKey, activeTabId.value);
  localStorage.setItem(themeStorageKey, darkMode.value ? 'dark' : 'light');
}

onMounted(restoreFromStorage);

watch([tabs, activeTabId, darkMode], persistState, { deep: true });
</script>

<template>
  <main :class="['playground-shell', { dark: darkMode }]">
    <section class="workspace-frame">
      <WorkspaceRail :logo-source="logoSource" @add-tab="addTab" />

      <div class="workspace-main">
        <TopBar
          :logo-source="logoSource"
          :message="activeTab?.message ?? 'Ready.'"
          :dark-mode="darkMode"
          :run-state="activeTab?.runState ?? 'idle'"
          @duplicate="duplicateTab"
          @format="formatWorkflow"
          @validate="validateWorkflow"
          @run="runWorkflow"
          @stop="stopRun"
          @toggle-theme="darkMode = !darkMode"
        />

        <WorkflowTabs
          :tabs="tabs"
          :active-tab-id="activeTabId"
          @select="activeTabId = $event"
          @close="closeTab"
          @add="addTab"
          @rename="renameActiveTab"
        />

        <section v-if="activeTab" class="content-grid">
          <div class="main-stack">
            <EditorPanel v-model="workflowSource" :title="activeTab.name" :dark-mode="darkMode" @rename="renameActiveTab" />
            <EventLogPanel :open="eventsOpen" :events="activeTab.eventLog" @toggle="eventsOpen = !eventsOpen" />
          </div>

          <aside class="side-stack">
            <RuntimePanel
              :open="runtimeOpen"
              :input-fields="activeTab.inputFields"
              :secret-fields="activeTab.secretFields"
              @toggle="runtimeOpen = !runtimeOpen"
              @add-input="addField(activeTab.inputFields, $event)"
              @add-secret="addField(activeTab.secretFields, $event)"
              @remove-input="removeField(activeTab.inputFields, $event)"
              @remove-secret="removeField(activeTab.secretFields, $event)"
            />
            <OutputPanel :open="outputOpen" :run-state="activeTab.runState" :output-json="activeTab.outputJson" @toggle="outputOpen = !outputOpen" />
          </aside>
        </section>
      </div>
    </section>
  </main>
</template>
