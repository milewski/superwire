import { json } from '@codemirror/lang-json';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { bracketMatching, defaultHighlightStyle, foldGutter, HighlightStyle, indentOnInput, syntaxHighlighting } from '@codemirror/language';
import { searchKeymap } from '@codemirror/search';
import { EditorState } from '@codemirror/state';
import { EditorView, highlightActiveLine, highlightActiveLineGutter, keymap, lineNumbers } from '@codemirror/view';
import { tags } from '@lezer/highlight';
import { useEffect, useRef, useState } from 'react';
import { cn } from '@/lib/utils';

type JsonCodeEditorProps = {
  value: string;
  readOnly?: boolean;
  fullEditor?: boolean;
  uncappedHeight?: boolean;
  wrap?: boolean;
  className?: string;
  ariaLabel: string;
  onChange?: (value: string) => void;
};

const jsonEditorTheme = EditorView.theme({
  '&': {
    backgroundColor: 'transparent',
    color: 'var(--editor-foreground)',
    fontSize: '13px',
  },
  '.cm-content': {
    fontFamily: 'var(--font-mono)',
    minHeight: '0',
    padding: '0.75rem 0.9rem',
    caretColor: 'var(--editor-caret)',
  },
  '.cm-editor': {
    minHeight: 'inherit',
  },
  '.cm-scroller': {
    fontFamily: 'var(--font-mono)',
    minHeight: 'inherit',
    overflow: 'visible',
  },
  '.cm-focused': {
    outline: 'none',
  },
  '.cm-line': {
    padding: 0,
  },
  '.cm-cursor': {
    borderLeftColor: 'var(--editor-caret) !important',
  },
  '.cm-cursorLayer': {
    color: 'var(--editor-caret)',
  },
  '.cm-activeLine': {
    backgroundColor: 'color-mix(in srgb, var(--superwire-accent) 7%, transparent)',
  },
  '.cm-selectionBackground, .cm-content ::selection': {
    backgroundColor: 'color-mix(in srgb, var(--superwire-accent) 24%, transparent)',
  },
});

