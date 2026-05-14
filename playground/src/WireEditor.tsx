import { autocompletion, type Completion, type CompletionContext, type CompletionResult } from '@codemirror/autocomplete';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { bracketMatching, defaultHighlightStyle, foldGutter, indentOnInput, syntaxHighlighting } from '@codemirror/language';
import { type Diagnostic, linter, setDiagnostics } from '@codemirror/lint';
import { searchKeymap } from '@codemirror/search';
import { EditorState } from '@codemirror/state';
import { EditorView, highlightActiveLine, highlightActiveLineGutter, hoverTooltip, keymap, lineNumbers, tooltips } from '@codemirror/view';
import { useEffect, useRef, useState } from 'react';
import { wireLanguage } from './wireLanguage';

interface WireEditorProps {
  value: string;
  documentId: string;
  darkMode: boolean;
  onChange: (value: string) => void;
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

class WebSocketLanguageClient {
  private socket: WebSocket | null = null;
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

      socket.addEventListener('open', () => resolve(), { once: true });
      socket.addEventListener('error', () => reject(new Error('Unable to connect to Superwire LSP websocket.')), { once: true });
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
    this.socket?.close();
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

export default function WireEditor({ value, documentId, darkMode, onChange }: WireEditorProps) {
  const editorElementRef = useRef<HTMLDivElement | null>(null);
  const editorViewRef = useRef<EditorView | null>(null);
  const languageClientRef = useRef<WebSocketLanguageClient | null>(null);
  const onChangeRef = useRef(onChange);
  const didSaveDebounceTimeoutRef = useRef<number | null>(null);
  const diagnosticsRef = useRef<LspDiagnostic[]>([]);
  const [editorHeight, setEditorHeight] = useState(260);
  const documentUri = `file:///playground/${documentId}.wire`;

  onChangeRef.current = onChange;

  useEffect(() => {
    const endpoint = `${window.location.protocol === 'https:' ? 'wss' : 'ws'}://${window.location.host}/lsp`;
    const languageClient = new WebSocketLanguageClient(endpoint);
    const parentElement = editorElementRef.current;

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

      editorView.dispatch(setDiagnostics(editorView.state, diagnostics.map((diagnostic) => lspDiagnosticToCodeMirror(editorView.state, diagnostic))));
    });

    const editorView = new EditorView({
      state: createEditorState(value, documentUri, languageClient, onChangeRef, didSaveDebounceTimeoutRef, diagnosticsRef, setEditorHeight),
      parent: parentElement,
    });
    editorViewRef.current = editorView;
    updateEditorHeight(editorView, setEditorHeight);

    languageClient
      .open()
      .then(() =>
        languageClient.notify('textDocument/didOpen', {
          textDocument: {
            uri: documentUri,
            languageId: 'wire',
            version: 1,
            text: value,
          },
        }),
      )
      .catch(() => undefined);

    return () => {
      if (didSaveDebounceTimeoutRef.current !== null) {
        window.clearTimeout(didSaveDebounceTimeoutRef.current);
      }

      void languageClient.notify('textDocument/didClose', { textDocument: { uri: documentUri } }).catch(() => undefined);
      languageClient.close();
      editorView.destroy();
      editorViewRef.current = null;
      languageClientRef.current = null;
    };
  }, [documentUri, darkMode]);

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

  return <div ref={editorElementRef} className="wire-editor-shell flex-1 overflow-hidden bg-transparent" style={{ height: `${editorHeight}px` }} />;
}

