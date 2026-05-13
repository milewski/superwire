import { Braces, Copy, Moon, Play, Plus, RefreshCcw, ScrollText, Square, Sun, Trash2, Workflow } from 'lucide-react';
import type { ReactNode } from 'react';
import { useEffect, useState } from 'react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Textarea } from '@/components/ui/textarea';
import { TooltipProvider } from '@/components/ui/tooltip';
import { eventTone, formatEventData } from './eventFormatting';
import type { ExecutorEvent, RunState, ValidationState, WorkflowTab } from './types';
import WireEditor from './WireEditor';
import { createWorkflowTab, normalizeWorkflowTab, parseJsonObject, uniqueId } from './workflowState';

const tabsStorageKey = 'superwire.playground.tabs.v3';
const legacyTabsStorageKey = 'superwire.playground.tabs.v2';
const activeTabStorageKey = 'superwire.playground.activeTab.v3';
const legacyActiveTabStorageKey = 'superwire.playground.activeTab.v2';
const themeStorageKey = 'superwire.playground.theme';
const logoSource = `${import.meta.env.BASE_URL}logo.svg`;
type PlaygroundView = 'workflow' | 'runtime' | 'logs';

export default function App() {
  const [tabs, setTabs] = useState<WorkflowTab[]>(() => [createWorkflowTab('Launch brief')]);
  const [activeTabId, setActiveTabId] = useState('');
  const [darkMode, setDarkMode] = useState(true);
  const [runtimeOpen, setRuntimeOpen] = useState(true);
  const [outputOpen, setOutputOpen] = useState(true);
  const [eventsOpen, setEventsOpen] = useState(true);
  const [activeView, setActiveView] = useState<PlaygroundView>('workflow');
  const [abortController, setAbortController] = useState<AbortController | null>(null);
  const activeTab = tabs.find((tab) => tab.id === activeTabId) ?? tabs[0];
  const canRun = activeTab?.runState !== 'running';

  useEffect(() => {
    restoreFromStorage(setTabs, setActiveTabId, setDarkMode);
  }, []);

  useEffect(() => {
    if (tabs.length === 0 || !activeTabId) {
      return;
    }

    localStorage.setItem(tabsStorageKey, JSON.stringify(tabs));
    localStorage.setItem(activeTabStorageKey, activeTabId);
    localStorage.setItem(themeStorageKey, darkMode ? 'dark' : 'light');
  }, [tabs, activeTabId, darkMode]);

  function updateActiveTab(updater: (tab: WorkflowTab) => WorkflowTab) {
    setTabs((currentTabs) => currentTabs.map((tab) => (tab.id === activeTab?.id ? updater(tab) : tab)));
  }

  function updateTab(tabId: string, updater: (tab: WorkflowTab) => WorkflowTab) {
    setTabs((currentTabs) => currentTabs.map((tab) => (tab.id === tabId ? updater(tab) : tab)));
  }

  function addTab() {
    const tab = createWorkflowTab(`Workflow ${tabs.length + 1}`);
    setTabs((currentTabs) => [...currentTabs, tab]);
    setActiveTabId(tab.id);
  }

  function duplicateTab() {
    if (!activeTab) {
      return;
    }

    const tab: WorkflowTab = {
      ...structuredClone(activeTab),
      id: uniqueId(),
      name: `${activeTab.name} copy`,
      runState: 'idle',
      validationState: 'idle',
      message: 'Duplicated workflow.',
      outputJson: '',
      eventLog: [],
      updatedAt: Date.now(),
    };
    setTabs((currentTabs) => [...currentTabs, tab]);
    setActiveTabId(tab.id);
  }

  function closeTab(tabId: string) {
    if (tabs.length === 1) {
      const tab = createWorkflowTab('Launch brief');
      setTabs([tab]);
      setActiveTabId(tab.id);

      return;
    }

    const closedIndex = tabs.findIndex((tab) => tab.id === tabId);
    const nextTabs = tabs.filter((tab) => tab.id !== tabId);
    setTabs(nextTabs);

    if (activeTabId === tabId) {
      setActiveTabId(nextTabs[Math.max(0, closedIndex - 1)]?.id ?? nextTabs[0]?.id ?? '');
    }
  }

  function renameActiveTab() {
    if (!activeTab) {
      return;
    }

    const nextName = window.prompt('Workflow tab name', activeTab.name)?.trim();

    if (!nextName) {
      return;
    }

    updateActiveTab((tab) => ({ ...tab, name: nextName, updatedAt: Date.now() }));
  }

  function requestBody(includeInput: boolean) {
    const currentTab = requireActiveTab(activeTab);
    const body: Record<string, unknown> = {
      workflow_source: currentTab.source,
      secrets: parseJsonObject(currentTab.secretsJson, 'secrets'),
    };

    if (includeInput) {
      body.input = parseJsonObject(currentTab.inputJson, 'input');
      body.options = { include_events: true };
    }

    return body;
  }

  async function validateWorkflow() {
    const currentTab = requireActiveTab(activeTab);
    updateActiveTab((tab) => ({ ...tab, validationState: 'running', message: 'Validating workflow...' }));

    try {
      const response = await fetch('/validate', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(requestBody(false)),
      });
      const payload = await response.json();

      if (!response.ok || !payload.valid) {
        updateTab(currentTab.id, (tab) => ({ ...tab, validationState: 'invalid', message: payload.details ?? payload.error ?? 'Workflow is invalid.' }));

        return;
      }

      updateTab(currentTab.id, (tab) => ({ ...tab, validationState: 'valid', message: 'Workflow is valid.' }));
    } catch (error) {
      updateTab(currentTab.id, (tab) => ({ ...tab, validationState: 'invalid', message: errorMessage(error) }));
    }
  }

  async function formatWorkflow() {
    const currentTab = requireActiveTab(activeTab);
    updateActiveTab((tab) => ({ ...tab, message: 'Formatting workflow...' }));

    try {
      const response = await fetch('/format', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ workflow_source: currentTab.source }),
      });
      const payload = await response.json();

      if (!response.ok) {
        updateTab(currentTab.id, (tab) => ({ ...tab, validationState: 'invalid', message: payload.error ?? 'Unable to format workflow.' }));

        return;
      }

      updateTab(currentTab.id, (tab) => ({
        ...tab,
        source: payload.formatted_workflow_source,
        validationState: 'valid',
        message: 'Workflow formatted.',
        updatedAt: Date.now(),
      }));
    } catch (error) {
      updateTab(currentTab.id, (tab) => ({ ...tab, validationState: 'invalid', message: errorMessage(error) }));
    }
  }

  async function runWorkflow() {
    if (!canRun) {
      return;
    }

    const currentTab = requireActiveTab(activeTab);
    const nextAbortController = new AbortController();
    setAbortController(nextAbortController);
    setEventsOpen(true);
    updateActiveTab((tab) => ({
      ...tab,
      runState: 'running',
      validationState: 'idle',
      message: 'Running workflow...',
      outputJson: '',
      eventLog: [],
    }));

    try {
      const response = await fetch('/execute/stream', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(requestBody(true)),
        signal: nextAbortController.signal,
      });

      if (!response.ok || !response.body) {
        const payload = await response.json().catch(() => ({}));
        throw new Error(payload.error ?? `Request failed with ${response.status}`);
      }

      const events = await readSseStream(response.body, currentTab.id, acceptSseChunk);
      const failedEvent = events.find((event) => event.kind === 'workflow_failed');

      if (failedEvent) {
        updateTab(currentTab.id, (tab) => ({ ...tab, runState: 'failed', message: failedEvent.message ?? 'Workflow failed.' }));

        return;
      }

      updateTab(currentTab.id, (tab) => ({ ...tab, runState: 'completed', validationState: 'valid', message: 'Workflow completed.' }));
    } catch (error) {
      if (error instanceof DOMException && error.name === 'AbortError') {
        updateTab(currentTab.id, (tab) => ({ ...tab, runState: 'idle', message: 'Run cancelled.' }));

        return;
      }

      updateTab(currentTab.id, (tab) => ({ ...tab, runState: 'failed', validationState: 'invalid', message: errorMessage(error) }));
    } finally {
      setAbortController(null);
    }
  }

  function stopRun() {
    abortController?.abort();
  }

  function acceptSseChunk(chunk: string, tabId: string) {
    const event = parseSseChunk(chunk);

    if (!event) {
      return null;
    }

    updateTab(tabId, (tab) => ({
      ...tab,
      eventLog: [...tab.eventLog, event],
      outputJson: event.kind === 'workflow_completed' && isRecord(event.data) && 'output' in event.data ? JSON.stringify(event.data.output, null, 2) : tab.outputJson,
    }));

    return event;
  }

  return (
    <TooltipProvider>
      <main className={darkMode ? 'dark' : ''}>
        <div className="playground-shell">
          <section className="workspace-frame">
            <div className="workspace-main">
              <header className="topbar">
                <div className="brand-group">
                  <img src={logoSource} alt="Superwire" className="brand-logo" />
                  <div>
                    <p className="eyebrow">Playground</p>
                    <h1>Build, validate, and run wire workflows</h1>
                  </div>
                </div>

                <nav className="mode-tabs" aria-label="Playground mode">
                  <Button variant={activeView === 'workflow' ? 'secondary' : 'ghost'} size="lg" className="mode-tab" onClick={() => setActiveView('workflow')}><Workflow /> Workflow</Button>
                  <Button variant={activeView === 'runtime' ? 'secondary' : 'ghost'} size="lg" className="mode-tab" onClick={() => setActiveView('runtime')}><Braces /> Runtime</Button>
                  <Button variant={activeView === 'logs' ? 'secondary' : 'ghost'} size="lg" className="mode-tab" onClick={() => setActiveView('logs')}><ScrollText /> Logs</Button>
                </nav>

                <div className="topbar-actions">
                  <StatusPill state={activeTab?.validationState ?? 'idle'} />
                  <span className="message-line">{activeTab?.message ?? 'Ready.'}</span>
                  <Button variant="ghost" size="lg" onClick={duplicateTab}><Copy /> Duplicate</Button>
                  <Button variant="ghost" size="lg" onClick={formatWorkflow}><RefreshCcw /> Format</Button>
                  <Button variant="ghost" size="lg" onClick={validateWorkflow}>Validate</Button>
                  {activeTab?.runState === 'running' ? (
                    <Button variant="destructive" size="lg" onClick={stopRun}><Square /> Stop</Button>
                  ) : (
                    <Button disabled={!canRun} size="lg" onClick={runWorkflow}><Play /> Run</Button>
                  )}
                  <Button variant="outline" size="icon-lg" aria-label="Toggle theme" onClick={() => setDarkMode((currentValue) => !currentValue)}>
                    {darkMode ? <Sun /> : <Moon />}
                  </Button>
                </div>
              </header>

              <Tabs value={activeTab?.id ?? ''} onValueChange={setActiveTabId} className="tabbar">
                <TabsList variant="line" className="h-auto flex-wrap justify-start gap-3 bg-transparent p-0">
                  {tabs.map((tab) => (
                    <div key={tab.id} className="workflow-tab-shell">
                      <TabsTrigger value={tab.id} className="workflow-tab-trigger">
                        <span className="tab-dot" />
                        <span className="truncate">{tab.name}</span>
                        <RunStateBadge state={tab.runState} />
                      </TabsTrigger>
                      <Button variant="ghost" size="icon-sm" aria-label={`Close ${tab.name}`} onClick={() => closeTab(tab.id)}>
                        <Trash2 />
                      </Button>
                    </div>
                  ))}
                  <Button variant="outline" size="lg" className="new-tab" onClick={addTab}><Plus /> Workflow</Button>
                </TabsList>
              </Tabs>

              <div className="workflow-canvas">
                {activeTab ? (
                  <section className="content-stack">
                    {activeView === 'workflow' ? (
                      <Card className="editor-card">
                        <CardHeader className="editor-card-header">
                          <CardTitle>{activeTab.name}</CardTitle>
                          <CardDescription>Write Superwire DSL with syntax highlighting, LSP completions, hovers, and diagnostics.</CardDescription>
                          <CardAction>
                            <Button variant="ghost" size="lg" onClick={renameActiveTab}>Rename</Button>
                          </CardAction>
                        </CardHeader>
                        <WireEditor
                          key={activeTab.id}
                          value={activeTab.source}
                          documentId={activeTab.id}
                          darkMode={darkMode}
                          onChange={(source) => updateActiveTab((tab) => ({ ...tab, source, updatedAt: Date.now() }))}
                        />
                      </Card>
                    ) : null}

                    {activeView === 'runtime' ? (
                      <section className="view-stack">
                        <ViewHeader title="Runtime data" description="Edit workflow input and secrets as JSON objects. This view is intentionally wide so nested payloads stay readable." />
                        <CollapsiblePanel open={runtimeOpen} title="Runtime JSON" description="Input and secrets are sent with every validation and run request." onToggle={() => setRuntimeOpen((currentValue) => !currentValue)}>
                          <div className="runtime-json-grid runtime-json-grid-wide">
                            <JsonRuntimeEditor title="Input" value={activeTab.inputJson} onChange={(inputJson) => updateActiveTab((tab) => ({ ...tab, inputJson, updatedAt: Date.now() }))} />
                            <JsonRuntimeEditor title="Secrets" secret value={activeTab.secretsJson} onChange={(secretsJson) => updateActiveTab((tab) => ({ ...tab, secretsJson, updatedAt: Date.now() }))} />
                          </div>
                        </CollapsiblePanel>
                      </section>
                    ) : null}

                    {activeView === 'logs' ? (
                      <section className="view-stack">
                        <ViewHeader title="Logs and output" description="Inspect the final workflow output and the streamed server event timeline." />
                      <div className="bottom-grid">
                        <CollapsiblePanel open={outputOpen} title="Output" description="Final workflow output payload." onToggle={() => setOutputOpen((currentValue) => !currentValue)}>
                          <OutputBox runState={activeTab.runState} outputJson={activeTab.outputJson} />
                        </CollapsiblePanel>
                        <CollapsiblePanel open={eventsOpen} title="Server events" description={`${activeTab.eventLog.length} streamed events.`} onToggle={() => setEventsOpen((currentValue) => !currentValue)}>
                          <EventLog events={activeTab.eventLog} />
                        </CollapsiblePanel>
                      </div>
                      </section>
                    ) : null}
                  </section>
                ) : null}
              </div>
            </div>
        </section>
      </div>
    </main>
    </TooltipProvider>
  );
}