const fullJsonEditorTheme = EditorView.theme({
  '&': {
    backgroundColor: 'transparent',
    color: 'var(--editor-foreground)',
    fontSize: '14px',
    height: '100%',
  },
  '.cm-content': {
    caretColor: 'var(--editor-caret)',
    fontFamily: 'var(--font-mono)',
    minHeight: '0',
    padding: '22px 0',
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
  '.cm-line': {
    padding: '0 18px 0 6px',
  },
  '.cm-focused': {
    outline: 'none',
  },
  '.cm-cursor': {
    borderLeftColor: 'var(--editor-caret) !important',
  },
  '.cm-cursorLayer': {
    color: 'var(--editor-caret)',
  },
  '.cm-selectionBackground, .cm-content ::selection': {
    backgroundColor: 'color-mix(in srgb, var(--superwire-accent) 24%, transparent)',
  },
});

const uncappedFullJsonEditorTheme = EditorView.theme({
  '&': {
    height: 'auto',
  },
  '.cm-scroller': {
    overflow: 'visible',
  },
});

const jsonHighlightStyle = HighlightStyle.define([
  { tag: tags.propertyName, color: 'var(--json-key-color)' },
  { tag: tags.string, color: 'var(--json-string-color)' },
  { tag: tags.number, color: 'var(--json-number-color)' },
  { tag: [tags.bool, tags.null], color: 'var(--json-literal-color)', fontWeight: '600' },
  { tag: tags.punctuation, color: 'var(--json-punctuation-color)' },
]);

export default function JsonCodeEditor({ value, readOnly = false, fullEditor = false, uncappedHeight = false, wrap = true, className, ariaLabel, onChange }: JsonCodeEditorProps) {
  const editorContainerElementRef = useRef<HTMLDivElement | null>(null);
  const editorViewRef = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);
  const [editorHeight, setEditorHeight] = useState(260);

  useEffect(() => {
    onChangeRef.current = onChange;
  }, [onChange]);

  useEffect(() => {
    const editorContainerElement = editorContainerElementRef.current;

    if (!editorContainerElement) {
      return undefined;
    }

    const extensions = fullEditor
      ? fullJsonEditorExtensions(readOnly, uncappedHeight, wrap, ariaLabel)
      : compactJsonEditorExtensions(readOnly, wrap, ariaLabel);

    extensions.push(
      EditorView.updateListener.of((update) => {
        if (!update.docChanged) {
          return;
        }

        onChangeRef.current?.(update.state.doc.toString());
        updateJsonEditorHeight(update.view, fullEditor, uncappedHeight, setEditorHeight);
      }),
    );

    const editorView = new EditorView({
      parent: editorContainerElement,
      state: EditorState.create({
        doc: value,
        extensions,
      }),
    });

    editorViewRef.current = editorView;
    updateJsonEditorHeight(editorView, fullEditor, uncappedHeight, setEditorHeight);

    return () => {
      editorView.destroy();
      editorViewRef.current = null;
    };
  }, [readOnly, fullEditor, uncappedHeight, wrap, ariaLabel]);

  useEffect(() => {
    const editorView = editorViewRef.current;

    if (!editorView) {
      return;
    }

    const currentValue = editorView.state.doc.toString();

    if (currentValue === value) {
      return;
    }

    editorView.dispatch({
      changes: { from: 0, to: currentValue.length, insert: value },
    });
    updateJsonEditorHeight(editorView, fullEditor, uncappedHeight, setEditorHeight);
  }, [value, fullEditor, uncappedHeight]);

  function focusEditor() {
    editorViewRef.current?.focus();
  }

  return (
    <div
      ref={editorContainerElementRef}
      className={cn(
        'json-code-editor',
        fullEditor && !uncappedHeight ? 'json-code-editor--full wire-editor-shell flex-1 overflow-hidden bg-transparent' : null,
        fullEditor && uncappedHeight ? 'json-code-editor--full json-code-editor--uncapped wire-editor-shell bg-transparent' : null,
        readOnly ? 'json-code-editor-readonly' : null,
        className,
      )}
      style={fullEditor && !uncappedHeight ? { height: `${editorHeight}px` } : undefined}
      onMouseDown={readOnly ? undefined : focusEditor}
    />
  );
}

function compactJsonEditorExtensions(readOnly: boolean, wrap: boolean, ariaLabel: string) {
  const extensions = [
    json(),
    syntaxHighlighting(jsonHighlightStyle),
    jsonEditorTheme,
    EditorState.readOnly.of(readOnly),
    EditorView.editable.of(!readOnly),
    EditorView.contentAttributes.of({ 'aria-label': ariaLabel }),
  ];

  if (wrap) {
    extensions.push(EditorView.lineWrapping);
  }

  return extensions;
}

function fullJsonEditorExtensions(readOnly: boolean, uncappedHeight: boolean, wrap: boolean, ariaLabel: string) {
  const extensions = [
    lineNumbers(),
    foldGutter(),
    highlightActiveLine(),
    highlightActiveLineGutter(),
    bracketMatching(),
    indentOnInput(),
    syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
    syntaxHighlighting(jsonHighlightStyle),
    json(),
    history(),
    keymap.of([indentWithTab, ...defaultKeymap, ...historyKeymap, ...searchKeymap]),
    fullJsonEditorTheme,
    EditorState.readOnly.of(readOnly),
    EditorView.editable.of(!readOnly),
    EditorView.contentAttributes.of({ 'aria-label': ariaLabel }),
  ];

  if (uncappedHeight) {
    extensions.push(uncappedFullJsonEditorTheme);
  }

  if (wrap) {
    extensions.push(EditorView.lineWrapping);
  }

  return extensions;
}

function updateJsonEditorHeight(editorView: EditorView, fullEditor: boolean, uncappedHeight: boolean, setEditorHeight: (height: number) => void) {
  if (!fullEditor) {
    return;
  }

  if (uncappedHeight) {
    return;
  }

  const contentHeight = editorView.contentHeight;
  const verticalPadding = 56;
  const nextHeight = Math.max(220, Math.min(740, contentHeight + verticalPadding));
  setEditorHeight(nextHeight);
}
