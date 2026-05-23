import { CheckCircle2, Copy, Database, DatabaseZap, Download, GitBranch, Moon, Pencil, Play, Plus, RefreshCcw, Square, Sun, Trash2, Workflow } from 'lucide-react';
import type { ReactElement } from 'react';
import { useEffect, useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { Dialog, DialogClose, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import JsonCodeEditor from '@/components/json-code-editor';
import PanelCard from '@/components/panel-card';
import EventLog, { EventGroupingMode } from '@/components/playground/event-log';
import OutputBox from '@/components/playground/output-box';
import PlaygroundTabChip from '@/components/playground/tab-chip';
import RunStateBadge from '@/components/playground/run-state-badge';
import WorkflowGraphView from '@/components/playground/workflow-graph-view';
import logoSource from '../../documentation/docs/public/logo-horizontal.svg';
import type { ExecutorEvent, PlaygroundView, WorkflowEditorView, WorkflowExecutionGraph, WorkflowTab } from './types';
import WireEditor from './WireEditor';
import { parseWorkflowSourceMetadata, workflowSourceWithMetadata, workflowSourceWithoutMetadata } from './workflowMetadata';
import {
  createWorkflowCodeFragment,
  parseWorkflowSourceFragments,
  preserveWorkflowCodeFragmentIdentities,
  sourceMapForFragment,
  sourceMapForFullOffset,
  uniqueCodeFragmentName,
  workflowSourceFromCodeFragments,
} from './workflowFragments';
import { workflowTemplates, type WorkflowTemplate } from './workflowTemplates';
import { createWorkflowTab, recoverWorkflowTabAfterReload, parseJsonObject, uniqueId } from './workflowState';

const tabsStorageKey = 'superwire.playground.tabs.v3';
const activeTabStorageKey = 'superwire.playground.activeTab.v3';
const themeStorageKey = 'superwire.playground.theme';
const runIdentifierHeader = 'x-superwire-run-id';
const streamReconnectDelayMilliseconds = 1000;

type RenameDialogTarget =
  | { kind: 'workflow'; tabId: string }
  | { kind: 'codeFragment'; tabId: string; fragmentId: string };

interface EditorJumpTarget {
  tabId: string;
  fragmentId: string;
  offset: number;
  sequence: number;
}

interface LspPosition {
  line: number;
  character: number;
}

export default function App() {
  const [tabs, setTabs] = useState<WorkflowTab[]>(() => [createWorkflowTab('Launch brief')]);
  const [activeTabId, setActiveTabId] = useState('');
  const [darkMode, setDarkMode] = useState(true);
  const [outputOpen, setOutputOpen] = useState(true);
  const [eventsOpen, setEventsOpen] = useState(true);
  const [eventGroupingMode, setEventGroupingMode] = useState<EventGroupingMode>(EventGroupingMode.Chronological);
  const [abortController, setAbortController] = useState<AbortController | null>(null);
  const [currentRunIdentifier, setCurrentRunIdentifier] = useState<string | null>(null);
  const [renameDialogTarget, setRenameDialogTarget] = useState<RenameDialogTarget | null>(null);
  const [renameDraft, setRenameDraft] = useState('');
  const [draggedTabId, setDraggedTabId] = useState<string | null>(null);
  const [dragOverTabId, setDragOverTabId] = useState<string | null>(null);
  const [draggedCodeFragmentId, setDraggedCodeFragmentId] = useState<string | null>(null);
  const [dragOverCodeFragmentId, setDragOverCodeFragmentId] = useState<string | null>(null);
  const [editorJumpTarget, setEditorJumpTarget] = useState<EditorJumpTarget | null>(null);
  const [playgroundControlsSentinelElement, setPlaygroundControlsSentinelElement] = useState<HTMLDivElement | null>(null);
  const [playgroundControlsStuck, setPlaygroundControlsStuck] = useState(false);
  const validationDebounceTimeoutRef = useRef<number | null>(null);
  const graphDebounceTimeoutRef = useRef<number | null>(null);
  const activeTab = tabs.find((tab) => tab.id === activeTabId) ?? tabs[0];
  const canRun = activeTab?.runState !== 'running';
  const activeView: PlaygroundView = activeTab?.activeView ?? 'workflow';
  const activeEditorView: WorkflowEditorView = activeTab?.activeEditorView ?? 'code';
  const shouldShowTemplatePicker = activeView === 'workflow' && activeEditorView === 'code' && (activeTab?.source.trim() ?? '') === '';
  const activeJsonValidationError = activeTab ? editorJsonValidationError(activeTab, activeEditorView) : null;
  const editorMessageTone = activeJsonValidationError ? 'error' : resolveEditorMessageTone(activeTab);
  const editorStateTone = activeTab ? workflowEditorTone(activeTab) : 'neutral';
  const editorMessage = activeJsonValidationError ?? activeTab?.message ?? 'Ready.';
  const activeCodeFragment = activeTab?.codeFragments.find((fragment) => fragment.id === activeTab.activeCodeFragmentId) ?? activeTab?.codeFragments[0];
  const activeCodeFragmentSourceMap = activeTab && activeCodeFragment
    ? sourceMapForFragment(activeTab.codeFragments, activeTab.codeFragmentsUseMarkers, activeCodeFragment.id)
    : null;
  const activeEditorJumpTarget =
    activeTab && activeCodeFragment && editorJumpTarget?.tabId === activeTab.id && editorJumpTarget.fragmentId === activeCodeFragment.id
      ? editorJumpTarget
      : null;

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
    if (!playgroundControlsSentinelElement) {
      setPlaygroundControlsStuck(false);

      return;
    }

    let animationFrameIdentifier: number | null = null;

    const updatePlaygroundControlsStuck = () => {
      animationFrameIdentifier = null;
      setPlaygroundControlsStuck(playgroundControlsSentinelElement.getBoundingClientRect().top < 0);
    };

    const requestPlaygroundControlsStuckUpdate = () => {
      if (animationFrameIdentifier !== null) {
        return;
      }

      animationFrameIdentifier = window.requestAnimationFrame(updatePlaygroundControlsStuck);
    };

    updatePlaygroundControlsStuck();
    window.addEventListener('scroll', requestPlaygroundControlsStuckUpdate, { passive: true });
    window.addEventListener('resize', requestPlaygroundControlsStuckUpdate);

    return () => {
      if (animationFrameIdentifier !== null) {
        window.cancelAnimationFrame(animationFrameIdentifier);
      }

      window.removeEventListener('scroll', requestPlaygroundControlsStuckUpdate);
      window.removeEventListener('resize', requestPlaygroundControlsStuckUpdate);
    };
  }, [playgroundControlsSentinelElement]);

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

  useEffect(() => {
    if (!activeTab || activeView !== 'graph') {
      return;
    }

    if (activeTab.graphData && activeTab.graphState === 'ready') {
      return;
    }

    if (graphDebounceTimeoutRef.current !== null) {
      window.clearTimeout(graphDebounceTimeoutRef.current);
    }

    graphDebounceTimeoutRef.current = window.setTimeout(() => {
      void loadGraphByTabId(activeTab.id);
    }, 250);

    return () => {
      if (graphDebounceTimeoutRef.current !== null) {
        window.clearTimeout(graphDebounceTimeoutRef.current);
      }
    };
  }, [activeTab?.id, activeTab?.activeView, activeTab?.source, activeTab?.secretsJson]);

  function updateActiveTab(updater: (tab: WorkflowTab) => WorkflowTab) {
    setTabs((currentTabs) => currentTabs.map((tab) => (tab.id === activeTab?.id ? updater(tab) : tab)));
  }

  function updateTab(tabId: string, updater: (tab: WorkflowTab) => WorkflowTab) {
    setTabs((currentTabs) => currentTabs.map((tab) => (tab.id === tabId ? updater(tab) : tab)));
  }

  function setTabView(nextView: PlaygroundView) {
    updateActiveTab((tab) => ({ ...tab, activeView: nextView }));
  }

  function setWorkflowEditorView(nextView: WorkflowEditorView) {
    updateActiveTab((tab) => ({ ...tab, activeEditorView: nextView }));
  }

  function toggleTheme() {
    document.documentElement.classList.add('theme-switching');
    setDarkMode((currentValue) => !currentValue);

    window.requestAnimationFrame(() => {
      window.requestAnimationFrame(() => {
        document.documentElement.classList.remove('theme-switching');
      });
    });
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
      codeFragments: sourceTab.codeFragments.map((fragment) => createWorkflowCodeFragment(fragment.name, fragment.source)),
      cacheKey: uniqueId(),
      runState: 'idle',
      validationState: 'idle',
      message: 'Duplicated workflow.',
      outputJson: '',
      eventLog: [],
      graphState: 'idle',
      graphMessage: 'Open the graph view to generate a visual workflow plan.',
      graphData: null,
      updatedAt: Date.now(),
    };
    tab.activeCodeFragmentId = tab.codeFragments[0]?.id ?? '';
    tab.source = workflowSourceFromCodeFragments(tab.codeFragments, tab.codeFragmentsUseMarkers);

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

  function reorderTab(draggedTabIdentifier: string, targetTabIdentifier: string) {
    if (draggedTabIdentifier === targetTabIdentifier) {
      return;
    }

    setTabs((currentTabs) => {
      const draggedIndex = currentTabs.findIndex((tab) => tab.id === draggedTabIdentifier);
      const targetIndex = currentTabs.findIndex((tab) => tab.id === targetTabIdentifier);

      if (draggedIndex < 0 || targetIndex < 0) {
        return currentTabs;
      }

      const nextTabs = [...currentTabs];
      const [draggedTab] = nextTabs.splice(draggedIndex, 1);

      nextTabs.splice(targetIndex, 0, draggedTab);

      return nextTabs;
    });
  }

  function handleTabDragStart(tabId: string) {
    setDraggedTabId(tabId);
    setDragOverTabId(tabId);
  }

  function handleTabDragOver(tabId: string) {
    if (!draggedTabId || draggedTabId === tabId) {
      return;
    }

    setDragOverTabId(tabId);
  }

  function handleTabDrop(targetTabId: string) {
    if (!draggedTabId) {
      return;
    }

    reorderTab(draggedTabId, targetTabId);
    setDraggedTabId(null);
    setDragOverTabId(null);
  }

  function clearTabDragState() {
    setDraggedTabId(null);
    setDragOverTabId(null);
  }

  function setActiveCodeFragment(fragmentId: string) {
    updateActiveTab((tab) => ({ ...tab, activeCodeFragmentId: fragmentId, activeEditorView: 'code' }));
  }

  function addCodeFragment() {
    const currentTab = requireActiveTab(activeTab);
    const fragmentName = uniqueCodeFragmentName(currentTab.codeFragments, `Fragment ${currentTab.codeFragments.length + 1}`);
    const codeFragment = createWorkflowCodeFragment(fragmentName);
    const nextFragments = [...currentTab.codeFragments, codeFragment];

    updateTab(currentTab.id, (tab) => ({
      ...tab,
      codeFragments: nextFragments,
      activeCodeFragmentId: codeFragment.id,
      activeEditorView: 'code',
      codeFragmentsUseMarkers: true,
      source: workflowSourceFromCodeFragments(nextFragments, true),
      graphState: 'idle',
      graphMessage: 'Graph needs to be regenerated after source changes.',
      graphData: null,
      updatedAt: Date.now(),
    }));
  }

  function updateActiveCodeFragmentSource(source: string) {
    updateActiveTab((tab) => {
      const nextFragments = tab.codeFragments.map((fragment) => (
        fragment.id === tab.activeCodeFragmentId ? { ...fragment, source } : fragment
      ));
      const nextSourceBeforeParsing = workflowSourceFromCodeFragments(nextFragments, tab.codeFragmentsUseMarkers);
      const metadata = parseWorkflowSourceMetadata(nextSourceBeforeParsing);

      if (metadata.source !== nextSourceBeforeParsing) {
        const parsedResult = parseWorkflowSourceFragments(metadata.source, metadata.name ?? tab.name);
        const codeFragments = preserveWorkflowCodeFragmentIdentities(parsedResult.fragments, tab.codeFragments);
        const activeCodeFragmentId = codeFragments.find((fragment) => fragment.id === tab.activeCodeFragmentId)?.id ?? codeFragments[0]?.id ?? tab.activeCodeFragmentId;

        return {
          ...tab,
          name: metadata.name ?? tab.name,
          source: workflowSourceFromCodeFragments(codeFragments, parsedResult.useMarkers),
          codeFragments,
          activeCodeFragmentId,
          codeFragmentsUseMarkers: parsedResult.useMarkers,
          inputJson: metadata.inputJson ?? tab.inputJson,
          secretsJson: metadata.secretsJson ?? tab.secretsJson,
          graphState: 'idle',
          graphMessage: 'Graph needs to be regenerated after source changes.',
          graphData: null,
          updatedAt: Date.now(),
        };
      }

      return {
        ...tab,
        source: workflowSourceWithoutMetadata(nextSourceBeforeParsing),
        codeFragments: nextFragments,
        graphState: 'idle',
        graphMessage: 'Graph needs to be regenerated after source changes.',
        graphData: null,
        updatedAt: Date.now(),
      };
    });
  }

  function closeCodeFragment(fragmentId: string) {
    const currentTab = requireActiveTab(activeTab);

    if (currentTab.codeFragments.length === 1) {
      updateTab(currentTab.id, (tab) => {
        const codeFragment = createWorkflowCodeFragment(tab.name);

        return {
          ...tab,
          source: '',
          codeFragments: [codeFragment],
          activeCodeFragmentId: codeFragment.id,
          activeEditorView: 'code',
          codeFragmentsUseMarkers: false,
          graphState: 'idle',
          graphMessage: 'Graph needs to be regenerated after source changes.',
          graphData: null,
          updatedAt: Date.now(),
        };
      });

      return;
    }

    updateTab(currentTab.id, (tab) => {
      const closedIndex = tab.codeFragments.findIndex((fragment) => fragment.id === fragmentId);
      const nextFragments = tab.codeFragments.filter((fragment) => fragment.id !== fragmentId);
      const nextActiveFragmentId =
        tab.activeCodeFragmentId === fragmentId
          ? nextFragments[Math.max(0, closedIndex - 1)]?.id ?? nextFragments[0]?.id ?? ''
          : tab.activeCodeFragmentId;
      const useMarkers = nextFragments.length > 1;

      return {
        ...tab,
        source: workflowSourceFromCodeFragments(nextFragments, useMarkers),
        codeFragments: nextFragments,
        activeCodeFragmentId: nextActiveFragmentId,
        codeFragmentsUseMarkers: useMarkers,
        graphState: 'idle',
        graphMessage: 'Graph needs to be regenerated after source changes.',
        graphData: null,
        updatedAt: Date.now(),
      };
    });
  }

  function reorderCodeFragment(draggedFragmentIdentifier: string, targetFragmentIdentifier: string) {
    if (draggedFragmentIdentifier === targetFragmentIdentifier) {
      return;
    }

    updateActiveTab((tab) => {
      const draggedIndex = tab.codeFragments.findIndex((fragment) => fragment.id === draggedFragmentIdentifier);
      const targetIndex = tab.codeFragments.findIndex((fragment) => fragment.id === targetFragmentIdentifier);

      if (draggedIndex < 0 || targetIndex < 0) {
        return tab;
      }

      const nextFragments = [...tab.codeFragments];
      const [draggedFragment] = nextFragments.splice(draggedIndex, 1);

      nextFragments.splice(targetIndex, 0, draggedFragment);

      return {
        ...tab,
        codeFragments: nextFragments,
        codeFragmentsUseMarkers: true,
        source: workflowSourceFromCodeFragments(nextFragments, true),
        graphState: 'idle',
        graphMessage: 'Graph needs to be regenerated after source changes.',
        graphData: null,
        updatedAt: Date.now(),
      };
    });
  }

  function handleCodeFragmentDragStart(fragmentId: string) {
    setDraggedCodeFragmentId(fragmentId);
    setDragOverCodeFragmentId(fragmentId);
  }

  function handleCodeFragmentDragOver(fragmentId: string) {
    if (!draggedCodeFragmentId || draggedCodeFragmentId === fragmentId) {
      return;
    }

    setDragOverCodeFragmentId(fragmentId);
  }

  function handleCodeFragmentDrop(targetFragmentId: string) {
    if (!draggedCodeFragmentId) {
      return;
    }

    reorderCodeFragment(draggedCodeFragmentId, targetFragmentId);
    setDraggedCodeFragmentId(null);
    setDragOverCodeFragmentId(null);
  }

  function clearCodeFragmentDragState() {
    setDraggedCodeFragmentId(null);
    setDragOverCodeFragmentId(null);
  }

  function openRenameDialog(tabId: string) {
    const sourceTab = tabs.find((tab) => tab.id === tabId);

    if (!sourceTab) {
      return;
    }

    setRenameDraft(sourceTab.name);
    setRenameDialogTarget({ kind: 'workflow', tabId });
  }

  function openCodeFragmentRenameDialog(tabId: string, fragmentId: string) {
    const sourceTab = tabs.find((tab) => tab.id === tabId);
    const sourceFragment = sourceTab?.codeFragments.find((fragment) => fragment.id === fragmentId);

    if (!sourceTab || !sourceFragment) {
      return;
    }

    setRenameDraft(sourceFragment.name);
    setRenameDialogTarget({ kind: 'codeFragment', tabId, fragmentId });
  }

  function closeRenameDialog() {
    setRenameDialogTarget(null);
    setRenameDraft('');
  }

  function submitRenameDialog() {
    const nextName = renameDraft.trim();

    if (!renameDialogTarget || !nextName) {
      return;
    }

    if (renameDialogTarget.kind === 'workflow') {
      updateTab(renameDialogTarget.tabId, (tab) => ({ ...tab, name: nextName, updatedAt: Date.now() }));
      closeRenameDialog();

      return;
    }

    updateTab(renameDialogTarget.tabId, (tab) => {
      const nextFragments = tab.codeFragments.map((fragment) => (
        fragment.id === renameDialogTarget.fragmentId ? { ...fragment, name: nextName } : fragment
      ));

      return {
        ...tab,
        codeFragments: nextFragments,
        source: workflowSourceFromCodeFragments(nextFragments, tab.codeFragmentsUseMarkers),
        graphState: 'idle',
        graphMessage: 'Graph needs to be regenerated after source changes.',
        graphData: null,
        updatedAt: Date.now(),
      };
    });
    closeRenameDialog();
  }

  function requestBody(currentTab: WorkflowTab, includeInput: boolean) {
    const body: Record<string, unknown> = {
      workflow_source: currentTab.source,
      secrets: parseJsonObject(currentTab.secretsJson, 'secrets'),
    };

    if (includeInput) {
      body.input = parseJsonObject(currentTab.inputJson, 'input');
      body.options = {
        include_events: true,
        max_concurrency: 5,
        use_cache: currentTab.useCache,
        cache_key: currentTab.cacheKey,
      };
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
    const currentCodeFragment = currentTab.codeFragments.find((fragment) => fragment.id === currentTab.activeCodeFragmentId);
    updateActiveTab((tab) => ({ ...tab, message: 'Formatting workflow...' }));

    try {
      const response = await fetch('/format', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ workflow_source: workflowSourceWithoutMetadata(currentTab.source) }),
      });
      const payload = await response.json();

      if (!response.ok) {
        updateTab(currentTab.id, (tab) => ({ ...tab, validationState: 'invalid', message: payload.error ?? 'Unable to format workflow.' }));

        return;
      }

      const parsedResult = parseWorkflowSourceFragments(payload.formatted_workflow_source, currentTab.name);
      const formattedCodeFragments = preserveWorkflowCodeFragmentIdentities(parsedResult.fragments, currentTab.codeFragments);
      const activeCodeFragmentId =
        formattedCodeFragments.find((fragment) => fragment.id === currentTab.activeCodeFragmentId)?.id
        ?? formattedCodeFragments.find((fragment) => fragment.name === currentCodeFragment?.name)?.id
        ?? formattedCodeFragments[0]?.id
        ?? currentTab.activeCodeFragmentId;

      updateTab(currentTab.id, (tab) => ({
        ...tab,
        source: workflowSourceFromCodeFragments(formattedCodeFragments, parsedResult.useMarkers),
        codeFragments: formattedCodeFragments,
        activeCodeFragmentId,
        codeFragmentsUseMarkers: parsedResult.useMarkers,
        validationState: 'valid',
        message: 'Workflow formatted.',
        graphState: 'idle',
        graphMessage: 'Graph needs to be regenerated after formatting.',
        graphData: null,
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
      const response = await fetch('/execute', {
        method: 'POST',
        headers: {
          accept: 'text/event-stream',
          'content-type': 'application/json',
        },
        body: JSON.stringify(requestBody(currentTab, true)),
        signal: nextAbortController.signal,
      });

      if (!response.ok || !response.body) {
        const payload = await response.json().catch(() => ({}));
        throw new Error(payload.error ?? `Request failed with ${response.status}`);
      }

      setCurrentRunIdentifier(response.headers.get(runIdentifierHeader));

      const events = await readWorkflowEventStream(response, currentTab.id, nextAbortController.signal, acceptSseChunk, updateTab);
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
      setCurrentRunIdentifier(null);
    }
  }

  async function stopRun() {
    if (abortController) {
      try {
        if (currentRunIdentifier) {
          updateActiveTab((tab) => ({ ...tab, message: 'Cancelling workflow...' }));
          await cancelWorkflowRun(currentRunIdentifier);

          return;
        }
      } catch (error) {
        updateActiveTab((tab) => ({ ...tab, message: `Run cancelled locally. ${errorMessage(error)}` }));
      }

      abortController.abort();

      return;
    }

    updateActiveTab((tab) => ({ ...tab, runState: 'idle', message: 'Run connection was lost. Start a new run to continue.' }));
  }

  function toggleCache() {
    updateActiveTab((tab) => ({
      ...tab,
      useCache: !tab.useCache,
      message: !tab.useCache ? 'Agent cache enabled.' : 'Agent cache disabled.',
      updatedAt: Date.now(),
    }));
  }

  function purgeCache() {
    updateActiveTab((tab) => ({
      ...tab,
      cacheKey: uniqueId(),
      message: 'Cache key regenerated.',
      updatedAt: Date.now(),
    }));
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

  function formatActiveEditor() {
    if (activeEditorView === 'input') {
      formatRuntimeJson('inputJson');

      return;
    }

    if (activeEditorView === 'secrets') {
      formatRuntimeJson('secretsJson');

      return;
    }

    void formatWorkflow();
  }

  function applyWorkflowTemplate(template: WorkflowTemplate) {
    const parsedResult = parseWorkflowSourceFragments(template.source, template.name);

    updateActiveTab((tab) => ({
      ...tab,
      source: template.source,
      codeFragments: parsedResult.fragments,
      activeCodeFragmentId: parsedResult.fragments[0]?.id ?? tab.activeCodeFragmentId,
      activeEditorView: 'code',
      codeFragmentsUseMarkers: parsedResult.useMarkers,
      inputJson: JSON.stringify(template.input, null, 2),
      secretsJson: JSON.stringify(template.secrets, null, 2),
      message: `Loaded template: ${template.name}.`,
      validationState: 'idle',
      runState: 'idle',
      outputJson: '',
      eventLog: [],
      graphState: 'idle',
      graphMessage: 'Graph needs to be regenerated after loading this template.',
      graphData: null,
      updatedAt: Date.now(),
    }));
  }

  async function exportWorkflowSource() {
    const currentTab = requireActiveTab(activeTab);

    try {
      await navigator.clipboard.writeText(workflowSourceWithMetadata(currentTab.source, currentTab.name, currentTab.inputJson, currentTab.secretsJson));
      updateTab(currentTab.id, (tab) => ({ ...tab, message: 'Workflow source copied to clipboard.' }));
    } catch (error) {
      updateTab(currentTab.id, (tab) => ({ ...tab, validationState: 'invalid', message: errorMessage(error) }));
    }
  }

  function jumpToFullDocumentPosition(position: LspPosition) {
    const currentTab = requireActiveTab(activeTab);
    const fullOffset = lspPositionToOffset(currentTab.source, position);
    const selectedSourceMap = sourceMapForFullOffset(currentTab.codeFragments, currentTab.codeFragmentsUseMarkers, fullOffset);

    if (!selectedSourceMap) {
      return;
    }

    setTabView('workflow');
    setWorkflowEditorView('code');
    setEditorJumpTarget({
      tabId: currentTab.id,
      fragmentId: selectedSourceMap.fragment.id,
      offset: Math.max(0, Math.min(fullOffset - selectedSourceMap.sourceStartOffset, selectedSourceMap.fragment.source.length)),
      sequence: Date.now(),
    });
    updateTab(currentTab.id, (tab) => ({ ...tab, activeCodeFragmentId: selectedSourceMap.fragment.id, activeEditorView: 'code' }));
  }

  async function loadGraph() {
    const currentTab = requireActiveTab(activeTab);
    await loadGraphByTabId(currentTab.id);
  }

  async function loadGraphByTabId(tabId: string) {
    const currentTab = tabs.find((tab) => tab.id === tabId);

    if (!currentTab) {
      return;
    }

    updateTab(currentTab.id, (tab) => ({ ...tab, graphState: 'loading', graphMessage: 'Building workflow graph...' }));

    try {
      const response = await fetch('/graph', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(requestBody(currentTab, false)),
      });
      const payload = await responsePayload(response);

      if (!response.ok || !payload.valid) {
        updateTab(currentTab.id, (tab) => ({
          ...tab,
          graphState: 'failed',
          graphMessage: stringPayloadValue(payload.details) ?? stringPayloadValue(payload.error) ?? 'Unable to build workflow graph.',
          graphData: null,
        }));

        return;
      }

      updateTab(currentTab.id, (tab) => ({
        ...tab,
        graphState: 'ready',
        graphMessage: 'Workflow graph is up to date.',
        graphData: payload.graph as WorkflowExecutionGraph,
        validationState: 'valid',
      }));
    } catch (error) {
      updateTab(currentTab.id, (tab) => ({
        ...tab,
        graphState: 'failed',
        graphMessage: errorMessage(error),
        graphData: null,
      }));
    }
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

  const playgroundControlsSentinel = <div ref={setPlaygroundControlsSentinelElement} className="playground__controls-sentinel" aria-hidden="true" />;

  const playgroundControls = activeTab ? (
    <div className="playground__controls" data-stuck={playgroundControlsStuck ? 'true' : 'false'}>
      <nav className="playground-mode-switch" aria-label="Playground mode">
        <Button variant={activeView === 'workflow' ? 'secondary' : 'ghost'} size="lg" className="playground-mode-switch__button" onClick={() => setTabView('workflow')}><Workflow /> Workflow</Button>
        <Button variant={activeView === 'graph' ? 'secondary' : 'ghost'} size="lg" className="playground-mode-switch__button" onClick={() => setTabView('graph')}><GitBranch /> Graph</Button>
      </nav>

      <div className="playground-actions">
        <div className="playground-cache-controls" aria-label="Cache settings">
          <ActionTooltip label={activeTab.useCache ? 'Disable agent cache for this workflow tab' : 'Enable agent cache for this workflow tab'}>
            <Button
              variant={activeTab.useCache ? 'secondary' : 'ghost'}
              size="icon-lg"
              aria-label={activeTab.useCache ? 'Disable cache' : 'Enable cache'}
              aria-pressed={activeTab.useCache}
              onClick={toggleCache}
            >
              <Database />
            </Button>
          </ActionTooltip>
          <ActionTooltip label="Regenerate cache key for this workflow tab">
            <Button variant="ghost" size="icon-lg" aria-label="Purge cache" onClick={purgeCache}>
              <DatabaseZap />
            </Button>
          </ActionTooltip>
        </div>
        {activeTab.runState === 'running' ? (
          <ActionTooltip label="Stop the current workflow run">
            <Button variant="destructive" size="lg" onClick={stopRun}><Square /> Stop</Button>
          </ActionTooltip>
        ) : (
          <ActionTooltip label="Run the workflow with the current input and secrets">
            <Button className="playground-actions__run" disabled={!canRun} size="lg" onClick={runWorkflow}><Play /> Run workflow</Button>
          </ActionTooltip>
        )}
      </div>
    </div>
  ) : null;

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
                  <ActionTooltip label="Toggle color theme">
                    <Button className="playground__theme-toggle" variant="ghost" size="icon-lg" aria-label="Toggle theme" onClick={toggleTheme}>
                      {darkMode ? <Sun /> : <Moon />}
                    </Button>
                  </ActionTooltip>
                </div>
              </header>

              <Tabs value={activeTab?.id ?? ''} onValueChange={setActiveTabId} className="playground__tabs">
                <TabsList variant="line" className="h-auto flex-wrap justify-start gap-3 bg-transparent p-0">
                  {tabs.map((tab) => (
                    <PlaygroundTabChip
                      key={tab.id}
                      size="large"
                      active={tab.id === activeTab?.id}
                      tone={workflowTabTone(tab)}
                      activeGlow
                      dragging={draggedTabId === tab.id}
                      dragOver={dragOverTabId === tab.id}
                      onDragStart={() => handleTabDragStart(tab.id)}
                      onDragOver={() => handleTabDragOver(tab.id)}
                      onDrop={() => handleTabDrop(tab.id)}
                      onDragEnd={clearTabDragState}
                      trigger={(
                        <TabsTrigger value={tab.id} className="playground-tab-chip__trigger">
                          <span className="playground-tab-chip__dot" />
                          <span className="playground-tab-chip__title">{tab.name}</span>
                          <RunStateBadge state={tab.runState} />
                        </TabsTrigger>
                      )}
                      actions={[
                        { label: `Rename ${tab.name}`, icon: <Pencil />, onClick: () => openRenameDialog(tab.id) },
                        { label: `Duplicate ${tab.name}`, icon: <Copy />, onClick: () => duplicateTabById(tab.id) },
                        { label: `Close ${tab.name}`, icon: <Trash2 />, onClick: () => closeTab(tab.id) },
                      ]}
                    />
                  ))}
                  <Button variant="outline" size="lg" className="playground-tabs__new" onClick={addTab}><Plus /> Workflow</Button>
                </TabsList>
              </Tabs>

              <div className="playground__canvas">
                {activeTab ? (
                  <section className="playground__content">
                    {activeView === 'workflow' ? (
                      <section className="workflow-layout">
                        <div className="workflow-layout__sticky-scope">
                          {playgroundControlsSentinel}
                          {playgroundControls}

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
                            <Card className="workflow-editor" data-tone={editorStateTone}>
                              <div className="workflow-editor__header panel-card__header">
                                <div className="panel-card__title-block">
                                  <strong>{activeTab.name}</strong>
                                </div>
                                <div className="workflow-fragment-actions">
                                  <ActionTooltip label="Copy this workflow, input, and secrets as portable source">
                                    <Button variant="ghost" size="sm" onClick={exportWorkflowSource}><Download /> Export</Button>
                                  </ActionTooltip>
                                  <ActionTooltip label="Format the active workflow, input, or secrets editor">
                                    <Button variant="ghost" size="sm" onClick={formatActiveEditor}><RefreshCcw /> Format</Button>
                                  </ActionTooltip>
                                  <ActionTooltip label="Validate the workflow without running agents">
                                    <Button variant="ghost" size="sm" onClick={validateWorkflow}><CheckCircle2 /> Validate</Button>
                                  </ActionTooltip>
                                  <ActionTooltip label="Add a new workflow code fragment">
                                    <Button variant="outline" size="sm" onClick={addCodeFragment}><Plus /> Fragment</Button>
                                  </ActionTooltip>
                                </div>
                              </div>
                              <div className="workflow-editor-tabs" aria-label="Workflow editor tabs">
                                <div className="workflow-editor-tabs__fragments" aria-label="Workflow code fragments">
                                  {activeTab.codeFragments.map((fragment) => (
                                    <PlaygroundTabChip
                                      key={fragment.id}
                                      size="small"
                                      active={activeEditorView === 'code' && fragment.id === activeTab.activeCodeFragmentId}
                                      dragging={draggedCodeFragmentId === fragment.id}
                                      dragOver={dragOverCodeFragmentId === fragment.id}
                                      onDragStart={() => handleCodeFragmentDragStart(fragment.id)}
                                      onDragOver={() => handleCodeFragmentDragOver(fragment.id)}
                                      onDrop={() => handleCodeFragmentDrop(fragment.id)}
                                      onDragEnd={clearCodeFragmentDragState}
                                      trigger={(
                                        <button type="button" className="playground-tab-chip__trigger" onClick={() => setActiveCodeFragment(fragment.id)}>
                                          <span className="playground-tab-chip__title">{fragment.name}</span>
                                        </button>
                                      )}
                                      actions={[
                                        { label: `Rename ${fragment.name}`, icon: <Pencil />, onClick: () => openCodeFragmentRenameDialog(activeTab.id, fragment.id) },
                                        { label: `Close ${fragment.name}`, icon: <Trash2 />, onClick: () => closeCodeFragment(fragment.id) },
                                      ]}
                                    />
                                  ))}
                                </div>

                                <div className="workflow-editor-tabs__variables" aria-label="Workflow variables">
                                  <PlaygroundTabChip
                                    size="small"
                                    active={activeEditorView === 'input'}
                                    draggable={false}
                                    dragging={false}
                                    dragOver={false}
                                    trigger={(
                                      <button type="button" className="playground-tab-chip__trigger" onClick={() => setWorkflowEditorView('input')}>
                                        <span className="playground-tab-chip__title">Input</span>
                                      </button>
                                    )}
                                    actions={[]}
                                  />
                                  <PlaygroundTabChip
                                    size="small"
                                    active={activeEditorView === 'secrets'}
                                    draggable={false}
                                    dragging={false}
                                    dragOver={false}
                                    trigger={(
                                      <button type="button" className="playground-tab-chip__trigger" onClick={() => setWorkflowEditorView('secrets')}>
                                        <span className="playground-tab-chip__title">Secrets</span>
                                      </button>
                                    )}
                                    actions={[]}
                                  />
                                </div>
                              </div>
                              {activeEditorView === 'code' && activeCodeFragment && activeCodeFragmentSourceMap ? (
                                <WireEditor
                                  key={`${activeTab.id}-${activeCodeFragment.id}`}
                                  value={activeCodeFragment.source}
                                  fullValue={activeTab.source}
                                  documentId={activeTab.id}
                                  documentOffset={activeCodeFragmentSourceMap.sourceStartOffset}
                                  darkMode={darkMode}
                                  inputJson={activeTab.inputJson}
                                  secretsJson={activeTab.secretsJson}
                                  jumpTarget={activeEditorJumpTarget}
                                  onChange={updateActiveCodeFragmentSource}
                                  onDefinitionJump={jumpToFullDocumentPosition}
                                />
                              ) : null}
                              {activeEditorView === 'input' ? (
                                <JsonCodeEditor
                                  key={`${activeTab.id}-input`}
                                  value={activeTab.inputJson}
                                  fullEditor
                                  className="workflow-editor__json"
                                  onChange={(inputJson) => updateActiveTab((tab) => ({ ...tab, inputJson, updatedAt: Date.now() }))}
                                />
                              ) : null}
                              {activeEditorView === 'secrets' ? (
                                <JsonCodeEditor
                                  key={`${activeTab.id}-secrets`}
                                  value={activeTab.secretsJson}
                                  fullEditor
                                  className="workflow-editor__json"
                                  onChange={(secretsJson) => updateActiveTab((tab) => ({
                                    ...tab,
                                    secretsJson,
                                    graphState: 'idle',
                                    graphMessage: 'Graph needs to be regenerated after secrets changes.',
                                    graphData: null,
                                    updatedAt: Date.now(),
                                  }))}
                                />
                              ) : null}
                              <div className={`workflow-editor__message workflow-editor__message--${editorMessageTone}`}>
                                <span className="workflow-editor__message-line workflow-editor__message-line--full">{editorMessage}</span>
                              </div>
                            </Card>
                          </div>
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

                    {activeView === 'graph' ? (
                      <section className="graph-layout">
                        {playgroundControlsSentinel}
                        {playgroundControls}

                        <WorkflowGraphView graph={activeTab.graphData} source={activeTab.source} graphState={activeTab.graphState} runState={activeTab.runState} events={activeTab.eventLog} outputJson={activeTab.outputJson} message={activeTab.graphMessage} onRefresh={loadGraph} />
                      </section>
                    ) : null}
                  </section>
                ) : null}
              </div>
            </div>
        </section>
      </div>
    </main>

      <Dialog open={renameDialogTarget !== null} onOpenChange={(open) => {
        if (!open) {
          closeRenameDialog();
        }
      }}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{renameDialogTarget?.kind === 'codeFragment' ? 'Rename code fragment' : 'Rename workflow tab'}</DialogTitle>
            <DialogDescription>
              {renameDialogTarget?.kind === 'codeFragment'
                ? 'Use a short, clear name for this fragment tab.'
                : 'Use a short, clear name for this workflow tab.'}
            </DialogDescription>
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

  const savedTabs = localStorage.getItem(tabsStorageKey);
  const restoredTabs = savedTabs ? (JSON.parse(savedTabs) as unknown[]).map(recoverWorkflowTabAfterReload) : [createWorkflowTab('Launch brief')];
  const tabs = restoredTabs.length > 0 ? restoredTabs : [createWorkflowTab('Launch brief')];
  const savedActiveTabId = localStorage.getItem(activeTabStorageKey);
  const activeTabId = tabs.some((tab) => tab.id === savedActiveTabId) ? savedActiveTabId! : tabs[0]?.id ?? '';

  setTabs(tabs);
  setActiveTabId(activeTabId);
}

type UpdateTab = (tabId: string, updater: (tab: WorkflowTab) => WorkflowTab) => void;

interface SseReadResult {
  events: ExecutorEvent[];
  lastEventIdentifier: string | null;
  terminalEvent: ExecutorEvent | null;
}

async function readWorkflowEventStream(
  initialResponse: Response,
  tabId: string,
  abortSignal: AbortSignal,
  acceptChunk: (chunk: string, tabId: string) => ExecutorEvent | null,
  updateTab: UpdateTab,
) {
  const runIdentifier = initialResponse.headers.get(runIdentifierHeader);
  let response = initialResponse;
  let lastEventIdentifier: string | null = null;
  const events: ExecutorEvent[] = [];

  while (true) {
    try {
      const readResult = await readSseStream(response.body, tabId, acceptChunk, lastEventIdentifier);
      events.push(...readResult.events);
      lastEventIdentifier = readResult.lastEventIdentifier;

      if (readResult.terminalEvent) {
        return events;
      }
    } catch (error) {
      if (abortSignal.aborted) {
        throw error;
      }
    }

    if (!runIdentifier) {
      throw new Error('Run connection was lost and the server did not provide a reconnect identifier.');
    }

    let reconnected = false;

    while (!reconnected) {
      updateTab(tabId, (tab) => ({ ...tab, message: 'Run connection was lost. Reconnecting...' }));
      await waitForReconnectDelay(abortSignal);

      try {
        response = await reconnectWorkflowEventStream(runIdentifier, lastEventIdentifier, abortSignal);
        reconnected = true;
      } catch (error) {
        if (abortSignal.aborted || error instanceof WorkflowStreamUnavailableError) {
          throw error;
        }
      }
    }
  }
}

class WorkflowStreamUnavailableError extends Error {}

async function cancelWorkflowRun(runIdentifier: string) {
  const response = await fetch(`/execute/${encodeURIComponent(runIdentifier)}/cancel`, { method: 'POST' });

  if (!response.ok && response.status !== 404) {
    const payload: Record<string, unknown> = await responsePayload(response).catch(() => ({}));
    throw new Error(typeof payload.error === 'string' ? payload.error : `Unable to cancel workflow run (${response.status}).`);
  }
}

async function reconnectWorkflowEventStream(runIdentifier: string, lastEventIdentifier: string | null, abortSignal: AbortSignal) {
  const headers: Record<string, string> = {
    accept: 'text/event-stream',
  };

  if (lastEventIdentifier) {
    headers['last-event-id'] = lastEventIdentifier;
  }

  const response = await fetch(`/execute/${encodeURIComponent(runIdentifier)}/events`, {
    headers,
    signal: abortSignal,
  });

  if (response.status === 404) {
    throw new WorkflowStreamUnavailableError('Workflow stream is no longer available on the server.');
  }

  if (!response.ok || !response.body) {
    const payload: Record<string, unknown> = await responsePayload(response).catch(() => ({}));
    throw new Error(typeof payload.error === 'string' ? payload.error : `Unable to reconnect workflow stream (${response.status}).`);
  }

  return response;
}

async function waitForReconnectDelay(abortSignal: AbortSignal) {
  await new Promise<void>((resolve, reject) => {
    if (abortSignal.aborted) {
      reject(new DOMException('Run cancelled.', 'AbortError'));

      return;
    }

    let timeoutHandle: number | null = null;
    const abortListener = () => {
      if (timeoutHandle !== null) {
        window.clearTimeout(timeoutHandle);
      }

      reject(new DOMException('Run cancelled.', 'AbortError'));
    };

    timeoutHandle = window.setTimeout(() => {
      abortSignal.removeEventListener('abort', abortListener);
      resolve();
    }, streamReconnectDelayMilliseconds);

    abortSignal.addEventListener('abort', abortListener, { once: true });
  });
}

async function readSseStream(
  stream: ReadableStream<Uint8Array> | null,
  tabId: string,
  acceptChunk: (chunk: string, tabId: string) => ExecutorEvent | null,
  initialLastEventIdentifier: string | null,
): Promise<SseReadResult> {
  if (!stream) {
    throw new Error('Workflow stream response did not include a body.');
  }

  const reader = stream.getReader();
  const decoder = new TextDecoder();
  const events: ExecutorEvent[] = [];
  let lastEventIdentifier = initialLastEventIdentifier;
  let terminalEvent: ExecutorEvent | null = null;
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
      const sseMessage = parseSseMessage(chunk);
      const event = sseMessage.data ? acceptChunk(chunk, tabId) : null;

      lastEventIdentifier = sseMessage.eventIdentifier ?? lastEventIdentifier;

      if (event) {
        events.push(event);

        if (isTerminalWorkflowEvent(event)) {
          terminalEvent = event;
        }
      }
    }
  }

  if (buffer.trim()) {
    const sseMessage = parseSseMessage(buffer);
    const event = sseMessage.data ? acceptChunk(buffer, tabId) : null;

    lastEventIdentifier = sseMessage.eventIdentifier ?? lastEventIdentifier;

    if (event) {
      events.push(event);

      if (isTerminalWorkflowEvent(event)) {
        terminalEvent = event;
      }
    }
  }

  return {
    events,
    lastEventIdentifier,
    terminalEvent,
  };
}

function parseSseChunk(chunk: string): ExecutorEvent | null {
  const sseMessage = parseSseMessage(chunk);

  if (!sseMessage.data) {
    return null;
  }

  return JSON.parse(sseMessage.data) as ExecutorEvent;
}

function parseSseMessage(chunk: string) {
  const eventIdentifierLines: string[] = [];
  const dataLines: string[] = [];

  for (const line of chunk.split('\n')) {
    if (line.startsWith('id:')) {
      eventIdentifierLines.push(line.slice('id:'.length).trimStart());
    }

    if (line.startsWith('data:')) {
      dataLines.push(line.slice('data:'.length).trimStart());
    }
  }

  return {
    eventIdentifier: eventIdentifierLines.at(-1) ?? null,
    data: dataLines.length > 0 ? dataLines.join('\n') : null,
  };
}

function isTerminalWorkflowEvent(event: ExecutorEvent) {
  return event.kind === 'workflow_completed' || event.kind === 'workflow_failed';
}

function jsonObjectValidationError(source: string) {
  try {
    parseJsonObject(source, 'value');

    return null;
  } catch (error) {
    return errorMessage(error);
  }
}

function editorJsonValidationError(activeTab: WorkflowTab, activeEditorView: WorkflowEditorView) {
  if (activeEditorView === 'input') {
    return jsonObjectValidationError(activeTab.inputJson);
  }

  if (activeEditorView === 'secrets') {
    return jsonObjectValidationError(activeTab.secretsJson);
  }

  return null;
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

async function responsePayload(response: Response): Promise<Record<string, unknown>> {
  const responseText = await response.text();

  if (!responseText.trim()) {
    return {};
  }

  try {
    const payload = JSON.parse(responseText) as unknown;

    if (isRecord(payload)) {
      return payload;
    }

    return { error: responseText };
  } catch {
    return { error: responseText };
  }
}

function stringPayloadValue(value: unknown) {
  return typeof value === 'string' ? value : null;
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

function resolveEditorMessageTone(activeTab: WorkflowTab | undefined): 'neutral' | 'success' | 'error' {
  if (!activeTab) {
    return 'neutral';
  }

  if (activeTab.validationState === 'valid') {
    return 'success';
  }

  if (activeTab.validationState === 'invalid') {
    return 'error';
  }

  if (activeTab.runState === 'failed') {
    return 'error';
  }

  if (activeTab.runState === 'completed') {
    return 'success';
  }

  return 'neutral';
}

function workflowEditorTone(tab: WorkflowTab): 'neutral' | 'success' | 'error' | 'running' {
  if (tab.validationState === 'invalid' || tab.runState === 'failed') {
    return 'error';
  }

  if (tab.runState === 'running') {
    return 'running';
  }

  if (tab.validationState === 'valid' || tab.runState === 'completed') {
    return 'success';
  }

  return 'neutral';
}

function workflowTabTone(tab: WorkflowTab): 'default' | 'error' {
  return tab.validationState === 'invalid' || tab.runState === 'failed' ? 'error' : 'default';
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function ActionTooltip({ children, label }: { children: ReactElement; label: string }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>{children}</TooltipTrigger>
      <TooltipContent side="bottom" sideOffset={8}>{label}</TooltipContent>
    </Tooltip>
  );
}