function ViewHeader({ title, description }: { title: string; description: string }) {
  return (
    <div className="view-header">
      <div>
        <h2>{title}</h2>
        <p>{description}</p>
      </div>
    </div>
  );
}

function CollapsiblePanel({ open, title, description, children, onToggle }: { open: boolean; title: string; description: string; children: ReactNode; onToggle: () => void }) {
  return (
    <Collapsible open={open} onOpenChange={onToggle} asChild>
      <Card className="side-panel">
        <CollapsibleTrigger asChild>
          <Button variant="ghost" className="panel-toggle">
            <span>
              <strong>{title}</strong>
              <small>{description}</small>
            </span>
            <span>{open ? 'Collapse' : 'Expand'}</span>
          </Button>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <CardContent className="panel-body">{children}</CardContent>
        </CollapsibleContent>
      </Card>
    </Collapsible>
  );
}

function JsonRuntimeEditor({ title, value, secret, onChange }: { title: string; value: string; secret?: boolean; onChange: (value: string) => void }) {
  const validationError = jsonObjectValidationError(value);

  return (
    <label className="json-editor-card">
      <span>
        <strong>{title}</strong>
        <small>{secret ? 'Sent as secrets.' : 'Sent as workflow input.'}</small>
      </span>
      <Textarea value={value} spellCheck={false} onChange={(event) => onChange(event.target.value)} />
      <em className={validationError ? 'json-error' : 'json-ok'}>{validationError ?? 'Valid JSON object'}</em>
    </label>
  );
}

