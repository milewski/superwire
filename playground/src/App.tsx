import { Braces, Copy, Moon, Pencil, Play, Plus, RefreshCcw, Square, Sun, Trash2, Workflow } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { Dialog, DialogClose, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { TooltipProvider } from '@/components/ui/tooltip';
import PanelCard from '@/components/panel-card';
import EventLog, { EventGroupingMode } from '@/components/playground/event-log';
import JsonRuntimeEditor from '@/components/playground/json-runtime-editor';
import OutputBox from '@/components/playground/output-box';
import RunStateBadge from '@/components/playground/run-state-badge';
import StatusPill from '@/components/playground/status-pill';
import ViewHeader from '@/components/playground/view-header';
import type { ExecutorEvent, PlaygroundView, WorkflowTab } from './types';
import WireEditor from './WireEditor';
import { workflowTemplates, type WorkflowTemplate } from './workflowTemplates';
import { createWorkflowTab, normalizeWorkflowTab, parseJsonObject, uniqueId } from './workflowState';

const tabsStorageKey = 'superwire.playground.tabs.v3';
const legacyTabsStorageKey = 'superwire.playground.tabs.v2';
const activeTabStorageKey = 'superwire.playground.activeTab.v3';
const legacyActiveTabStorageKey = 'superwire.playground.activeTab.v2';
const themeStorageKey = 'superwire.playground.theme';
const logoSource = `${import.meta.env.BASE_URL}logo.svg`;

export default function App() {
  const [tabs, setTabs] = useState<WorkflowTab[]>(() => [createWorkflowTab('Launch brief')]);
  const [activeTabId, setActiveTabId] = useState('');
  const [darkMode, setDarkMode] = useState(true);
  const [runtimeOpen, setRuntimeOpen] = useState(true);
  const [outputOpen, setOutputOpen] = useState(true);
  const [eventsOpen, setEventsOpen] = useState(true);
  const [eventGroupingMode, setEventGroupingMode] = useState<EventGroupingMode>(EventGroupingMode.Chronological);
  const [abortController, setAbortController] = useState<AbortController | null>(null);
  const [renameDialogTabId, setRenameDialogTabId] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState('');
  const validationDebounceTimeoutRef = useRef<number | null>(null);
  const activeTab = tabs.find((tab) => tab.id === activeTabId) ?? tabs[0];
  const canRun = activeTab?.runState !== 'running';
  const hasEditorMessageError = activeTab?.validationState === 'invalid' || activeTab?.runState === 'failed';
  const activeView: PlaygroundView = activeTab?.activeView ?? 'workflow';
  const shouldShowTemplatePicker = activeView === 'workflow' && (activeTab?.source.trim() ?? '') === '';

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

  useEffect(() => {
    document.documentElement.classList.toggle('dark', darkMode);
  }, [darkMode]);

  useEffect(() => {
    if (!activeTab) {
      return;
    }

    if (activeTab.runState === 'running') {
      return;
    }

    if (validationDebounceTimeoutRef.current !== null) {
      window.clearTimeout(validationDebounceTimeoutRef.current);
    }

    validationDebounceTimeoutRef.current = window.setTimeout(() => {
      void validateWorkflowByTabId(activeTab.id);
    }, 700);

    return () => {
      if (validationDebounceTimeoutRef.current !== null) {
        window.clearTimeout(validationDebounceTimeoutRef.current);
      }
    };
  }, [activeTab?.id, activeTab?.source]);

  function updateActiveTab(updater: (tab: WorkflowTab) => WorkflowTab) {
    setTabs((currentTabs) => currentTabs.map((tab) => (tab.id === activeTab?.id ? updater(tab) : tab)));
  }

  function updateTab(tabId: string, updater: (tab: WorkflowTab) => WorkflowTab) {
    setTabs((currentTabs) => currentTabs.map((tab) => (tab.id === tabId ? updater(tab) : tab)));
  }

  function setTabView(nextView: PlaygroundView) {
    updateActiveTab((tab) => ({ ...tab, activeView: nextView }));
  }

  function addTab() {
    const tab = createWorkflowTab(`Workflow ${tabs.length + 1}`);
    setTabs((currentTabs) => [...currentTabs, tab]);
    setActiveTabId(tab.id);
  }

  function duplicateTabById(tabId: string) {
    const sourceTab = tabs.find((tab) => tab.id === tabId);

    if (!sourceTab) {
      return;
    }

    const tab: WorkflowTab = {
      ...structuredClone(sourceTab),
      id: uniqueId(),
      name: `${sourceTab.name} copy`,
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

  function openRenameDialog(tabId: string) {
    const sourceTab = tabs.find((tab) => tab.id === tabId);

    if (!sourceTab) {
      return;
    }

    setRenameDraft(sourceTab.name);
    setRenameDialogTabId(tabId);
  }

  function closeRenameDialog() {
    setRenameDialogTabId(null);
    setRenameDraft('');
  }

  function submitRenameDialog() {
    const nextName = renameDraft.trim();

    if (!renameDialogTabId || !nextName) {
      return;
    }

    updateTab(renameDialogTabId, (tab) => ({ ...tab, name: nextName, updatedAt: Date.now() }));
    closeRenameDialog();
  }

  function requestBody(currentTab: WorkflowTab, includeInput: boolean) {
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
    await validateWorkflowByTabId(currentTab.id);
  }

  async function validateWorkflowByTabId(tabId: string) {
    const currentTab = tabs.find((tab) => tab.id === tabId);

    if (!currentTab) {
      return;
    }

    updateTab(currentTab.id, (tab) => ({ ...tab, validationState: 'running', message: 'Validating workflow...' }));

    try {
      const response = await fetch('/validate', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(requestBody(currentTab, false)),
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
        body: JSON.stringify(requestBody(currentTab, true)),
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

  function formatRuntimeJson(fieldName: 'inputJson' | 'secretsJson') {
    const currentTab = requireActiveTab(activeTab);

    try {
      const parsedValue = parseJsonObject(currentTab[fieldName], fieldName === 'inputJson' ? 'input' : 'secrets');
      const formattedJson = JSON.stringify(parsedValue, null, 2);

      updateTab(currentTab.id, (tab) => ({
        ...tab,
        [fieldName]: formattedJson,
        message: `${fieldName === 'inputJson' ? 'Input' : 'Secrets'} JSON formatted.`,
        updatedAt: Date.now(),
      }));
    } catch (error) {
      updateTab(currentTab.id, (tab) => ({
        ...tab,
        validationState: 'invalid',
        message: errorMessage(error),
      }));
    }
  }

  function applyWorkflowTemplate(template: WorkflowTemplate) {
    updateActiveTab((tab) => ({
      ...tab,
      source: template.source,
      inputJson: JSON.stringify(template.input, null, 2),
      secretsJson: JSON.stringify(template.secrets, null, 2),
      message: `Loaded template: ${template.name}.`,
      validationState: 'idle',
      runState: 'idle',
      outputJson: '',
      eventLog: [],
      updatedAt: Date.now(),
    }));
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
        <div className="playground">
          <section className="playground__frame">
            <div className="playground__main">
              <header className="playground__topbar">
                <div className="playground__brand">
                  <img src={logoSource} alt="Superwire" className="playground__logo" />
                </div>

                <div className="playground__topbar-actions">
                  <Button className="playground__theme-toggle" variant="ghost" size="icon-lg" aria-label="Toggle theme" onClick={() => setDarkMode((currentValue) => !currentValue)}>
                    {darkMode ? <Sun /> : <Moon />}
                  </Button>
                </div>
              </header>

              <Tabs value={activeTab?.id ?? ''} onValueChange={setActiveTabId} className="playground__tabs">
                <TabsList variant="line" className="h-auto flex-wrap justify-start gap-3 bg-transparent p-0">
                  {tabs.map((tab) => (
                    <div key={tab.id} className="playground-tabs__item">
                      <TabsTrigger value={tab.id} className="playground-tabs__trigger">
                        <span className="playground-tabs__dot" />
                        <span className="playground-tabs__title">{tab.name}</span>
                        <RunStateBadge state={tab.runState} />
                      </TabsTrigger>

                      <div className="playground-tabs__actions">
                        <Button className="playground-tabs__action" variant="ghost" size="icon-sm" aria-label={`Rename ${tab.name}`} onClick={() => openRenameDialog(tab.id)}>
                          <Pencil />
                        </Button>

                        <Button className="playground-tabs__action" variant="ghost" size="icon-sm" aria-label={`Duplicate ${tab.name}`} onClick={() => duplicateTabById(tab.id)}>
                          <Copy />
                        </Button>

                        <Button className="playground-tabs__action" variant="ghost" size="icon-sm" aria-label={`Close ${tab.name}`} onClick={() => closeTab(tab.id)}>
                          <Trash2 />
                        </Button>
                      </div>
                    </div>
                  ))}
                  <Button variant="outline" size="lg" className="playground-tabs__new" onClick={addTab}><Plus /> Workflow</Button>
                </TabsList>
              </Tabs>

              <div className="playground__canvas">
                {activeTab ? (
                  <section className="playground__content">
                    <div className="playground__controls">
                      <nav className="playground-mode-switch" aria-label="Playground mode">
                        <Button variant={activeView === 'workflow' ? 'secondary' : 'ghost'} size="lg" className="playground-mode-switch__button" onClick={() => setTabView('workflow')}><Workflow /> Workflow</Button>
                        <Button variant={activeView === 'runtime' ? 'secondary' : 'ghost'} size="lg" className="playground-mode-switch__button" onClick={() => setTabView('runtime')}><Braces /> Variables</Button>
                      </nav>

                      <div className="playground-actions">
                        <StatusPill state={activeTab.validationState} />
                        <Button variant="ghost" size="lg" onClick={formatWorkflow}><RefreshCcw /> Format</Button>
                        <Button variant="ghost" size="lg" onClick={validateWorkflow}>Validate</Button>
                        {activeTab.runState === 'running' ? (
                          <Button variant="destructive" size="lg" onClick={stopRun}><Square /> Stop</Button>
                        ) : (
                          <Button className="playground-actions__run" disabled={!canRun} size="lg" onClick={runWorkflow}><Play /> Run workflow</Button>
                        )}
                      </div>
                    </div>

                    {activeView === 'workflow' ? (
                      <section className="workflow-layout">
                        {shouldShowTemplatePicker ? (
                          <PanelCard title="Start from a template" description="Pick a fixture to quickly explore the DSL." className="template-picker" bodyClassName="template-picker__grid">
                              {workflowTemplates.map((template) => (
                                <Button key={template.id} variant="outline" className="template-picker__button" onClick={() => applyWorkflowTemplate(template)}>
                                  <span className="template-picker__name">{template.name}</span>
                                  <span className="template-picker__description">{template.description}</span>
                                </Button>
                              ))}
                          </PanelCard>
                        ) : null}

                        <div className="workflow-layout__top workflow-layout__top--single">
                          <Card className="workflow-editor">
                            <div className="workflow-editor__header panel-card__header">
                              <div className="panel-card__title-block">
                                <strong>{activeTab.name}</strong>
                              </div>
                            </div>
                            <WireEditor
                              key={activeTab.id}
                              value={activeTab.source}
                              documentId={activeTab.id}
                              darkMode={darkMode}
                              onChange={(source) => updateActiveTab((tab) => ({ ...tab, source, updatedAt: Date.now() }))}
                            />
                            <div className={hasEditorMessageError ? 'workflow-editor__message workflow-editor__message--error' : 'workflow-editor__message workflow-editor__message--neutral'}>
                              <span className="workflow-editor__message-line workflow-editor__message-line--full">{activeTab.message ?? 'Ready.'}</span>
                            </div>
                          </Card>
                        </div>

                        <div className="workflow-layout__bottom">
                          <PanelCard collapsible open={outputOpen} title="Output" description="Final workflow output payload." className="workflow-log-panel" bodyClassName="workflow-log-panel__body" onToggle={() => setOutputOpen((currentValue) => !currentValue)}>
                            <OutputBox runState={activeTab.runState} outputJson={activeTab.outputJson} />
                          </PanelCard>
                          <PanelCard collapsible open={eventsOpen} title="Server events" description={`${activeTab.eventLog.length} streamed events.`} className="workflow-log-panel" bodyClassName="workflow-log-panel__body" onToggle={() => setEventsOpen((currentValue) => !currentValue)}>
                            <EventLog events={activeTab.eventLog} eventGroupingMode={eventGroupingMode} onEventGroupingModeChange={setEventGroupingMode} />
                          </PanelCard>
                        </div>
                      </section>
                    ) : null}

                    {activeView === 'runtime' ? (
                      <section className="runtime-view">
                        <ViewHeader title="Variables" description="Edit workflow input and secrets as JSON objects. This view is intentionally wide so nested payloads stay readable." />
                        <PanelCard collapsible open={runtimeOpen} title="Input and secrets" description="Variables are sent with every validation and run request." onToggle={() => setRuntimeOpen((currentValue) => !currentValue)}>
                          <div className="runtime-variables runtime-variables--wide">
                            <JsonRuntimeEditor title="Input" value={activeTab.inputJson} validationError={jsonObjectValidationError(activeTab.inputJson)} onFormat={() => formatRuntimeJson('inputJson')} onChange={(inputJson) => updateActiveTab((tab) => ({ ...tab, inputJson, updatedAt: Date.now() }))} />
                            <JsonRuntimeEditor title="Secrets" secret value={activeTab.secretsJson} validationError={jsonObjectValidationError(activeTab.secretsJson)} onFormat={() => formatRuntimeJson('secretsJson')} onChange={(secretsJson) => updateActiveTab((tab) => ({ ...tab, secretsJson, updatedAt: Date.now() }))} />
                          </div>
                        </PanelCard>
                      </section>
                    ) : null}
                  </section>
                ) : null}
              </div>
            </div>
        </section>
      </div>
    </main>

      <Dialog open={renameDialogTabId !== null} onOpenChange={(open) => {
        if (!open) {
          closeRenameDialog();
        }
      }}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Rename workflow tab</DialogTitle>
            <DialogDescription>Use a short, clear name for this workflow tab.</DialogDescription>
          </DialogHeader>

          <form
            className="rename-dialog__form"
            onSubmit={(event) => {
              event.preventDefault();
              submitRenameDialog();
            }}
          >
            <input
              autoFocus
              value={renameDraft}
              onChange={(event) => setRenameDraft(event.target.value)}
              className="rename-dialog__input"
              placeholder="Workflow tab name"
            />

            <DialogFooter>
              <DialogClose asChild>
                <Button variant="outline" type="button">Cancel</Button>
              </DialogClose>
              <Button type="submit" disabled={!renameDraft.trim()}>Save name</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

    </TooltipProvider>
  );
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
