import { autocompletion, type Completion, type CompletionContext, type CompletionResult } from '@codemirror/autocomplete';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { bracketMatching, defaultHighlightStyle, foldGutter, foldService, indentOnInput, syntaxHighlighting } from '@codemirror/language';
import { type Diagnostic, linter, setDiagnostics } from '@codemirror/lint';
import { searchKeymap } from '@codemirror/search';
import { EditorState } from '@codemirror/state';
import { EditorView, highlightActiveLine, highlightActiveLineGutter, hoverTooltip, keymap, lineNumbers, tooltips } from '@codemirror/view';
import { useEffect, useRef, useState } from 'react';
import { wireLanguage } from './wireLanguage';

interface WireEditorProps {
  value: string;
  fullValue: string;
  documentId: string;
  documentOffset: number;
  darkMode: boolean;
  inputJson: string;
  secretsJson: string;
  jumpTarget: EditorJumpTarget | null;
  onChange: (value: string, cursorOffset: number) => void;
  onDefinitionJump: (position: LspPosition) => void;
}

interface JsonRpcResponse {
  id?: number;
  method?: string;
  result?: unknown;
  params?: unknown;
}

interface CompletionList {
  items: CompletionItem[];
}

interface CompletionItem {
  label: string;
  detail?: string;
  documentation?: string | { value?: string };
  insertText?: string;
  textEdit?: {
    range: LspRange;
    newText: string;
  };
}

interface LspDiagnostic {
  range: LspRange;
  severity?: number;
  message: string;
}

interface LspHover {
  contents?: string | { value?: string };
}

interface LspLocation {
  range: LspRange;
}

interface LspRange {
  start: LspPosition;
  end: LspPosition;
}

interface LspPosition {
  line: number;
  character: number;
}

type PendingRequest = {
  resolve: (value: unknown) => void;
  reject: (reason?: unknown) => void;
};

interface EditorJumpTarget {
  offset: number;
  sequence: number;
}

class WebSocketLanguageClient {
  private socket: WebSocket | null = null;
  private closed = false;
  private nextRequestId = 1;
  private pendingRequests = new Map<number, PendingRequest>();
  private openPromise: Promise<void> | null = null;
  private diagnosticListener: ((diagnostics: LspDiagnostic[]) => void) | null = null;

  constructor(private readonly endpoint: string) {}

  setDiagnosticListener(listener: (diagnostics: LspDiagnostic[]) => void) {
    this.diagnosticListener = listener;
  }

  async open() {
    if (this.socket?.readyState === WebSocket.OPEN) {
      return;
    }

    if (this.openPromise) {
      return this.openPromise;
    }

    this.openPromise = new Promise((resolve, reject) => {
      const socket = new WebSocket(this.endpoint);
      this.socket = socket;
      this.closed = false;

      socket.addEventListener('open', () => {
        if (this.closed) {
          socket.close();
        }

        resolve();
      }, { once: true });
      socket.addEventListener('error', () => {
        if (this.closed) {
          resolve();

          return;
        }

        reject(new Error('Unable to connect to Superwire LSP websocket.'));
      }, { once: true });
      socket.addEventListener('message', (event) => this.acceptMessage(event.data));
      socket.addEventListener('close', () => this.rejectPendingRequests());
    });

    await this.openPromise;

    await this.request('initialize', {
      capabilities: {},
      rootUri: null,
    });
    this.notify('initialized', {});
  }

  close() {
    this.closed = true;

    if (this.socket?.readyState === WebSocket.OPEN) {
      this.socket.close();
    }

    this.socket = null;
    this.openPromise = null;
    this.rejectPendingRequests();
  }

  async request(method: string, params: unknown): Promise<unknown> {
    await this.ensureOpenSocket();

    const requestId = this.nextRequestId;
    this.nextRequestId += 1;

    const requestPromise = new Promise<unknown>((resolve, reject) => {
      this.pendingRequests.set(requestId, { resolve, reject });
    });

    this.socket?.send(
      JSON.stringify({
        jsonrpc: '2.0',
        id: requestId,
        method,
        params,
      }),
    );

    return requestPromise;
  }

