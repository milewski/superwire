import { json } from '@codemirror/lang-json';
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { tags } from '@lezer/highlight';
import { useEffect, useRef } from 'react';
import { cn } from '@/lib/utils';

type JsonCodeEditorProps = {
  value: string;
  readOnly?: boolean;
  className?: string;
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
  },
  '.cm-scroller': {
    fontFamily: 'var(--font-mono)',
    overflow: 'visible',
  },
  '.cm-focused': {
    outline: 'none',
  },
  '.cm-line': {
    padding: 0,
  },
  '.cm-cursor': {
    borderLeftColor: 'var(--editor-caret)',
  },
  '.cm-activeLine': {
    backgroundColor: 'color-mix(in srgb, var(--superwire-accent) 7%, transparent)',
  },
  '.cm-selectionBackground, .cm-content ::selection': {
    backgroundColor: 'color-mix(in srgb, var(--superwire-accent) 24%, transparent)',
  },
});

const jsonHighlightStyle = HighlightStyle.define([
  { tag: tags.propertyName, color: 'var(--json-key-color)' },
  { tag: tags.string, color: 'var(--json-string-color)' },
  { tag: tags.number, color: 'var(--json-number-color)' },
  { tag: [tags.bool, tags.null], color: 'var(--json-literal-color)', fontWeight: '600' },
  { tag: tags.punctuation, color: 'var(--json-punctuation-color)' },
]);

export default function JsonCodeEditor({ value, readOnly = false, className, onChange }: JsonCodeEditorProps) {
  const editorContainerElementRef = useRef<HTMLDivElement | null>(null);
  const editorViewRef = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);

  useEffect(() => {
    onChangeRef.current = onChange;
  }, [onChange]);

  useEffect(() => {
    const editorContainerElement = editorContainerElementRef.current;

    if (!editorContainerElement) {
      return undefined;
    }

    const extensions = [
      json(),
      syntaxHighlighting(jsonHighlightStyle),
      EditorView.lineWrapping,
      jsonEditorTheme,
      EditorState.readOnly.of(readOnly),
      EditorView.editable.of(!readOnly),
    ];

    extensions.push(
      EditorView.updateListener.of((update) => {
        if (!update.docChanged) {
          return;
        }

        onChangeRef.current?.(update.state.doc.toString());
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

    return () => {
      editorView.destroy();
      editorViewRef.current = null;
    };
  }, [readOnly]);

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
  }, [value]);

  return <div ref={editorContainerElementRef} className={cn('json-code-editor', readOnly ? 'json-code-editor-readonly' : null, className)} />;
}