function OutputBox({ runState, outputJson }: { runState: RunState; outputJson: string }) {
  if (!outputJson) {
    return <div className="empty-state compact">{runState === 'running' ? 'Waiting for workflow output...' : 'Run a workflow to see output.'}</div>;
  }

  return <pre className="output-box">{outputJson}</pre>;
}

function EventLog({ events }: { events: ExecutorEvent[] }) {
  if (events.length === 0) {
    return <div className="empty-state compact">Run a workflow to stream server events.</div>;
  }

  return (
    <ScrollArea className="event-list">
      <div className="space-y-3 pr-3">
        {events.map((event, eventIndex) => (
          <Card key={`${event.kind}-${eventIndex}`} size="sm" className="event-card">
            <CardHeader className="event-card-header">
              <Badge variant="outline" className={eventTone(event.kind)}>{event.kind}</Badge>
              {event.agent_name ? <Badge variant="secondary">{event.agent_name}</Badge> : null}
            </CardHeader>
            <CardContent>
              <pre className="event-data">{formatEventData(event)}</pre>
            </CardContent>
          </Card>
        ))}
      </div>
    </ScrollArea>
  );
}

function StatusPill({ state }: { state: ValidationState }) {
  return <Badge variant="outline" className={`status-pill ${state}`}>{state}</Badge>;
}