  async notify(method: string, params: unknown) {
    await this.ensureOpenSocket();

    this.socket?.send(
      JSON.stringify({
        jsonrpc: '2.0',
        method,
        params,
      }),
    );
  }

  notifyIfOpen(method: string, params: unknown) {
    if (this.socket?.readyState !== WebSocket.OPEN) {
      return;
    }

    this.socket.send(
      JSON.stringify({
        jsonrpc: '2.0',
        method,
        params,
      }),
    );
  }

  private async ensureOpenSocket() {
    if (this.socket?.readyState !== WebSocket.OPEN) {
      await this.open();
    }
  }

  private acceptMessage(rawMessage: unknown) {
    if (typeof rawMessage !== 'string') {
      return;
    }

    const message = JSON.parse(rawMessage) as JsonRpcResponse;

    if (typeof message.id === 'number') {
      const pendingRequest = this.pendingRequests.get(message.id);
      this.pendingRequests.delete(message.id);
      pendingRequest?.resolve(message.result);

      return;
    }

    if (message.method === 'textDocument/publishDiagnostics' && isRecord(message.params)) {
      const diagnostics = Array.isArray(message.params.diagnostics) ? (message.params.diagnostics as LspDiagnostic[]) : [];
      this.diagnosticListener?.(diagnostics);
    }
  }

  private rejectPendingRequests() {
    for (const pendingRequest of this.pendingRequests.values()) {
      pendingRequest.reject(new Error('Superwire LSP websocket closed.'));
    }

    this.pendingRequests.clear();
  }
}

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
  '.cm-tooltip': {
    backgroundColor: 'var(--popover)',
    border: '1px solid var(--border)',
    borderRadius: '10px',
    color: 'var(--foreground)',
  },
  '.cm-tooltip-autocomplete ul li[aria-selected]': {
    backgroundColor: 'var(--accent-soft)',
    color: 'var(--foreground)',
  },
  '.cm-line': {
    padding: '0 18px 0 6px',
  },
});