function createEditorState(
  value: string,
  documentUri: string,
  languageClient: WebSocketLanguageClient,
  onChangeRef: React.MutableRefObject<(value: string) => void>,
  didSaveDebounceTimeoutRef: React.MutableRefObject<number | null>,
  diagnosticsRef: React.MutableRefObject<LspDiagnostic[]>,
  setEditorHeight: (height: number) => void,
) {
  return EditorState.create({
    doc: value,
    extensions: [
      lineNumbers(),
      foldGutter(),
      highlightActiveLine(),
      highlightActiveLineGutter(),
      bracketMatching(),
      indentOnInput(),
      syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
      wireLanguage(),
      history(),
      linter((editorView) => diagnosticsRef.current.map((diagnostic) => lspDiagnosticToCodeMirror(editorView.state, diagnostic))),
      autocompletion({ override: [lspCompletionSource(documentUri, languageClient)] }),
      hoverTooltip((editorView: EditorView, position: number) => lspHoverTooltip(editorView, position, documentUri, languageClient)),
      keymap.of([indentWithTab, ...defaultKeymap, ...historyKeymap, ...searchKeymap]),
      tooltips({ parent: document.body }),
      baseTheme,
      EditorView.lineWrapping,
      EditorView.updateListener.of((update) => {
        if (!update.docChanged) {
          return;
        }

        const nextValue = update.state.doc.toString();
        onChangeRef.current(nextValue);
        updateEditorHeight(update.view, setEditorHeight);

        void languageClient.notify('textDocument/didChange', {
          textDocument: { uri: documentUri, version: Date.now() },
          contentChanges: [{ text: nextValue }],
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

function updateEditorHeight(editorView: EditorView, setEditorHeight: (height: number) => void) {
  const contentHeight = editorView.contentHeight;
  const gutterPadding = 56;
  const nextHeight = Math.max(220, Math.min(740, contentHeight + gutterPadding));
  setEditorHeight(nextHeight);
}

function lspCompletionSource(documentUri: string, languageClient: WebSocketLanguageClient) {
  return async (completionContext: CompletionContext): Promise<CompletionResult | null> => {
    const word = completionContext.matchBefore(/[A-Za-z0-9_.]*/);

    if (!completionContext.explicit && (!word || word.from === word.to)) {
      return null;
    }

    const result = (await languageClient.request('textDocument/completion', {
      textDocument: { uri: documentUri },
      position: offsetToLspPosition(completionContext.state, completionContext.pos),
    })) as CompletionList;
    const items = Array.isArray(result.items) ? result.items : [];

    return {
      from: word?.from ?? completionContext.pos,
      options: items.map((item) => completionItemToCodeMirror(completionContext.state, item)),
    };
  };
}

async function lspHoverTooltip(editorView: EditorView, position: number, documentUri: string, languageClient: WebSocketLanguageClient) {
  const result = (await languageClient.request('textDocument/hover', {
    textDocument: { uri: documentUri },
    position: offsetToLspPosition(editorView.state, position),
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

function completionItemToCodeMirror(editorState: EditorState, item: CompletionItem): Completion {
  const completion: Completion = {
    label: item.label,
    type: 'keyword',
    detail: item.detail,
    info: completionDocumentationToText(item.documentation),
  };

  if (item.textEdit) {
    completion.apply = (editorView) => {
      editorView.dispatch({
        changes: {
          from: lspPositionToOffset(editorState, item.textEdit!.range.start),
          to: lspPositionToOffset(editorState, item.textEdit!.range.end),
          insert: item.textEdit!.newText.replace(/\$1/g, ''),
        },
      });
    };
  } else if (item.insertText) {
    completion.apply = item.insertText.replace(/\$1/g, '');
  }

  return completion;
}

function lspDiagnosticToCodeMirror(editorState: EditorState, diagnostic: LspDiagnostic): Diagnostic {
  return {
    from: lspPositionToOffset(editorState, diagnostic.range.start),
    to: lspPositionToOffset(editorState, diagnostic.range.end),
    severity: diagnostic.severity === 1 ? 'error' : 'warning',
    message: diagnostic.message,
  };
}

function offsetToLspPosition(editorState: EditorState, offset: number): LspPosition {
  const line = editorState.doc.lineAt(offset);

  return {
    line: line.number - 1,
    character: offset - line.from,
  };
}

function lspPositionToOffset(editorState: EditorState, position: LspPosition): number {
  const line = editorState.doc.line(Math.min(position.line + 1, editorState.doc.lines));

  return Math.min(line.from + position.character, line.to);
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
