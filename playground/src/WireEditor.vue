<script setup lang="ts">
import { indentWithTab } from '@codemirror/commands';
import { bracketMatching, defaultHighlightStyle, foldGutter, indentOnInput, syntaxHighlighting } from '@codemirror/language';
import { searchKeymap } from '@codemirror/search';
import { EditorState } from '@codemirror/state';
import { EditorView, highlightActiveLine, highlightActiveLineGutter, keymap, lineNumbers } from '@codemirror/view';
import { onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { wireLanguage } from './wireLanguage';

const model = defineModel<string>({ required: true });

const props = defineProps<{
  dark: boolean;
}>();

const editorElement = ref<HTMLDivElement | null>(null);
let editorView: EditorView | null = null;

const baseTheme = EditorView.theme({
  '&': {
    backgroundColor: 'transparent',
    color: 'var(--editor-foreground)',
    fontSize: '14px',
    height: '100%',
  },
  '.cm-content': {
    caretColor: 'var(--editor-caret)',
    fontFamily: 'var(--font-mono)',
    minHeight: '760px',
    padding: '24px 0',
  },
  '.cm-gutters': {
    backgroundColor: 'transparent',
    border: 'none',
    color: 'var(--editor-muted)',
  },
  '.cm-activeLine': {
    backgroundColor: 'var(--editor-active-line)',
  },
  '.cm-activeLineGutter': {
    backgroundColor: 'var(--editor-active-line)',
    color: 'var(--editor-foreground)',
  },
  '.cm-scroller': {
    fontFamily: 'var(--font-mono)',
    overflow: 'auto',
  },
  '.cm-selectionBackground': {
    backgroundColor: 'var(--editor-selection) !important',
  },
  '.cm-line': {
    padding: '0 18px 0 6px',
  },
});

function createState(): EditorState {
  return EditorState.create({
    doc: model.value,
    extensions: [
      lineNumbers(),
      foldGutter(),
      highlightActiveLine(),
      highlightActiveLineGutter(),
      bracketMatching(),
      indentOnInput(),
      syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
      wireLanguage(),
      keymap.of([indentWithTab, ...searchKeymap]),
      baseTheme,
      EditorView.lineWrapping,
      EditorView.updateListener.of((update) => {
        if (!update.docChanged) {
          return;
        }

        const nextValue = update.state.doc.toString();

        if (nextValue !== model.value) {
          model.value = nextValue;
        }
      }),
    ],
  });
}

function recreateEditor() {
  const parent = editorElement.value;

  if (!parent) {
    return;
  }

  editorView?.destroy();
  editorView = new EditorView({
    state: createState(),
    parent,
  });
}

onMounted(recreateEditor);

watch(
  () => props.dark,
  () => recreateEditor(),
);

watch(model, (nextValue) => {
  if (!editorView) {
    return;
  }

  const currentValue = editorView.state.doc.toString();

  if (nextValue === currentValue) {
    return;
  }

  editorView.dispatch({
    changes: {
      from: 0,
      to: currentValue.length,
      insert: nextValue,
    },
  });
});

onBeforeUnmount(() => editorView?.destroy());
</script>

<template>
  <div ref="editorElement" class="min-h-[760px] flex-1 overflow-hidden bg-transparent" />
</template>