export default function WireEditor({
  value,
  fullValue,
  documentId,
  documentOffset,
  darkMode,
  inputJson,
  secretsJson,
  jumpTarget,
  onChange,
  onDefinitionJump,
}: WireEditorProps) {
  const editorElementRef = useRef<HTMLDivElement | null>(null);
  const editorViewRef = useRef<EditorView | null>(null);
  const languageClientRef = useRef<WebSocketLanguageClient | null>(null);
  const onChangeRef = useRef(onChange);
  const onDefinitionJumpRef = useRef(onDefinitionJump);
  const fullValueRef = useRef(fullValue);
  const documentOffsetRef = useRef(documentOffset);
  const visibleValueRef = useRef(value);
  const documentVersionRef = useRef(1);
  const didSaveDebounceTimeoutRef = useRef<number | null>(null);
  const diagnosticsRef = useRef<LspDiagnostic[]>([]);
  const [editorHeight, setEditorHeight] = useState(260);
  const documentUri = `file:///playground/${documentId}.wire`;

  onChangeRef.current = onChange;
  onDefinitionJumpRef.current = onDefinitionJump;
  fullValueRef.current = fullValue;
  documentOffsetRef.current = documentOffset;
  visibleValueRef.current = value;

  useEffect(() => {
    const endpoint = `${window.location.protocol === 'https:' ? 'wss' : 'ws'}://${window.location.host}/lsp`;
    const languageClient = new WebSocketLanguageClient(endpoint);
    const parentElement = editorElementRef.current;
    let disposed = false;

    if (!parentElement) {
      return undefined;
    }

    languageClientRef.current = languageClient;
    languageClient.setDiagnosticListener((diagnostics) => {
      diagnosticsRef.current = diagnostics;

      const editorView = editorViewRef.current;

      if (!editorView) {
        return;
      }

      editorView.dispatch(setDiagnostics(
        editorView.state,
        diagnostics.flatMap((diagnostic) => lspDiagnosticToCodeMirror(editorView.state, diagnostic, fullValueRef.current, documentOffsetRef.current)),
      ));
    });

    const editorView = new EditorView({
      state: createEditorState(
        value,
        documentUri,
        languageClient,
        onChangeRef,
        onDefinitionJumpRef,
        fullValueRef,
        documentOffsetRef,
        visibleValueRef,
        documentVersionRef,
        didSaveDebounceTimeoutRef,
        diagnosticsRef,
        setEditorHeight,
      ),
      parent: parentElement,
    });
    editorViewRef.current = editorView;
    updateEditorHeight(editorView, setEditorHeight);

    languageClient
      .open()
      .then(() => {
        if (disposed) {
          return undefined;
        }

        return languageClient.notify('textDocument/didOpen', {
          textDocument: {
            uri: documentUri,
            languageId: 'wire',
            version: documentVersionRef.current,
            text: fullValue,
          },
        }).then(() => notifyRuntimeValues(languageClient, documentUri, inputJson, secretsJson));
      })
      .catch(() => undefined);

    return () => {
      disposed = true;

      if (didSaveDebounceTimeoutRef.current !== null) {
        window.clearTimeout(didSaveDebounceTimeoutRef.current);
      }

      languageClient.notifyIfOpen('textDocument/didClose', { textDocument: { uri: documentUri } });
      languageClient.close();
      editorView.destroy();
      editorViewRef.current = null;
      languageClientRef.current = null;
    };
  }, [documentUri, darkMode]);

  useEffect(() => {
    const languageClient = languageClientRef.current;

    if (!languageClient) {
      return;
    }

    notifyRuntimeValues(languageClient, documentUri, inputJson, secretsJson);
  }, [documentUri, inputJson, secretsJson]);

  useEffect(() => {
    const editorView = editorViewRef.current;

    if (!editorView) {
      return;
    }

    const currentValue = editorView.state.doc.toString();

    if (value === currentValue) {
      return;
    }

    editorView.dispatch({
      changes: {
        from: 0,
        to: currentValue.length,
        insert: value,
      },
    });

    updateEditorHeight(editorView, setEditorHeight);
  }, [value]);

  useEffect(() => {
    const languageClient = languageClientRef.current;

    if (!languageClient) {
      return;
    }

    documentVersionRef.current += 1;
    languageClient.notifyIfOpen('textDocument/didChange', {
      textDocument: { uri: documentUri, version: documentVersionRef.current },
      contentChanges: [{ text: fullValue }],
    });
  }, [documentUri, fullValue]);

  useEffect(() => {
    const editorView = editorViewRef.current;

    if (!editorView || !jumpTarget) {
      return;
    }

    const targetOffset = Math.max(0, Math.min(jumpTarget.offset, editorView.state.doc.length));
    editorView.dispatch({
      selection: { anchor: targetOffset },
      effects: EditorView.scrollIntoView(targetOffset, { y: 'center' }),
    });
    editorView.focus();
  }, [jumpTarget?.sequence]);

  return <div ref={editorElementRef} className="wire-editor-shell flex-1 overflow-hidden bg-transparent" style={{ height: `${editorHeight}px` }} />;
}

function notifyRuntimeValues(languageClient: WebSocketLanguageClient, documentUri: string, inputJson: string, secretsJson: string) {
  const input = parseJsonObjectOrEmpty(inputJson);
  const secrets = parseJsonObjectOrEmpty(secretsJson);

  void languageClient.notify('superwire/runtimeValues', {
    textDocument: { uri: documentUri },
    input,
    secrets,
  });
}

function parseJsonObjectOrEmpty(jsonText: string) {
  try {
    const value = JSON.parse(jsonText);

    if (isRecord(value)) {
      return value;
    }
  } catch {
    return {};
  }

  return {};
}