function RunStateBadge({ state }: { state: RunState }) {
  return <Badge variant="outline" className={`mini-status ${state}`}>{state}</Badge>;
}

function restoreFromStorage(setTabs: (tabs: WorkflowTab[]) => void, setActiveTabId: (tabId: string) => void, setDarkMode: (darkMode: boolean) => void) {
  setDarkMode(localStorage.getItem(themeStorageKey) !== 'light');

  const savedTabs = localStorage.getItem(tabsStorageKey) ?? localStorage.getItem(legacyTabsStorageKey);
  const restoredTabs = savedTabs ? (JSON.parse(savedTabs) as unknown[]).map(normalizeWorkflowTab) : [createWorkflowTab('Launch brief')];
  const tabs = restoredTabs.length > 0 ? restoredTabs : [createWorkflowTab('Launch brief')];
  const savedActiveTabId = localStorage.getItem(activeTabStorageKey) ?? localStorage.getItem(legacyActiveTabStorageKey);
  const activeTabId = tabs.some((tab) => tab.id === savedActiveTabId) ? savedActiveTabId! : tabs[0]?.id ?? '';

  setTabs(tabs);
  setActiveTabId(activeTabId);
}

async function readSseStream(stream: ReadableStream<Uint8Array>, tabId: string, acceptChunk: (chunk: string, tabId: string) => ExecutorEvent | null) {
  const reader = stream.getReader();
  const decoder = new TextDecoder();
  const events: ExecutorEvent[] = [];
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
      const event = acceptChunk(chunk, tabId);

      if (event) {
        events.push(event);
      }
    }
  }

  if (buffer.trim()) {
    const event = acceptChunk(buffer, tabId);

    if (event) {
      events.push(event);
    }
  }

  return events;
}

function parseSseChunk(chunk: string): ExecutorEvent | null {
  const dataLines = chunk
    .split('\n')
    .filter((line) => line.startsWith('data:'))
    .map((line) => line.slice('data:'.length).trimStart());

  if (dataLines.length === 0) {
    return null;
  }

  return JSON.parse(dataLines.join('\n')) as ExecutorEvent;
}

function jsonObjectValidationError(source: string) {
  try {
    parseJsonObject(source, 'value');

    return null;
  } catch (error) {
    return errorMessage(error);
  }
}

function requireActiveTab(activeTab: WorkflowTab | undefined) {
  if (!activeTab) {
    throw new Error('No active workflow tab.');
  }

  return activeTab;
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}