function createEditorState(
  value: string,
  documentUri: string,
  languageClient: WebSocketLanguageClient,
  onChangeRef: React.MutableRefObject<(value: string, cursorOffset: number) => void>,
  onDefinitionJumpRef: React.MutableRefObject<(position: LspPosition) => void>,
  fullValueRef: React.MutableRefObject<string>,
  documentOffsetRef: React.MutableRefObject<number>,
  visibleValueRef: React.MutableRefObject<string>,
  documentVersionRef: React.MutableRefObject<number>,
  didSaveDebounceTimeoutRef: React.MutableRefObject<number | null>,
  diagnosticsRef: React.MutableRefObject<LspDiagnostic[]>,
  setEditorHeight: (height: number) => void,
) {
  return EditorState.create({
    doc: value,
    extensions: [
      lineNumbers(),
      foldGutter(),
      foldService.of(wireFoldRange),
      highlightActiveLine(),
      highlightActiveLineGutter(),
      bracketMatching(),
      indentOnInput(),
      syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
      wireLanguage(),
      history(),
      linter((editorView) => diagnosticsRef.current.flatMap((diagnostic) => (
        lspDiagnosticToCodeMirror(editorView.state, diagnostic, fullValueRef.current, documentOffsetRef.current)
      ))),
      autocompletion({ override: [lspCompletionSource(documentUri, languageClient, fullValueRef, documentOffsetRef)] }),
      hoverTooltip((_editorView: EditorView, position: number) => (
        lspHoverTooltip(position, documentUri, languageClient, fullValueRef, documentOffsetRef)
      )),
      keymap.of([indentWithTab, ...defaultKeymap, ...historyKeymap, ...searchKeymap]),
      tooltips({ parent: document.body }),
      baseTheme,
      EditorView.lineWrapping,
      EditorView.domEventHandlers({
        mousedown: (event, editorView) => handleDefinitionMouseDown(event, editorView, documentUri, languageClient, fullValueRef, documentOffsetRef, onDefinitionJumpRef),
      }),
      EditorView.updateListener.of((update) => {
        if (!update.docChanged) {
          return;
        }

        const nextValue = update.state.doc.toString();
        const cursorOffset = update.state.selection.main.head;
        const nextFullValue = fullValueForVisibleChange(fullValueRef.current, documentOffsetRef.current, visibleValueRef.current.length, nextValue);
        visibleValueRef.current = nextValue;
        fullValueRef.current = nextFullValue;
        onChangeRef.current(nextValue, cursorOffset);
        updateEditorHeight(update.view, setEditorHeight);
        documentVersionRef.current += 1;

        void languageClient.notify('textDocument/didChange', {
          textDocument: { uri: documentUri, version: documentVersionRef.current },
          contentChanges: [{ text: nextFullValue }],
        });

        if (didSaveDebounceTimeoutRef.current !== null) {
          window.clearTimeout(didSaveDebounceTimeoutRef.current);
        }

        didSaveDebounceTimeoutRef.current = window.setTimeout(() => {
          void languageClient.notify('textDocument/didSave', {
            textDocument: { uri: documentUri },
          });
        }, 700);
      }),
    ],
  });
}

function wireFoldRange(editorState: EditorState, lineStart: number) {
  const source = editorState.doc.toString();
  const openingBraceIndex = findOpeningBraceOnLine(source, lineStart);

  if (openingBraceIndex === null) {
    return null;
  }

  const closingBraceIndex = findMatchingClosingBrace(source, openingBraceIndex);

  if (closingBraceIndex === null) {
    return null;
  }

  const openingLine = editorState.doc.lineAt(openingBraceIndex);
  const closingLine = editorState.doc.lineAt(closingBraceIndex);

  if (openingLine.number === closingLine.number) {
    return null;
  }

  return {
    from: openingLine.to,
    to: closingLine.from,
  };
}

function findOpeningBraceOnLine(source: string, lineStart: number) {
  const lineEnd = source.indexOf('\n', lineStart);
  const searchEnd = lineEnd === -1 ? source.length : lineEnd;
  let insideString = false;
  let escaping = false;

  for (let characterIndex = lineStart; characterIndex < searchEnd; characterIndex += 1) {
    const character = source[characterIndex];
    const nextCharacter = source[characterIndex + 1];

    if (!insideString && character === '/' && nextCharacter === '/') {
      return null;
    }

    if (character === '"' && !escaping) {
      insideString = !insideString;
    }

    if (!insideString && character === '{') {
      return characterIndex;
    }

    escaping = character === '\\' && !escaping;
  }

  return null;
}

function findMatchingClosingBrace(source: string, openingBraceIndex: number) {
  let braceDepth = 0;
  let insideString = false;
  let escaping = false;
  let insideLineComment = false;

  for (let characterIndex = openingBraceIndex; characterIndex < source.length; characterIndex += 1) {
    const character = source[characterIndex];
    const nextCharacter = source[characterIndex + 1];

    if (insideLineComment) {
      if (character === '\n') {
        insideLineComment = false;
      }

      continue;
    }

    if (!insideString && character === '/' && nextCharacter === '/') {
      insideLineComment = true;
      characterIndex += 1;

      continue;
    }

    if (character === '"' && !escaping) {
      insideString = !insideString;
    }

    if (!insideString && character === '{') {
      braceDepth += 1;
    }

    if (!insideString && character === '}') {
      braceDepth -= 1;

      if (braceDepth === 0) {
        return characterIndex;
      }
    }

    escaping = character === '\\' && !escaping;
  }

  return null;
}

function updateEditorHeight(editorView: EditorView, setEditorHeight: (height: number) => void) {
  const contentHeight = editorView.contentHeight;
  const gutterPadding = 56;
  const nextHeight = Math.max(220, Math.min(740, contentHeight + gutterPadding));
  setEditorHeight(nextHeight);
}

function fullValueForVisibleChange(fullValue: string, documentOffset: number, previousVisibleLength: number, nextVisibleValue: string) {
  return `${fullValue.slice(0, documentOffset)}${nextVisibleValue}${fullValue.slice(documentOffset + previousVisibleLength)}`;
}

function handleDefinitionMouseDown(
  event: MouseEvent,
  editorView: EditorView,
  documentUri: string,
  languageClient: WebSocketLanguageClient,
  fullValueRef: React.MutableRefObject<string>,
  documentOffsetRef: React.MutableRefObject<number>,
  onDefinitionJumpRef: React.MutableRefObject<(position: LspPosition) => void>,
) {
  if (!event.ctrlKey && !event.metaKey) {
    return false;
  }

  const position = editorView.posAtCoords({ x: event.clientX, y: event.clientY });

  if (position === null) {
    return false;
  }

  event.preventDefault();

  void languageClient
    .request('textDocument/definition', {
      textDocument: { uri: documentUri },
      position: offsetToLspPosition(fullValueRef.current, documentOffsetRef.current + position),
    })
    .then((result) => {
      const locations = Array.isArray(result) ? (result as LspLocation[]) : [];
      const firstLocation = locations[0];

      if (!firstLocation) {
        return;
      }

      onDefinitionJumpRef.current(firstLocation.range.start);
    })
    .catch(() => undefined);

  return true;
}

function lspCompletionSource(
  documentUri: string,
  languageClient: WebSocketLanguageClient,
  fullValueRef: React.MutableRefObject<string>,
  documentOffsetRef: React.MutableRefObject<number>,
) {
  return async (completionContext: CompletionContext): Promise<CompletionResult | null> => {
    const word = completionContext.matchBefore(/[A-Za-z0-9_.]*/);

    if (!completionContext.explicit && (!word || word.from === word.to)) {
      return null;
    }

    const result = (await languageClient.request('textDocument/completion', {
      textDocument: { uri: documentUri },
      position: offsetToLspPosition(fullValueRef.current, documentOffsetRef.current + completionContext.pos),
    })) as CompletionList;
    const items = Array.isArray(result.items) ? result.items : [];

    return {
      from: completionResultFrom(completionContext, items, word, fullValueRef.current, documentOffsetRef.current),
      options: items.map((item) => completionItemToCodeMirror(item, fullValueRef.current, documentOffsetRef.current)),
    };
  };
}

function completionResultFrom(
  completionContext: CompletionContext,
  items: CompletionItem[],
  word: { from: number } | null,
  fullValue: string,
  documentOffset: number,
) {
  const firstTextEdit = items.find((item) => item.textEdit)?.textEdit;

  if (firstTextEdit) {
    return Math.max(0, lspPositionToOffset(fullValue, firstTextEdit.range.start) - documentOffset);
  }

  return word?.from ?? completionContext.pos;
}

async function lspHoverTooltip(
  position: number,
  documentUri: string,
  languageClient: WebSocketLanguageClient,
  fullValueRef: React.MutableRefObject<string>,
  documentOffsetRef: React.MutableRefObject<number>,
) {
  const result = (await languageClient.request('textDocument/hover', {
    textDocument: { uri: documentUri },
    position: offsetToLspPosition(fullValueRef.current, documentOffsetRef.current + position),
  })) as LspHover | null;
  const hoverText = hoverContentsToText(result?.contents);

  if (!hoverText) {
    return null;
  }

  return {
    pos: position,
    create: () => {
      const dom = document.createElement('div');
      dom.className = 'max-w-sm whitespace-pre-wrap p-3 text-xs leading-5';
      dom.textContent = hoverText;

      return { dom };
    },
  };
}

function completionItemToCodeMirror(item: CompletionItem, fullValue: string, documentOffset: number): Completion {
  const completion: Completion = {
    label: item.label,
    type: 'keyword',
    detail: item.detail,
    info: completionDocumentationToText(item.documentation),
  };

  if (item.textEdit) {
    completion.apply = (editorView) => {
      const editStartOffset = lspPositionToOffset(fullValue, item.textEdit!.range.start) - documentOffset;
      const editEndOffset = lspPositionToOffset(fullValue, item.textEdit!.range.end) - documentOffset;

      editorView.dispatch({
        changes: {
          from: Math.max(0, Math.min(editStartOffset, editorView.state.doc.length)),
          to: Math.max(0, Math.min(editEndOffset, editorView.state.doc.length)),
          insert: item.textEdit!.newText.replace(/\$1/g, ''),
        },
      });
    };
  } else if (item.insertText) {
    completion.apply = item.insertText.replace(/\$1/g, '');
  }

  return completion;
}

function lspDiagnosticToCodeMirror(editorState: EditorState, diagnostic: LspDiagnostic, fullValue: string, documentOffset: number): Diagnostic[] {
  const fullStartOffset = lspPositionToOffset(fullValue, diagnostic.range.start);
  const fullEndOffset = lspPositionToOffset(fullValue, diagnostic.range.end);
  const visibleStartOffset = Math.max(fullStartOffset - documentOffset, 0);
  const visibleEndOffset = Math.min(fullEndOffset - documentOffset, editorState.doc.length);

  if (visibleEndOffset < 0 || visibleStartOffset > editorState.doc.length) {
    return [];
  }

  return [{
    from: visibleStartOffset,
    to: Math.max(visibleStartOffset, visibleEndOffset),
    severity: diagnostic.severity === 1 ? 'error' : 'warning',
    message: diagnostic.message,
  }];
}

function offsetToLspPosition(source: string, offset: number): LspPosition {
  const safeOffset = Math.max(0, Math.min(offset, source.length));
  let line = 0;
  let lineStartOffset = 0;

  for (let characterIndex = 0; characterIndex < safeOffset; characterIndex += 1) {
    if (source[characterIndex] === '\n') {
      line += 1;
      lineStartOffset = characterIndex + 1;
    }
  }

  return { line, character: safeOffset - lineStartOffset };
}

function lspPositionToOffset(source: string, position: LspPosition): number {
  const lines = source.split('\n');
  const targetLineIndex = Math.min(Math.max(position.line, 0), Math.max(lines.length - 1, 0));
  let offset = 0;

  for (let lineIndex = 0; lineIndex < targetLineIndex; lineIndex += 1) {
    offset += (lines[lineIndex]?.length ?? 0) + 1;
  }

  return Math.min(offset + Math.max(position.character, 0), offset + (lines[targetLineIndex]?.length ?? 0));
}

function completionDocumentationToText(documentation: CompletionItem['documentation']) {
  if (typeof documentation === 'string') {
    return documentation;
  }

  return documentation?.value;
}

function hoverContentsToText(contents: LspHover['contents']) {
  if (typeof contents === 'string') {
    return contents;
  }

  return contents?.value;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}
