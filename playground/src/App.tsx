import { ArrowLeft, ArrowRight, CheckCircle2, Copy, Database, DatabaseZap, GitBranch, KeyRound, Maximize2, Menu, Minimize2, Moon, Pencil, Play, Plus, RefreshCcw, Square, Sun, Trash2, Workflow } from 'lucide-react';
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
import {
  CancellationTransition,
  ExecutorCacheOperation,
  ExecutorDiagnosticCode,
  ExecutorDiagnosticRetryability,
  ExecutorDiagnosticSeverity,
  ExecutorDiagnosticSubjectType,
  ExecutorEventKind,
  ExecutorStage,
  type CancellationResponse,
  type ExecutionDiagnostic,
  type ExecutionDiagnosticSubject,
  type ExecutorEvent,
  type PlaygroundView,
  type WorkflowEditorView,
  type WorkflowExecutionGraph,
  type WorkflowTab,
} from './types';
import { formatExecutionDiagnosticData } from './eventFormatting';
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
const defaultGraphMessage = 'Open the graph view to generate a visual workflow plan.';
const executorEventKinds = new Set<string>(Object.values(ExecutorEventKind));
const executorCacheOperations = new Set<string>(Object.values(ExecutorCacheOperation));
const executorDiagnosticCodes = new Set<string>(Object.values(ExecutorDiagnosticCode));
const executorStages = new Set<string>(Object.values(ExecutorStage));
const executorDiagnosticSeverities = new Set<string>(Object.values(ExecutorDiagnosticSeverity));
const executorDiagnosticRetryabilities = new Set<string>(Object.values(ExecutorDiagnosticRetryability));
const executorDiagnosticSubjectTypes = new Set<string>(Object.values(ExecutorDiagnosticSubjectType));
const cancellationTransitions = new Set<string>(Object.values(CancellationTransition));
const maxSseLineBytes = 256 * 1024;
const maxSseDataBytes = 768 * 1024;
const maxSseFrameBytes = 1024 * 1024;
const maxSseBufferedTextBytes = 2 * 1024 * 1024;
const maxSseTotalBytes = 16 * 1024 * 1024;
const maxAcceptedEventIdentifiers = 2048;
const maxRetainedUiEvents = 500;
const maxRetainedUiEventBytes = 4 * 1024 * 1024;
const eventHistoryTruncationMessage = 'Older browser event history was truncated to stay within local safety limits.';
const utf8Encoder = new TextEncoder();
const serializedExecutorEventByteLengths = new WeakMap<ExecutorEvent, number>();

type RenameDialogTarget =
  | { kind: 'workflow'; tabId: string }
  | { kind: 'codeFragment'; tabId: string; fragmentId: string };

type MaximizedWorkflowPanel = 'output' | 'events' | null;

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

enum WorkflowOperationKind {
  Validate = 'validate',
  Format = 'format',
  Graph = 'graph',
  Export = 'export',
}

interface WorkflowContentSnapshot {
  source: string;
  inputJson: string;
  secretsJson: string;
}

interface WorkflowOperationToken {
  tabId: string;
  kind: WorkflowOperationKind;
  revision: number;
  snapshot: WorkflowContentSnapshot;
}

interface WorkflowRunOwnership {
  sequence: number;
  abortController: AbortController;
  runIdentifier: string | null;
  cancellationRequested: boolean;
  snapshot: WorkflowContentSnapshot;
}

interface StreamGapConfirmationRequest {
  requestIdentifier: number;
  tabId: string;
  runSequence: number;
  diagnostic: ExecutionDiagnostic;
  resumeAfter: string | null;
  historyLoss: boolean;
  abortSignal: AbortSignal;
  abortListener: () => void;
  resolve: (accepted: boolean) => void;
}

interface WorkflowProblem {
  key: string;
  message: string;
  diagnostic: ExecutionDiagnostic | null;
  tone: 'error' | 'warning' | 'cancelled' | 'gap';
}

export default function App() {
  const [tabs, setTabs] = useState<WorkflowTab[]>(() => [createWorkflowTab('Launch brief')]);
  const [activeTabId, setActiveTabId] = useState('');
  const [darkMode, setDarkMode] = useState(true);
  const [problemsOpen, setProblemsOpen] = useState(true);
  const [outputOpen, setOutputOpen] = useState(true);
  const [eventsOpen, setEventsOpen] = useState(true);
  const [maximizedWorkflowPanel, setMaximizedWorkflowPanel] = useState<MaximizedWorkflowPanel>(null);
  const [eventGroupingMode, setEventGroupingMode] = useState<EventGroupingMode>(EventGroupingMode.Chronological);
  const [includeSecretsConfirmationOpen, setIncludeSecretsConfirmationOpen] = useState(false);
  const [streamGapConfirmationQueue, setStreamGapConfirmationQueue] = useState<StreamGapConfirmationRequest[]>([]);
  const [renameDialogTarget, setRenameDialogTarget] = useState<RenameDialogTarget | null>(null);
  const [renameDraft, setRenameDraft] = useState('');
  const [draggedTabId, setDraggedTabId] = useState<string | null>(null);
  const [dragOverTabId, setDragOverTabId] = useState<string | null>(null);
  const [draggedCodeFragmentId, setDraggedCodeFragmentId] = useState<string | null>(null);
  const [dragOverCodeFragmentId, setDragOverCodeFragmentId] = useState<string | null>(null);
  const [editorJumpTarget, setEditorJumpTarget] = useState<EditorJumpTarget | null>(null);
  const [playgroundControlsSentinelElement, setPlaygroundControlsSentinelElement] = useState<HTMLDivElement | null>(null);
  const [playgroundControlsStuck, setPlaygroundControlsStuck] = useState(false);
  const [toastMessage, setToastMessage] = useState('');
  const graphDebounceTimeoutRef = useRef<number | null>(null);
  const pendingEventBatchesRef = useRef(new Map<string, ExecutorEvent[]>());
  const eventBatchFlushFrameRef = useRef<number | null>(null);
  const tabsRef = useRef(tabs);
  const runSequenceRef = useRef(0);
  const runOwnershipByTabRef = useRef(new Map<string, WorkflowRunOwnership>());
  const streamGapConfirmationQueueRef = useRef<StreamGapConfirmationRequest[]>([]);
  const streamGapConfirmationSequenceRef = useRef(0);
  const operationRevisionByKeyRef = useRef(new Map<string, number>());
  const scheduledValidationByTabRef = useRef(new Map<string, number>());
  const visibleStreamGapConfirmationRequest = streamGapConfirmationQueue[0] ?? null;
  const activeTab = tabs.find((tab) => tab.id === activeTabId) ?? tabs[0];
  const canRun = activeTab?.runState !== 'running';
  const activeView: PlaygroundView = activeTab?.activeView ?? 'workflow';
  const activeEditorView: WorkflowEditorView = activeTab?.activeEditorView ?? 'code';
  const shouldShowTemplatePicker = activeView === 'workflow' && activeEditorView === 'code' && (activeTab?.source.trim() ?? '') === '';
  const activeJsonValidationError = activeTab ? editorJsonValidationError(activeTab, activeEditorView) : null;
  const editorMessageTone = activeJsonValidationError ? 'error' : resolveEditorMessageTone(activeTab);
  const editorStateTone = activeTab ? workflowEditorTone(activeTab) : 'neutral';
  const editorMessage = activeJsonValidationError ?? activeTab?.message ?? 'Ready.';
  const activeProblems = activeTab ? workflowProblems(activeTab, activeJsonValidationError) : [];
  const activeCodeFragment = activeTab?.codeFragments.find((fragment) => fragment.id === activeTab.activeCodeFragmentId) ?? activeTab?.codeFragments[0];
  const activeCodeFragmentSourceMap = activeTab && activeCodeFragment
    ? sourceMapForFragment(activeTab.codeFragments, activeTab.codeFragmentsUseMarkers, activeCodeFragment.id)
    : null;
  const activeEditorJumpTarget =
    activeTab && activeCodeFragment && editorJumpTarget?.tabId === activeTab.id && editorJumpTarget.fragmentId === activeCodeFragment.id
      ? editorJumpTarget
      : null;

  tabsRef.current = tabs;

  useEffect(() => {
    restoreFromStorage(setTabs, setActiveTabId, setDarkMode);
  }, []);

  useEffect(() => {
    if (tabs.length === 0 || !activeTabId) {
      return;
    }

    persistPlaygroundState(tabs, activeTabId, darkMode);
  }, [tabs, activeTabId, darkMode]);

  useEffect(() => {
    document.documentElement.classList.toggle('dark', darkMode);
  }, [darkMode]);

  useEffect(() => {
    if (!toastMessage) {
      return;
    }

    const timeoutIdentifier = window.setTimeout(() => setToastMessage(''), 2600);

    return () => window.clearTimeout(timeoutIdentifier);
  }, [toastMessage]);

  useEffect(() => () => {
    if (eventBatchFlushFrameRef.current !== null) {
      window.cancelAnimationFrame(eventBatchFlushFrameRef.current);
    }

    for (const timeoutIdentifier of scheduledValidationByTabRef.current.values()) {
      window.clearTimeout(timeoutIdentifier);
    }

    for (const ownership of runOwnershipByTabRef.current.values()) {
      ownership.abortController.abort();
    }
  }, []);

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

  function snapshotForTab(tab: WorkflowTab): WorkflowContentSnapshot {
    return {
      source: tab.source,
      inputJson: tab.inputJson,
      secretsJson: tab.secretsJson,
    };
  }

  function operationKey(tabId: string, kind: WorkflowOperationKind) {
    return `${tabId}:${kind}`;
  }

  function beginWorkflowOperation(tab: WorkflowTab, kind: WorkflowOperationKind): WorkflowOperationToken {
    const key = operationKey(tab.id, kind);
    const revision = (operationRevisionByKeyRef.current.get(key) ?? 0) + 1;
    operationRevisionByKeyRef.current.set(key, revision);

    return {
      tabId: tab.id,
      kind,
      revision,
      snapshot: snapshotForTab(tab),
    };
  }

  function invalidateWorkflowOperation(tabId: string, kind: WorkflowOperationKind) {
    const key = operationKey(tabId, kind);
    operationRevisionByKeyRef.current.set(key, (operationRevisionByKeyRef.current.get(key) ?? 0) + 1);
  }

  function invalidateAllWorkflowOperations(tabId: string) {
    for (const kind of Object.values(WorkflowOperationKind)) {
      invalidateWorkflowOperation(tabId, kind);
    }
  }

  function workflowOperationIsCurrent(token: WorkflowOperationToken) {
    const currentRevision = operationRevisionByKeyRef.current.get(operationKey(token.tabId, token.kind));
    const currentTab = tabsRef.current.find((tab) => tab.id === token.tabId);

    return currentRevision === token.revision && currentTab !== undefined && workflowSnapshotsMatch(snapshotForTab(currentTab), token.snapshot);
  }

  function cancelScheduledValidation(tabId: string) {
    const timeoutIdentifier = scheduledValidationByTabRef.current.get(tabId);

    if (timeoutIdentifier !== undefined) {
      window.clearTimeout(timeoutIdentifier);
      scheduledValidationByTabRef.current.delete(tabId);
    }
  }

  function scheduleActiveWorkflowValidation() {
    if (!activeTab || activeTab.runState === 'running') {
      return;
    }

    cancelScheduledValidation(activeTab.id);

    const tabId = activeTab.id;
    const timeoutIdentifier = window.setTimeout(() => {
      scheduledValidationByTabRef.current.delete(tabId);
      void validateWorkflowByTabId(tabId);
    }, 180);
    scheduledValidationByTabRef.current.set(tabId, timeoutIdentifier);
  }

  function runOwnershipIsCurrent(tabId: string, sequence: number) {
    return runOwnershipByTabRef.current.get(tabId)?.sequence === sequence;
  }

  function setTabView(nextView: PlaygroundView) {
    if (activeTab) {
      cancelScheduledValidation(activeTab.id);
    }

    updateActiveTab((tab) => ({ ...tab, activeView: nextView }));
  }

  function setWorkflowEditorView(nextView: WorkflowEditorView) {
    if (activeTab) {
      cancelScheduledValidation(activeTab.id);
    }

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
      runtimeDiagnostic: null,
      graphState: 'idle',
      graphMessage: defaultGraphMessage,
      graphData: null,
      updatedAt: Date.now(),
    };
    tab.activeCodeFragmentId = tab.codeFragments[0]?.id ?? '';
    tab.source = workflowSourceFromCodeFragments(tab.codeFragments, tab.codeFragmentsUseMarkers);

    setTabs((currentTabs) => [...currentTabs, tab]);
    setActiveTabId(tab.id);
  }

  function closeTab(tabId: string) {
    const tabToClose = tabs.find((tab) => tab.id === tabId);
    const runOwnership = runOwnershipByTabRef.current.get(tabId);

    if (tabToClose?.runState === 'running' && !window.confirm(`Stop and close ${tabToClose.name}?`)) {
      return;
    }

    cancelScheduledValidation(tabId);
    invalidateAllWorkflowOperations(tabId);

    if (runOwnership) {
      if (runOwnership.runIdentifier) {
        void cancelWorkflowRun(runOwnership.runIdentifier).catch(() => undefined);
      }

      runOwnership.abortController.abort();
      runOwnershipByTabRef.current.delete(tabId);
    }

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

  function moveWorkflowTab(tabId: string, offset: -1 | 1) {
    const tabIndex = tabs.findIndex((tab) => tab.id === tabId);
    const targetTab = tabs[tabIndex + offset];

    if (targetTab) {
      reorderTab(tabId, targetTab.id);
      setToastMessage(`Moved workflow ${offset < 0 ? 'left' : 'right'}.`);
    }
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
    if (activeTab) {
      cancelScheduledValidation(activeTab.id);
    }

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
      validationState: 'idle',
      message: 'Workflow changed. Validate or run to refresh status.',
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
          validationState: 'idle',
          message: 'Workflow changed. Validate or run to refresh status.',
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
        validationState: 'idle',
        message: 'Workflow changed. Validate or run to refresh status.',
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
          validationState: 'idle',
          message: 'Workflow changed. Validate or run to refresh status.',
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
        validationState: 'idle',
        message: 'Workflow changed. Validate or run to refresh status.',
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
        validationState: 'idle',
        message: 'Workflow changed. Validate or run to refresh status.',
        graphState: 'idle',
        graphMessage: 'Graph needs to be regenerated after source changes.',
        graphData: null,
        updatedAt: Date.now(),
      };
    });
  }

  function moveCodeFragment(fragmentId: string, offset: -1 | 1) {
    const currentTab = requireActiveTab(activeTab);
    const fragmentIndex = currentTab.codeFragments.findIndex((fragment) => fragment.id === fragmentId);
    const targetFragment = currentTab.codeFragments[fragmentIndex + offset];

    if (targetFragment) {
      reorderCodeFragment(fragmentId, targetFragment.id);
      setToastMessage(`Moved fragment ${offset < 0 ? 'left' : 'right'}.`);
    }
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
        validationState: 'idle',
        message: 'Workflow changed. Validate or run to refresh status.',
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
        max_concurrency: 5,
        use_cache: currentTab.useCache,
        cache_key: currentTab.cacheKey,
      };
    }

    return body;
  }

  async function validateWorkflow() {
    const currentTab = requireActiveTab(activeTab);
    cancelScheduledValidation(currentTab.id);
    const valid = await validateWorkflowByTabId(currentTab.id);

    if (valid) {
      setToastMessage('Workflow is valid.');
    }
  }

  function validateActiveWorkflowOnBlur() {
    scheduleActiveWorkflowValidation();
  }

  async function validateWorkflowByTabId(tabId: string) {
    const currentTab = tabsRef.current.find((tab) => tab.id === tabId);

    if (!currentTab || currentTab.runState === 'running') {
      return false;
    }

    const operationToken = beginWorkflowOperation(currentTab, WorkflowOperationKind.Validate);
    updateTab(currentTab.id, (tab) => ({ ...tab, validationState: 'running', message: 'Validating workflow...' }));

    try {
      const response = await fetch('/validate', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(requestBody(currentTab, false)),
      });
      const payload = await responsePayload(response);

      if (!workflowOperationIsCurrent(operationToken)) {
        return false;
      }

      if (!response.ok || payload.valid !== true) {
        const diagnostic = diagnosticFromErrorPayload(payload);
        updateTab(currentTab.id, (tab) => ({
          ...tab,
          validationState: 'invalid',
          message: diagnostic?.message ?? stringPayloadValue(payload.details) ?? stringPayloadValue(payload.error) ?? 'Workflow is invalid.',
          runtimeDiagnostic: diagnostic ?? tab.runtimeDiagnostic,
        }));

        return false;
      }

      updateTab(currentTab.id, (tab) => ({ ...tab, validationState: 'valid', message: 'Workflow is valid.' }));

      return true;
    } catch (error) {
      if (workflowOperationIsCurrent(operationToken)) {
        updateTab(currentTab.id, (tab) => ({ ...tab, validationState: 'idle', message: `Validation unavailable: ${errorMessage(error)}` }));
      }

      return false;
    }
  }

  async function formatWorkflow() {
    const currentTab = requireActiveTab(activeTab);
    cancelScheduledValidation(currentTab.id);
    invalidateWorkflowOperation(currentTab.id, WorkflowOperationKind.Validate);
    const operationToken = beginWorkflowOperation(currentTab, WorkflowOperationKind.Format);
    updateTab(currentTab.id, (tab) => ({ ...tab, message: 'Formatting workflow...' }));

    try {
      const response = await fetch('/format', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ workflow_source: workflowSourceWithoutMetadata(currentTab.source) }),
      });
      const payload = await responsePayload(response);

      if (!workflowOperationIsCurrent(operationToken)) {
        return;
      }

      const formattedWorkflowSource = stringPayloadValue(payload.formatted_workflow_source);

      if (!response.ok || !formattedWorkflowSource) {
        const diagnostic = diagnosticFromErrorPayload(payload);
        updateTab(currentTab.id, (tab) => ({
          ...tab,
          validationState: 'invalid',
          message: diagnostic?.message ?? stringPayloadValue(payload.error) ?? 'Unable to format workflow.',
          runtimeDiagnostic: diagnostic ?? tab.runtimeDiagnostic,
        }));

        return;
      }

      const parsedResult = parseWorkflowSourceFragments(formattedWorkflowSource, currentTab.name);
      const formattedCodeFragments = preserveWorkflowCodeFragmentIdentities(parsedResult.fragments, currentTab.codeFragments);

      updateTab(currentTab.id, (tab) => ({
        ...tab,
        source: workflowSourceFromCodeFragments(formattedCodeFragments, parsedResult.useMarkers),
        codeFragments: formattedCodeFragments,
        activeCodeFragmentId:
          formattedCodeFragments.find((fragment) => fragment.id === tab.activeCodeFragmentId)?.id
          ?? formattedCodeFragments[0]?.id
          ?? tab.activeCodeFragmentId,
        codeFragmentsUseMarkers: parsedResult.useMarkers,
        validationState: 'valid',
        message: 'Workflow formatted.',
        graphState: 'idle',
        graphMessage: 'Graph needs to be regenerated after formatting.',
        graphData: null,
        updatedAt: Date.now(),
      }));
      setToastMessage('Workflow formatted.');
    } catch (error) {
      if (workflowOperationIsCurrent(operationToken)) {
        updateTab(currentTab.id, (tab) => ({ ...tab, message: `Formatting unavailable: ${errorMessage(error)}` }));
      }
    }
  }

  async function runWorkflow() {
    if (!canRun) {
      return;
    }

    const currentTab = requireActiveTab(activeTab);
    cancelScheduledValidation(currentTab.id);
    invalidateAllWorkflowOperations(currentTab.id);
    runSequenceRef.current += 1;

    const ownership: WorkflowRunOwnership = {
      sequence: runSequenceRef.current,
      abortController: new AbortController(),
      runIdentifier: null,
      cancellationRequested: false,
      snapshot: snapshotForTab(currentTab),
    };
    runOwnershipByTabRef.current.set(currentTab.id, ownership);
    setEventsOpen(true);
    setMaximizedWorkflowPanel(null);
    updateTab(currentTab.id, (tab) => ({
      ...tab,
      runState: 'running',
      validationState: 'idle',
      message: 'Running workflow...',
      outputJson: '',
      eventLog: [],
      runtimeDiagnostic: null,
    }));

    try {
      const response = await fetch('/execute', {
        method: 'POST',
        headers: {
          accept: 'text/event-stream',
          'content-type': 'application/json',
        },
        body: JSON.stringify(requestBody(currentTab, true)),
        signal: ownership.abortController.signal,
      });

      if (!response.ok || !response.body) {
        const payload = await responsePayload(response).catch(() => ({}));
        throw responseDiagnosticError(payload, `Request failed with ${response.status}`);
      }

      if (!runOwnershipIsCurrent(currentTab.id, ownership.sequence)) {
        ownership.abortController.abort();

        return;
      }

      ownership.runIdentifier = response.headers.get(runIdentifierHeader);

      if (ownership.cancellationRequested && ownership.runIdentifier) {
        const cancellationResponse = await cancelWorkflowRun(ownership.runIdentifier);

        if (applyCancellationTransition(currentTab.id, ownership, cancellationResponse.transition)) {
          return;
        }
      }

      const events = await readWorkflowEventStream(
        response,
        currentTab.id,
        ownership.abortController.signal,
        acceptSseChunk,
        updateTab,
        (diagnostic, resumeAfter, historyLoss) => requestStreamGapResumeConfirmation(
          currentTab.id,
          ownership.sequence,
          diagnostic,
          resumeAfter,
          historyLoss,
          ownership.abortController.signal,
        ),
      );

      if (!runOwnershipIsCurrent(currentTab.id, ownership.sequence)) {
        return;
      }

      const terminalEvent = latestTerminalWorkflowEvent(events);

      if (!terminalEvent) {
        throw new Error('Workflow stream ended without a terminal event.');
      }

      if (terminalEvent.kind === ExecutorEventKind.WorkflowFailed) {
        updateTab(currentTab.id, (tab) => ({
          ...tab,
          runState: 'failed',
          message: terminalEvent.diagnostic?.message ?? terminalEvent.message ?? 'Workflow failed.',
          runtimeDiagnostic: terminalEvent.diagnostic ?? tab.runtimeDiagnostic,
        }));

        return;
      }

      if (terminalEvent.kind === ExecutorEventKind.WorkflowCancelled) {
        updateTab(currentTab.id, (tab) => ({
          ...tab,
          runState: 'cancelled',
          message: terminalEvent.diagnostic?.message ?? terminalEvent.message ?? 'Workflow cancelled.',
          runtimeDiagnostic: terminalEvent.diagnostic ?? tab.runtimeDiagnostic,
        }));

        return;
      }

      updateTab(currentTab.id, (tab) => ({
        ...tab,
        runState: 'completed',
        validationState: workflowSnapshotsMatch(snapshotForTab(tab), ownership.snapshot) ? 'valid' : 'idle',
        message: workflowSnapshotsMatch(snapshotForTab(tab), ownership.snapshot)
          ? 'Workflow completed.'
          : 'Workflow completed for an earlier editor revision. Run again to refresh the result.',
      }));
    } catch (error) {
      if (!runOwnershipIsCurrent(currentTab.id, ownership.sequence)) {
        return;
      }

      if (error instanceof DOMException && error.name === 'AbortError') {
        updateTab(currentTab.id, (tab) => (
          tab.runState === 'running'
            ? { ...tab, runState: 'idle', message: 'Run connection stopped.' }
            : tab
        ));

        return;
      }

      const diagnostic = error instanceof WorkflowDiagnosticError ? error.diagnostic : null;
      updateTab(currentTab.id, (tab) => ({
        ...tab,
        runState: 'failed',
        message: diagnostic?.message ?? errorMessage(error),
        runtimeDiagnostic: diagnostic ?? tab.runtimeDiagnostic,
      }));
    } finally {
      removeStreamGapConfirmationRequests(currentTab.id, ownership.sequence);
      if (runOwnershipIsCurrent(currentTab.id, ownership.sequence)) {
        runOwnershipByTabRef.current.delete(currentTab.id);
      }
    }
  }

  async function stopRun() {
    const currentTab = requireActiveTab(activeTab);
    const ownership = runOwnershipByTabRef.current.get(currentTab.id);

    if (!ownership) {
      updateTab(currentTab.id, (tab) => ({ ...tab, runState: 'idle', message: 'Run connection was lost. Start a new run to continue.' }));

      return;
    }

    ownership.cancellationRequested = true;

    if (!ownership.runIdentifier) {
      updateTab(currentTab.id, (tab) => ({ ...tab, message: 'Waiting for the server run identifier before cancelling...' }));

      return;
    }

    updateTab(currentTab.id, (tab) => ({ ...tab, message: 'Requesting workflow cancellation...' }));

    try {
      const cancellationResponse = await cancelWorkflowRun(ownership.runIdentifier);
      applyCancellationTransition(currentTab.id, ownership, cancellationResponse.transition);
    } catch (error) {
      const diagnostic = error instanceof WorkflowDiagnosticError ? error.diagnostic : null;
      updateTab(currentTab.id, (tab) => ({
        ...tab,
        message: diagnostic?.message ?? `Server cancellation failed. ${errorMessage(error)}`,
        runtimeDiagnostic: diagnostic ?? tab.runtimeDiagnostic,
      }));
      setToastMessage('Cancellation was not confirmed; the event stream is still active.');
    }
  }

  function applyCancellationTransition(tabId: string, ownership: WorkflowRunOwnership, transition: CancellationTransition) {
    if (transition === CancellationTransition.Accepted) {
      updateTab(tabId, (tab) => ({ ...tab, message: 'Cancellation accepted. Waiting for the terminal workflow event...' }));

      return false;
    }

    if (transition === CancellationTransition.AlreadyRequested) {
      updateTab(tabId, (tab) => ({ ...tab, message: 'Cancellation was already requested. Waiting for the terminal workflow event...' }));

      return false;
    }

    if (transition === CancellationTransition.AlreadyTerminal) {
      updateTab(tabId, (tab) => ({ ...tab, message: 'The workflow is already terminal. Waiting for the retained terminal event...' }));

      return false;
    }

    const diagnostic = unknownRunCancellationDiagnostic();
    updateTab(tabId, (tab) => ({
      ...tab,
      runState: 'failed',
      message: diagnostic.message,
      runtimeDiagnostic: diagnostic,
    }));
    ownership.abortController.abort();

    return true;
  }

  function requestStreamGapResumeConfirmation(
    tabId: string,
    runSequence: number,
    diagnostic: ExecutionDiagnostic,
    resumeAfter: string | null,
    historyLoss: boolean,
    abortSignal: AbortSignal,
  ) {
    if (abortSignal.aborted) {
      return Promise.resolve(false);
    }

    return new Promise<boolean>((resolve) => {
      const requestIdentifier = streamGapConfirmationSequenceRef.current;
      streamGapConfirmationSequenceRef.current += 1;
      const abortListener = () => removeStreamGapConfirmationRequests(tabId, runSequence);
      const request: StreamGapConfirmationRequest = {
        requestIdentifier,
        tabId,
        runSequence,
        diagnostic,
        resumeAfter,
        historyLoss,
        abortSignal,
        abortListener,
        resolve,
      };

      abortSignal.addEventListener('abort', abortListener, { once: true });
      streamGapConfirmationQueueRef.current = [...streamGapConfirmationQueueRef.current, request];
      setStreamGapConfirmationQueue(streamGapConfirmationQueueRef.current);
    });
  }

  function removeStreamGapConfirmationRequests(tabId: string, runSequence: number) {
    const removedRequests = streamGapConfirmationQueueRef.current.filter((request) => (
      request.tabId === tabId && request.runSequence === runSequence
    ));

    if (removedRequests.length === 0) {
      return;
    }

    streamGapConfirmationQueueRef.current = streamGapConfirmationQueueRef.current.filter((request) => (
      request.tabId !== tabId || request.runSequence !== runSequence
    ));

    for (const request of removedRequests) {
      request.abortSignal.removeEventListener('abort', request.abortListener);
      request.resolve(false);
    }

    setStreamGapConfirmationQueue([...streamGapConfirmationQueueRef.current]);
  }

  function resolveStreamGapResumeConfirmation(requestIdentifier: number | undefined, accepted: boolean) {
    const request = streamGapConfirmationQueueRef.current[0];

    if (!request || request.requestIdentifier !== requestIdentifier) {
      return;
    }

    streamGapConfirmationQueueRef.current = streamGapConfirmationQueueRef.current.filter((queuedRequest) => (
      queuedRequest.requestIdentifier !== request.requestIdentifier
    ));
    request.abortSignal.removeEventListener('abort', request.abortListener);
    setStreamGapConfirmationQueue([...streamGapConfirmationQueueRef.current]);
    request.resolve(accepted);
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

  function toggleMaximizedWorkflowPanel(panel: Exclude<MaximizedWorkflowPanel, null>) {
    setMaximizedWorkflowPanel((currentPanel) => (currentPanel === panel ? null : panel));
  }

  function formatRuntimeJson(fieldName: 'inputJson' | 'secretsJson') {
    const currentTab = requireActiveTab(activeTab);
    cancelScheduledValidation(currentTab.id);
    invalidateWorkflowOperation(currentTab.id, WorkflowOperationKind.Validate);

    try {
      const parsedValue = parseJsonObject(currentTab[fieldName], fieldName === 'inputJson' ? 'input' : 'secrets');
      const formattedJson = JSON.stringify(parsedValue, null, 2);

      updateTab(currentTab.id, (tab) => ({
        ...tab,
        [fieldName]: formattedJson,
        validationState: 'idle',
        message: `${fieldName === 'inputJson' ? 'Input' : 'Secrets'} JSON formatted.`,
        graphState: fieldName === 'secretsJson' ? 'idle' : tab.graphState,
        graphMessage: fieldName === 'secretsJson' ? 'Graph needs to be regenerated after secrets changes.' : tab.graphMessage,
        graphData: fieldName === 'secretsJson' ? null : tab.graphData,
        updatedAt: Date.now(),
      }));
      setToastMessage(`${fieldName === 'inputJson' ? 'Input' : 'Secrets'} JSON formatted.`);
    } catch (error) {
      updateTab(currentTab.id, (tab) => ({ ...tab, message: errorMessage(error) }));
    }
  }

  function formatActiveEditor() {
    if (activeTab) {
      cancelScheduledValidation(activeTab.id);
    }

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
    const currentTab = requireActiveTab(activeTab);
    const parsedResult = parseWorkflowSourceFragments(template.source, template.name);
    cancelScheduledValidation(currentTab.id);
    invalidateAllWorkflowOperations(currentTab.id);

    updateTab(currentTab.id, (tab) => ({
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
      runtimeDiagnostic: null,
      graphState: 'idle',
      graphMessage: 'Graph needs to be regenerated after loading this template.',
      graphData: null,
      updatedAt: Date.now(),
    }));
  }

  async function exportWorkflowSource(includeSecrets: boolean) {
    const currentTab = requireActiveTab(activeTab);
    cancelScheduledValidation(currentTab.id);
    invalidateWorkflowOperation(currentTab.id, WorkflowOperationKind.Validate);
    const operationToken = beginWorkflowOperation(currentTab, WorkflowOperationKind.Export);

    try {
      updateTab(currentTab.id, (tab) => ({ ...tab, message: 'Formatting workflow before copying...' }));

      const response = await fetch('/format', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ workflow_source: workflowSourceWithoutMetadata(currentTab.source) }),
      });
      const payload = await responsePayload(response);
      const formattedWorkflowSource = stringPayloadValue(payload.formatted_workflow_source);

      if (!workflowOperationIsCurrent(operationToken)) {
        return;
      }

      if (!response.ok || !formattedWorkflowSource) {
        throw responseDiagnosticError(payload, 'Unable to format workflow before copying.');
      }

      const parsedResult = parseWorkflowSourceFragments(formattedWorkflowSource, currentTab.name);
      const formattedCodeFragments = preserveWorkflowCodeFragmentIdentities(parsedResult.fragments, currentTab.codeFragments);
      const formattedSource = workflowSourceFromCodeFragments(formattedCodeFragments, parsedResult.useMarkers);
      const clipboardSource = includeSecrets
        ? workflowSourceWithMetadata(formattedSource, currentTab.name, currentTab.inputJson, currentTab.secretsJson)
        : formattedSource;

      await navigator.clipboard.writeText(clipboardSource);

      if (!workflowOperationIsCurrent(operationToken)) {
        return;
      }

      const successMessage = includeSecrets ? 'Portable workflow bundle copied with secrets.' : 'Workflow source copied without runtime secrets.';
      setToastMessage(successMessage);
      updateTab(currentTab.id, (tab) => ({
        ...tab,
        source: formattedSource,
        codeFragments: formattedCodeFragments,
        activeCodeFragmentId:
          formattedCodeFragments.find((fragment) => fragment.id === tab.activeCodeFragmentId)?.id
          ?? formattedCodeFragments[0]?.id
          ?? tab.activeCodeFragmentId,
        codeFragmentsUseMarkers: parsedResult.useMarkers,
        validationState: 'valid',
        message: successMessage,
        graphState: 'idle',
        graphMessage: 'Graph needs to be regenerated after formatting.',
        graphData: null,
        updatedAt: Date.now(),
      }));
    } catch (error) {
      if (workflowOperationIsCurrent(operationToken)) {
        updateTab(currentTab.id, (tab) => ({ ...tab, message: errorMessage(error) }));
      }
    }
  }

  async function copyWorkflowOutput() {
    const currentTab = requireActiveTab(activeTab);

    if (!currentTab.outputJson) {
      return;
    }

    try {
      await navigator.clipboard.writeText(currentTab.outputJson);
      setToastMessage('Output copied to clipboard.');
      updateTab(currentTab.id, (tab) => ({ ...tab, message: 'Output copied to clipboard.' }));
    } catch (error) {
      updateTab(currentTab.id, (tab) => ({ ...tab, message: errorMessage(error) }));
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
    cancelScheduledValidation(currentTab.id);
    await loadGraphByTabId(currentTab.id);
  }

  async function loadGraphByTabId(tabId: string) {
    const currentTab = tabsRef.current.find((tab) => tab.id === tabId);

    if (!currentTab) {
      return;
    }

    invalidateWorkflowOperation(currentTab.id, WorkflowOperationKind.Validate);
    const operationToken = beginWorkflowOperation(currentTab, WorkflowOperationKind.Graph);
    updateTab(currentTab.id, (tab) => ({ ...tab, graphState: 'loading', graphMessage: 'Building workflow graph...' }));

    try {
      const response = await fetch('/graph', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(requestBody(currentTab, false)),
      });
      const payload = await responsePayload(response);

      if (!workflowOperationIsCurrent(operationToken)) {
        return;
      }

      if (!response.ok || payload.valid !== true) {
        const diagnostic = diagnosticFromErrorPayload(payload);
        updateTab(currentTab.id, (tab) => ({
          ...tab,
          graphState: 'failed',
          graphMessage: diagnostic?.message ?? stringPayloadValue(payload.details) ?? stringPayloadValue(payload.error) ?? 'Unable to build workflow graph.',
          graphData: null,
          runtimeDiagnostic: diagnostic ?? tab.runtimeDiagnostic,
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
      if (workflowOperationIsCurrent(operationToken)) {
        updateTab(currentTab.id, (tab) => ({
          ...tab,
          graphState: 'failed',
          graphMessage: errorMessage(error),
          graphData: null,
        }));
      }
    }
  }

  function acceptSseChunk(chunk: string, tabId: string) {
    const event = parseSseChunk(chunk);

    if (!event) {
      return null;
    }

    enqueueWorkflowEvent(tabId, event);

    if (event.kind === ExecutorEventKind.StreamGap && event.diagnostic) {
      const diagnostic = event.diagnostic;
      updateTab(tabId, (tab) => ({
        ...tab,
        message: 'Workflow event history has a gap. Reconnecting from the last confirmed event...',
        runtimeDiagnostic: diagnostic,
      }));
    }

    if (event.kind === ExecutorEventKind.CacheDegraded && event.diagnostic) {
      const diagnostic = event.diagnostic;
      updateTab(tabId, (tab) => ({
        ...tab,
        message: diagnostic.message,
        runtimeDiagnostic: diagnostic,
      }));
    }

    if (isTerminalWorkflowEvent(event)) {
      flushPendingWorkflowEvents();
    }

    return event;
  }

  function enqueueWorkflowEvent(tabId: string, event: ExecutorEvent) {
    const pendingEvents = pendingEventBatchesRef.current.get(tabId) ?? [];
    const retainedEvents = retainEventHistory(pendingEvents, [event], true);
    pendingEventBatchesRef.current.set(tabId, retainedEvents);

    if (eventBatchFlushFrameRef.current !== null) {
      return;
    }

    eventBatchFlushFrameRef.current = window.requestAnimationFrame(flushPendingWorkflowEvents);
  }

  function flushPendingWorkflowEvents() {
    if (eventBatchFlushFrameRef.current !== null) {
      window.cancelAnimationFrame(eventBatchFlushFrameRef.current);
      eventBatchFlushFrameRef.current = null;
    }

    if (pendingEventBatchesRef.current.size === 0) {
      return;
    }

    const pendingEventBatches = pendingEventBatchesRef.current;
    pendingEventBatchesRef.current = new Map();

    setTabs((currentTabs) => currentTabs.map((tab) => {
      const pendingEvents = pendingEventBatches.get(tab.id);

      if (!pendingEvents || pendingEvents.length === 0) {
        return tab;
      }

      const completedEvent = latestCompletedWorkflowEvent(pendingEvents);
      const outputJson = completedEvent && isRecord(completedEvent.data) && 'output' in completedEvent.data
        ? JSON.stringify(completedEvent.data.output, null, 2)
        : tab.outputJson;

      return {
        ...tab,
        eventLog: retainEventHistory(tab.eventLog, pendingEvents, true),
        outputJson,
      };
    }));
  }

  function latestCompletedWorkflowEvent(events: ExecutorEvent[]) {
    for (let eventIndex = events.length - 1; eventIndex >= 0; eventIndex -= 1) {
      const event = events[eventIndex];

      if (event.kind === ExecutorEventKind.WorkflowCompleted) {
        return event;
      }
    }

    return null;
  }

  const playgroundControlsSentinel = <div ref={setPlaygroundControlsSentinelElement} className="playground__controls-sentinel" aria-hidden="true" />;

  const playgroundControls = activeTab ? (
    <div className="playground__controls" data-stuck={playgroundControlsStuck ? 'true' : 'false'}>
      <nav className="playground-mode-switch" aria-label="Playground mode">
        <Button
          variant={activeView === 'workflow' ? 'secondary' : 'ghost'}
          size="lg"
          className="playground-mode-switch__button"
          aria-pressed={activeView === 'workflow'}
          onClick={() => setTabView('workflow')}
        >
          <Workflow /> Workflow
        </Button>
        <Button
          variant={activeView === 'graph' ? 'secondary' : 'ghost'}
          size="lg"
          className="playground-mode-switch__button"
          aria-pressed={activeView === 'graph'}
          onClick={() => setTabView('graph')}
        >
          <GitBranch /> Graph
        </Button>
      </nav>

      <div className="playground-actions">
        <div className="playground-cache-controls playground-actions__desktop" role="group" aria-label="Cache settings">
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
          <ActionTooltip label="Regenerate the cache key for future runs">
            <Button variant="ghost" size="icon-lg" aria-label="Regenerate cache key" onClick={purgeCache}>
              <DatabaseZap />
            </Button>
          </ActionTooltip>
        </div>

        {activeView === 'workflow' ? (
          <details className="playground-command-menu">
            <summary><Menu /> Actions</summary>
            <div className="playground-command-menu__content">
              <Button variant="ghost" size="sm" disabled={activeTab.runState === 'running'} onClick={() => void exportWorkflowSource(false)}><Copy /> Copy source</Button>
              <Button variant="ghost" size="sm" disabled={activeTab.runState === 'running'} onClick={() => setIncludeSecretsConfirmationOpen(true)}><KeyRound /> Copy with secrets</Button>
              <Button variant="ghost" size="sm" disabled={activeTab.runState === 'running'} onClick={formatActiveEditor}><RefreshCcw /> Format</Button>
              <Button variant="ghost" size="sm" disabled={activeTab.runState === 'running'} onClick={() => void validateWorkflow()}><CheckCircle2 /> Validate</Button>
              <Button variant="ghost" size="sm" disabled={activeTab.runState === 'running'} onClick={addCodeFragment}><Plus /> Add fragment</Button>
              <Button variant="ghost" size="sm" aria-pressed={activeTab.useCache} onClick={toggleCache}><Database /> {activeTab.useCache ? 'Disable cache' : 'Enable cache'}</Button>
              <Button variant="ghost" size="sm" onClick={purgeCache}><DatabaseZap /> Regenerate cache key</Button>
            </div>
          </details>
        ) : null}

        <div className="playground-run-control">
          {activeTab.runState === 'running' ? (
            <ActionTooltip label="Stop the current workflow run">
              <Button variant="destructive" size="lg" onClick={() => void stopRun()}><Square /> Stop</Button>
            </ActionTooltip>
          ) : (
            <ActionTooltip label="Run the workflow with the current input and secrets">
              <Button className="playground-actions__run" disabled={!canRun} size="lg" onClick={() => void runWorkflow()}><Play /> Run workflow</Button>
            </ActionTooltip>
          )}
        </div>
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
                    <Button className="playground__theme-toggle" variant="ghost" size="icon-lg" aria-label={darkMode ? 'Use light theme' : 'Use dark theme'} onClick={toggleTheme}>
                      {darkMode ? <Sun /> : <Moon />}
                    </Button>
                  </ActionTooltip>
                </div>
              </header>

              <Tabs value={activeTab?.id ?? ''} onValueChange={setActiveTabId} className="playground__tabs">
                <TabsList variant="line" className="playground-tabs__list h-auto flex-nowrap justify-start gap-3 bg-transparent p-0">
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
                        <TabsTrigger value={tab.id} className="playground-tab-chip__trigger" aria-invalid={tab.validationState === 'invalid' || tab.runState === 'failed'}>
                          <span className="playground-tab-chip__dot" />
                          <span className="playground-tab-chip__title">{tab.name}</span>
                          <RunStateBadge state={tab.runState} />
                        </TabsTrigger>
                      )}
                      actions={[
                        { label: `Move ${tab.name} left`, icon: <ArrowLeft />, disabled: tabs[0]?.id === tab.id, onClick: () => moveWorkflowTab(tab.id, -1) },
                        { label: `Move ${tab.name} right`, icon: <ArrowRight />, disabled: tabs.at(-1)?.id === tab.id, onClick: () => moveWorkflowTab(tab.id, 1) },
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
                        {playgroundControlsSentinel}
                        {playgroundControls}
                        <div className="workflow-workspace">
                        <div className="workflow-layout__sticky-scope workflow-editor-pane">

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
                                  <ActionTooltip label="Copy formatted workflow source without input or secrets">
                                    <Button variant="ghost" size="sm" disabled={activeTab.runState === 'running'} onClick={() => void exportWorkflowSource(false)}><Copy /> Copy source</Button>
                                  </ActionTooltip>
                                  <ActionTooltip label="Copy a portable bundle after confirming that secrets are included">
                                    <Button variant="ghost" size="sm" disabled={activeTab.runState === 'running'} onClick={() => setIncludeSecretsConfirmationOpen(true)}><KeyRound /> Include secrets</Button>
                                  </ActionTooltip>
                                  <ActionTooltip label="Format the active editor contents">
                                    <Button variant="ghost" size="sm" aria-label="Format active editor" disabled={activeTab.runState === 'running'} onClick={formatActiveEditor}><RefreshCcw /> Format</Button>
                                  </ActionTooltip>
                                  <ActionTooltip label="Validate the workflow without running agents">
                                    <Button variant="ghost" size="sm" aria-label="Validate workflow" disabled={activeTab.runState === 'running'} onClick={() => void validateWorkflow()}><CheckCircle2 /> Validate</Button>
                                  </ActionTooltip>
                                  <ActionTooltip label="Add a new workflow code fragment">
                                    <Button variant="outline" size="sm" disabled={activeTab.runState === 'running'} onClick={addCodeFragment}><Plus /> Fragment</Button>
                                  </ActionTooltip>
                                </div>
                              </div>
                              <div className="workflow-editor-tabs" role="toolbar" aria-label="Workflow editor views">
                                <div className="workflow-editor-tabs__fragments" role="group" aria-label="Workflow code fragments">
                                  {activeTab.codeFragments.map((fragment) => (
                                    <PlaygroundTabChip
                                      key={fragment.id}
                                      size="small"
                                      draggable={activeTab.runState !== 'running'}
                                      active={activeEditorView === 'code' && fragment.id === activeTab.activeCodeFragmentId}
                                      dragging={draggedCodeFragmentId === fragment.id}
                                      dragOver={dragOverCodeFragmentId === fragment.id}
                                      onDragStart={() => handleCodeFragmentDragStart(fragment.id)}
                                      onDragOver={() => handleCodeFragmentDragOver(fragment.id)}
                                      onDrop={() => handleCodeFragmentDrop(fragment.id)}
                                      onDragEnd={clearCodeFragmentDragState}
                                      trigger={(
                                        <button type="button" className="playground-tab-chip__trigger" aria-pressed={activeEditorView === 'code' && fragment.id === activeTab.activeCodeFragmentId} onClick={() => setActiveCodeFragment(fragment.id)}>
                                          <span className="playground-tab-chip__title">{fragment.name}</span>
                                        </button>
                                      )}
                                      actions={[
                                        { label: `Move ${fragment.name} left`, icon: <ArrowLeft />, disabled: activeTab.runState === 'running' || activeTab.codeFragments[0]?.id === fragment.id, onClick: () => moveCodeFragment(fragment.id, -1) },
                                        { label: `Move ${fragment.name} right`, icon: <ArrowRight />, disabled: activeTab.runState === 'running' || activeTab.codeFragments.at(-1)?.id === fragment.id, onClick: () => moveCodeFragment(fragment.id, 1) },
                                        { label: `Rename ${fragment.name}`, icon: <Pencil />, disabled: activeTab.runState === 'running', onClick: () => openCodeFragmentRenameDialog(activeTab.id, fragment.id) },
                                        { label: `Close ${fragment.name}`, icon: <Trash2 />, disabled: activeTab.runState === 'running', onClick: () => closeCodeFragment(fragment.id) },
                                      ]}
                                    />
                                  ))}
                                </div>

                                <div className="workflow-editor-tabs__variables" role="group" aria-label="Workflow runtime values">
                                  <PlaygroundTabChip
                                    size="small"
                                    active={activeEditorView === 'input'}
                                    draggable={false}
                                    dragging={false}
                                    dragOver={false}
                                    trigger={(
                                      <button type="button" className="playground-tab-chip__trigger" aria-pressed={activeEditorView === 'input'} onClick={() => setWorkflowEditorView('input')}>
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
                                      <button type="button" className="playground-tab-chip__trigger" aria-pressed={activeEditorView === 'secrets'} onClick={() => setWorkflowEditorView('secrets')}>
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
                                  readOnly={activeTab.runState === 'running'}
                                  ariaLabel={`${activeCodeFragment.name} workflow source editor`}
                                  jumpTarget={activeEditorJumpTarget}
                                  onChange={updateActiveCodeFragmentSource}
                                  onBlur={validateActiveWorkflowOnBlur}
                                  onDefinitionJump={jumpToFullDocumentPosition}
                                />
                              ) : null}
                              {activeEditorView === 'input' ? (
                                <JsonCodeEditor
                                  key={`${activeTab.id}-input`}
                                  value={activeTab.inputJson}
                                  fullEditor
                                  readOnly={activeTab.runState === 'running'}
                                  ariaLabel="Workflow input JSON editor"
                                  className="workflow-editor__json"
                                  onChange={(inputJson) => updateActiveTab((tab) => ({
                                    ...tab,
                                    inputJson,
                                    validationState: 'idle',
                                    message: 'Workflow input changed. Validate or run to refresh status.',
                                    updatedAt: Date.now(),
                                  }))}
                                />
                              ) : null}
                              {activeEditorView === 'secrets' ? (
                                <JsonCodeEditor
                                  key={`${activeTab.id}-secrets`}
                                  value={activeTab.secretsJson}
                                  fullEditor
                                  readOnly={activeTab.runState === 'running'}
                                  ariaLabel="Workflow secrets JSON editor"
                                  className="workflow-editor__json"
                                  onChange={(secretsJson) => updateActiveTab((tab) => ({
                                    ...tab,
                                    secretsJson,
                                    validationState: 'idle',
                                    message: 'Workflow secrets changed. Validate or run to refresh status.',
                                    graphState: 'idle',
                                    graphMessage: 'Graph needs to be regenerated after secrets changes.',
                                    graphData: null,
                                    updatedAt: Date.now(),
                                  }))}
                                />
                              ) : null}
                              <div className={`workflow-editor__message workflow-editor__message--${editorMessageTone}`} role={editorMessageTone === 'error' ? 'alert' : 'status'} aria-live="polite" aria-atomic="true">
                                <span className="workflow-editor__message-line workflow-editor__message-line--full">{editorMessage}</span>
                              </div>
                            </Card>
                          </div>
                        </div>

                        <aside className="workflow-layout__bottom workflow-inspector" data-maximized={maximizedWorkflowPanel ?? 'none'} aria-label="Workflow execution inspector">
                          <PanelCard
                            collapsible
                            open={problemsOpen}
                            title="Problems"
                            description={activeProblems.length === 0 ? 'No current workflow problems.' : `${activeProblems.length} current problem${activeProblems.length === 1 ? '' : 's'}.`}
                            className="workflow-log-panel workflow-log-panel--problems"
                            bodyClassName="workflow-log-panel__body"
                            onToggle={() => setProblemsOpen((currentValue) => !currentValue)}
                          >
                            {activeProblems.length > 0 ? (
                              <ul className="workflow-problems" aria-live="polite">
                                {activeProblems.map((problem) => (
                                  <li key={problem.key} data-tone={problem.tone}>
                                    <strong>{problem.message}</strong>
                                    {problem.diagnostic ? (
                                      <details>
                                        <summary>Diagnostic details</summary>
                                        <pre>{formatExecutionDiagnosticData(problem.diagnostic)}</pre>
                                      </details>
                                    ) : null}
                                  </li>
                                ))}
                              </ul>
                            ) : (
                              <div className="empty-state compact" role="status">No validation or run failures.</div>
                            )}
                          </PanelCard>
                          <PanelCard
                            collapsible
                            open={outputOpen}
                            title="Output"
                            description="Final workflow output payload."
                            className="workflow-log-panel workflow-log-panel--output"
                            bodyClassName="workflow-log-panel__body"
                            onToggle={() => setOutputOpen((currentValue) => !currentValue)}
                            actions={(
                              <>
                                <ActionTooltip label="Copy output to clipboard">
                                  <Button variant="ghost" size="icon" aria-label="Copy output to clipboard" disabled={!activeTab.outputJson} onClick={copyWorkflowOutput}>
                                    <Copy />
                                  </Button>
                                </ActionTooltip>
                                <ActionTooltip label={maximizedWorkflowPanel === 'output' ? 'Restore output panel size' : 'Maximize output panel'}>
                                  <Button variant="ghost" size="icon" aria-label={maximizedWorkflowPanel === 'output' ? 'Restore output panel size' : 'Maximize output panel'} onClick={() => toggleMaximizedWorkflowPanel('output')}>
                                    {maximizedWorkflowPanel === 'output' ? <Minimize2 /> : <Maximize2 />}
                                  </Button>
                                </ActionTooltip>
                              </>
                            )}
                          >
                            <OutputBox runState={activeTab.runState} outputJson={activeTab.outputJson} />
                          </PanelCard>
                          <PanelCard
                            collapsible
                            open={eventsOpen}
                            title="Server events"
                            description={`${activeTab.eventLog.length} streamed events.`}
                            className="workflow-log-panel workflow-log-panel--events"
                            bodyClassName="workflow-log-panel__body"
                            onToggle={() => setEventsOpen((currentValue) => !currentValue)}
                            actions={(
                              <ActionTooltip label={maximizedWorkflowPanel === 'events' ? 'Restore server events panel size' : 'Maximize server events panel'}>
                                <Button variant="ghost" size="icon" aria-label={maximizedWorkflowPanel === 'events' ? 'Restore server events panel size' : 'Maximize server events panel'} onClick={() => toggleMaximizedWorkflowPanel('events')}>
                                  {maximizedWorkflowPanel === 'events' ? <Minimize2 /> : <Maximize2 />}
                                </Button>
                              </ActionTooltip>
                            )}
                          >
                            <EventLog events={activeTab.eventLog} eventGroupingMode={eventGroupingMode} onEventGroupingModeChange={setEventGroupingMode} />
                          </PanelCard>
                        </aside>
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
            <label htmlFor="rename-workflow-input" className="rename-dialog__label">Name</label>
            <input
              id="rename-workflow-input"
              autoFocus
              value={renameDraft}
              onChange={(event) => setRenameDraft(event.target.value)}
              className="rename-dialog__input"
              placeholder={renameDialogTarget?.kind === 'codeFragment' ? 'Code fragment name' : 'Workflow tab name'}
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

      <Dialog open={includeSecretsConfirmationOpen} onOpenChange={setIncludeSecretsConfirmationOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Copy workflow with secrets?</DialogTitle>
            <DialogDescription>
              This portable bundle includes the current input and plaintext secrets. Only paste it into a trusted destination.
            </DialogDescription>
          </DialogHeader>

          <DialogFooter>
            <DialogClose asChild>
              <Button variant="outline" type="button">Cancel</Button>
            </DialogClose>
            <Button
              type="button"
              variant="destructive"
              onClick={() => {
                setIncludeSecretsConfirmationOpen(false);
                void exportWorkflowSource(true);
              }}
            >
              <KeyRound /> Copy with secrets
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={visibleStreamGapConfirmationRequest !== null}
        onOpenChange={(open) => {
          if (!open) {
            resolveStreamGapResumeConfirmation(visibleStreamGapConfirmationRequest?.requestIdentifier, false);
          }
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {visibleStreamGapConfirmationRequest?.historyLoss
                ? 'Resume with missing event history?'
                : 'Resume interrupted event stream?'}
            </DialogTitle>
            <DialogDescription>
              {visibleStreamGapConfirmationRequest?.historyLoss
                ? `Events before ${streamGapOldestAvailable(visibleStreamGapConfirmationRequest.diagnostic) ?? 'the retained window'} are no longer available. Resuming continues from the oldest retained event and cannot reconstruct the missing history.`
                : `The stream stopped after event ${visibleStreamGapConfirmationRequest?.resumeAfter ?? 'the last confirmed event'}. Confirm replay from the last confirmed event.`}
            </DialogDescription>
          </DialogHeader>

          {visibleStreamGapConfirmationRequest ? (
            <JsonCodeEditor
              value={formatExecutionDiagnosticData(visibleStreamGapConfirmationRequest.diagnostic)}
              readOnly
              uncappedHeight
              ariaLabel="Stream history gap diagnostic"
              className="stream-gap-dialog__diagnostic"
            />
          ) : null}

          <DialogFooter>
            <Button variant="outline" type="button" onClick={() => resolveStreamGapResumeConfirmation(visibleStreamGapConfirmationRequest?.requestIdentifier, false)}>Stop following run</Button>
            <Button variant="destructive" type="button" onClick={() => resolveStreamGapResumeConfirmation(visibleStreamGapConfirmationRequest?.requestIdentifier, true)}>
              {visibleStreamGapConfirmationRequest?.historyLoss ? 'Resume with missing history' : 'Resume from last confirmed event'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {toastMessage ? <div className="playground-toast" role="status">{toastMessage}</div> : null}

    </TooltipProvider>
  );
}

function workflowSnapshotsMatch(leftSnapshot: WorkflowContentSnapshot, rightSnapshot: WorkflowContentSnapshot) {
  return (
    leftSnapshot.source === rightSnapshot.source
    && leftSnapshot.inputJson === rightSnapshot.inputJson
    && leftSnapshot.secretsJson === rightSnapshot.secretsJson
  );
}

function workflowProblems(tab: WorkflowTab, jsonValidationError: string | null) {
  const problemsByKey = new Map<string, WorkflowProblem>();

  if (jsonValidationError) {
    problemsByKey.set(`json:${jsonValidationError}`, {
      key: `json:${jsonValidationError}`,
      message: jsonValidationError,
      diagnostic: null,
      tone: 'error',
    });
  }

  if (tab.validationState === 'invalid') {
    const message = tab.message || 'Workflow validation failed.';
    problemsByKey.set(`validation:${message}`, {
      key: `validation:${message}`,
      message,
      diagnostic: null,
      tone: 'error',
    });
  }

  if (tab.runState === 'failed' && !tab.runtimeDiagnostic) {
    const message = tab.message || 'Workflow run failed.';
    problemsByKey.set(`run:${message}`, {
      key: `run:${message}`,
      message,
      diagnostic: null,
      tone: 'error',
    });
  }

  for (const event of tab.eventLog) {
    if (event.diagnostic) {
      addDiagnosticProblem(problemsByKey, event.diagnostic);
    }
  }

  if (tab.runtimeDiagnostic) {
    addDiagnosticProblem(problemsByKey, tab.runtimeDiagnostic);
  }

  return Array.from(problemsByKey.values());
}

function addDiagnosticProblem(problemsByKey: Map<string, WorkflowProblem>, diagnostic: ExecutionDiagnostic) {
  const subjectKey = JSON.stringify(diagnostic.subject);
  const key = `${diagnostic.code}:${diagnostic.stage}:${subjectKey}:${diagnostic.message}`;
  const tone = diagnostic.code === ExecutorDiagnosticCode.Cancelled
    ? 'cancelled'
    : diagnostic.code === ExecutorDiagnosticCode.StreamGap
      ? 'gap'
      : diagnostic.severity === ExecutorDiagnosticSeverity.Warning
        ? 'warning'
        : 'error';

  problemsByKey.set(key, {
    key,
    message: diagnostic.message,
    diagnostic,
    tone,
  });
}

function persistPlaygroundState(tabs: WorkflowTab[], activeTabId: string, darkMode: boolean) {
  const persistedTabs = tabs.map(persistableWorkflowTab);
  const persistedItems: [string, string][] = [
    [tabsStorageKey, JSON.stringify(persistedTabs)],
    [activeTabStorageKey, activeTabId],
    [themeStorageKey, darkMode ? 'dark' : 'light'],
  ];

  try {
    for (const [storageKey, storageValue] of persistedItems) {
      localStorage.setItem(storageKey, storageValue);
    }
  } catch (error) {
    try {
      localStorage.removeItem(tabsStorageKey);

      for (const [storageKey, storageValue] of persistedItems) {
        localStorage.setItem(storageKey, storageValue);
      }
    } catch (retryError) {
      console.warn('Unable to persist playground state.', retryError, error);
    }
  }
}

function persistableWorkflowTab(tab: WorkflowTab): WorkflowTab {
  const idleRunState = tab.runState === 'running' ? 'failed' : tab.runState;
  const idleMessage = tab.runState === 'running' ? 'Run connection was lost during page reload. Start a new run to continue.' : tab.message;

  return {
    ...tab,
    runState: idleRunState,
    message: idleMessage,
    secretsJson: '{}',
    outputJson: '',
    eventLog: [],
    runtimeDiagnostic: null,
    graphState: 'idle',
    graphMessage: defaultGraphMessage,
    graphData: null,
  };
}

function restoreFromStorage(setTabs: (tabs: WorkflowTab[]) => void, setActiveTabId: (tabId: string) => void, setDarkMode: (darkMode: boolean) => void) {
  setDarkMode(localStorageValue(themeStorageKey) !== 'light');

  const savedTabs = localStorageValue(tabsStorageKey);
  const restoredTabs = savedTabs ? parseStoredWorkflowTabs(savedTabs) : [createWorkflowTab('Launch brief')];
  const tabs = restoredTabs.length > 0 ? restoredTabs : [createWorkflowTab('Launch brief')];
  const savedActiveTabId = localStorageValue(activeTabStorageKey);
  const activeTabId = tabs.some((tab) => tab.id === savedActiveTabId) ? savedActiveTabId! : tabs[0]?.id ?? '';

  setTabs(tabs);
  setActiveTabId(activeTabId);
}

function localStorageValue(storageKey: string) {
  try {
    return localStorage.getItem(storageKey);
  } catch (error) {
    console.warn(`Unable to read ${storageKey} from local storage.`, error);

    return null;
  }
}

function parseStoredWorkflowTabs(savedTabs: string) {
  try {
    const parsedTabs = JSON.parse(savedTabs) as unknown;

    if (Array.isArray(parsedTabs)) {
      return parsedTabs.map((tab) => ({ ...recoverWorkflowTabAfterReload(tab), secretsJson: '{}' }));
    }
  } catch (error) {
    console.warn('Unable to restore saved playground tabs.', error);
  }

  return [createWorkflowTab('Launch brief')];
}

export type UpdateTab = (tabId: string, updater: (tab: WorkflowTab) => WorkflowTab) => void;

interface SseStreamProgress {
  events: ExecutorEvent[];
  lastEventIdentifier: string | null;
  acceptedEventIdentifiers: Set<string>;
  acceptedEventIdentifierOrder: string[];
  terminalEvent: ExecutorEvent | null;
  historyGapDiagnostic: ExecutionDiagnostic | null;
  historyGapConfirmationGranted: boolean;
}

export interface WorkflowEventStreamDependencies {
  reconnect: (runIdentifier: string, replayCursor: string | null, abortSignal: AbortSignal) => Promise<Response>;
  waitForReconnect: (abortSignal: AbortSignal) => Promise<void>;
}

export async function readWorkflowEventStream(
  initialResponse: Response,
  tabId: string,
  abortSignal: AbortSignal,
  acceptChunk: (chunk: string, tabId: string) => ExecutorEvent | null,
  updateTab: UpdateTab,
  confirmStreamGap: (
    diagnostic: ExecutionDiagnostic,
    resumeAfter: string | null,
    historyLoss: boolean,
  ) => Promise<boolean>,
  dependencies: WorkflowEventStreamDependencies = {
    reconnect: reconnectWorkflowEventStream,
    waitForReconnect: waitForReconnectDelay,
  },
) {
  const runIdentifier = initialResponse.headers.get(runIdentifierHeader);
  let response = initialResponse;
  let replayCursorOverride: string | null = null;
  const progress: SseStreamProgress = {
    events: [],
    lastEventIdentifier: null,
    acceptedEventIdentifiers: new Set(),
    acceptedEventIdentifierOrder: [],
    terminalEvent: null,
    historyGapDiagnostic: null,
    historyGapConfirmationGranted: false,
  };

  while (true) {
    try {
      await readSseStream(response.body, tabId, acceptChunk, progress);

      if (progress.terminalEvent) {
        return progress.events;
      }
    } catch (error) {
      if (abortSignal.aborted || error instanceof WorkflowStreamLimitError) {
        throw error;
      }
    }

    if (progress.historyGapDiagnostic && !progress.historyGapConfirmationGranted) {
      const resumeAfter = streamGapRequestedAfter(progress.historyGapDiagnostic) ?? progress.lastEventIdentifier;
      const acceptedReplay = await confirmStreamGap(progress.historyGapDiagnostic, resumeAfter, false);

      if (!acceptedReplay) {
        throw new WorkflowStreamUnavailableError(progress.historyGapDiagnostic);
      }

      replayCursorOverride = resumeAfter;
      progress.historyGapConfirmationGranted = true;
    }

    if (!runIdentifier) {
      throw new Error('Run connection was lost and the server did not provide a reconnect identifier.');
    }

    let reconnected = false;

    while (!reconnected) {
      const replayCursor = replayCursorOverride ?? progress.lastEventIdentifier;
      updateTab(tabId, (tab) => ({
        ...tab,
        message: progress.historyGapDiagnostic
          ? 'Workflow event history has a gap. Reconnecting from the last confirmed event...'
          : 'Run connection was lost. Reconnecting...',
      }));
      await dependencies.waitForReconnect(abortSignal);

      try {
        response = await dependencies.reconnect(runIdentifier, replayCursor, abortSignal);
        replayCursorOverride = null;
        progress.historyGapDiagnostic = null;
        progress.historyGapConfirmationGranted = false;
        reconnected = true;
      } catch (error) {
        if (abortSignal.aborted || error instanceof WorkflowStreamUnavailableError) {
          throw error;
        }

        if (error instanceof WorkflowStreamGapError) {
          progress.historyGapDiagnostic = error.diagnostic;
          progress.historyGapConfirmationGranted = false;
          updateTab(tabId, (tab) => ({
            ...tab,
            message: 'Earlier workflow events have expired. Confirmation is required to resume with incomplete history.',
            runtimeDiagnostic: error.diagnostic,
            eventLog: appendDiagnosticEvent(tab.eventLog, ExecutorEventKind.StreamGap, error.diagnostic),
          }));

          const oldestAvailable = streamGapOldestAvailable(error.diagnostic);

          if (oldestAvailable === null) {
            throw new WorkflowStreamUnavailableError(error.diagnostic);
          }

          const resumeAfter = eventIdentifierBefore(oldestAvailable);
          const acceptedHistoryLoss = await confirmStreamGap(error.diagnostic, resumeAfter, true);

          if (!acceptedHistoryLoss) {
            throw new WorkflowStreamUnavailableError(error.diagnostic);
          }

          replayCursorOverride = resumeAfter;
          progress.historyGapConfirmationGranted = true;
        }
      }
    }
  }
}

export class WorkflowDiagnosticError extends Error {
  constructor(readonly diagnostic: ExecutionDiagnostic) {
    super(diagnostic.message);
  }
}

export class WorkflowStreamUnavailableError extends WorkflowDiagnosticError {}

export class WorkflowStreamGapError extends WorkflowDiagnosticError {}

export class WorkflowStreamLimitError extends WorkflowDiagnosticError {}

async function cancelWorkflowRun(runIdentifier: string): Promise<CancellationResponse> {
  const response = await fetch(`/execute/${encodeURIComponent(runIdentifier)}/cancel`, { method: 'POST' });
  const payload = await responsePayload(response).catch(() => ({}));

  if (!response.ok) {
    throw responseDiagnosticError(payload, `Unable to cancel workflow run (${response.status}).`);
  }

  const cancellationResponse = parseCancellationResponse(payload);

  if (!cancellationResponse) {
    throw new Error('Cancellation response did not include a recognized transition.');
  }

  return cancellationResponse;
}

async function reconnectWorkflowEventStream(runIdentifier: string, replayCursor: string | null, abortSignal: AbortSignal) {
  const query = replayCursor === null ? '' : `?after=${encodeURIComponent(replayCursor)}`;
  const response = await fetch(`/execute/${encodeURIComponent(runIdentifier)}/events${query}`, {
    headers: {
      accept: 'text/event-stream',
    },
    signal: abortSignal,
  });

  if (!response.ok || !response.body) {
    const payload = await responsePayload(response).catch(() => ({}));
    const diagnostic = diagnosticFromErrorPayload(payload);

    if (response.status === 409 && diagnostic?.code === ExecutorDiagnosticCode.StreamGap) {
      throw new WorkflowStreamGapError(diagnostic);
    }

    if (
      (response.status === 404 && diagnostic?.code === ExecutorDiagnosticCode.UnknownRun)
      || (response.status === 410 && diagnostic?.code === ExecutorDiagnosticCode.StreamExpired)
    ) {
      throw new WorkflowStreamUnavailableError(diagnostic);
    }

    throw responseDiagnosticError(payload, `Unable to reconnect workflow stream (${response.status}).`);
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

export class BoundedSseBuffer {
  private readonly decoder = new TextDecoder();
  private buffer = '';
  private totalByteLength = 0;

  append(bytes: Uint8Array, lastEventIdentifier: string | null) {
    this.totalByteLength += bytes.byteLength;

    if (this.totalByteLength > maxSseTotalBytes) {
      throw this.limitError('The workflow event stream exceeded the browser total-size safety limit. Start a new run with fewer events.', lastEventIdentifier);
    }

    this.buffer += this.decoder.decode(bytes, { stream: true });
    this.normalizeLineEndings();
    this.assertWithinLimits(lastEventIdentifier);

    return this.drainCompletedFrames(lastEventIdentifier);
  }

  finish(lastEventIdentifier: string | null) {
    this.buffer += this.decoder.decode();
    this.normalizeLineEndings();
    this.assertWithinLimits(lastEventIdentifier);
    const frames = this.drainCompletedFrames(lastEventIdentifier);

    if (this.buffer.trim()) {
      this.assertDataWithinLimits(this.buffer, lastEventIdentifier);
      frames.push(this.buffer);
    }

    this.buffer = '';

    return frames;
  }

  private normalizeLineEndings() {
    this.buffer = this.buffer.replaceAll('\r\n', '\n');
  }

  private assertWithinLimits(lastEventIdentifier: string | null) {
    if (utf8Encoder.encode(this.buffer).byteLength > maxSseBufferedTextBytes) {
      throw this.limitError('Workflow event buffering exceeded the browser safety limit. Start a new run with smaller event payloads.', lastEventIdentifier);
    }

    for (const frame of this.buffer.split('\n\n')) {
      if (utf8Encoder.encode(frame).byteLength > maxSseFrameBytes) {
        throw this.limitError('A workflow event frame exceeded the browser safety limit. Start a new run with smaller event payloads.', lastEventIdentifier);
      }
    }

    for (const line of this.buffer.split('\n')) {
      if (utf8Encoder.encode(line).byteLength > maxSseLineBytes) {
        throw this.limitError('A workflow event stream line exceeded the browser safety limit. Start a new run with smaller event payloads.', lastEventIdentifier);
      }
    }
  }

  private drainCompletedFrames(lastEventIdentifier: string | null) {
    const frames = this.buffer.split('\n\n');
    this.buffer = frames.pop() ?? '';

    for (const frame of frames) {
      this.assertDataWithinLimits(frame, lastEventIdentifier);
    }

    return frames;
  }

  private assertDataWithinLimits(frame: string, lastEventIdentifier: string | null) {
    const data = parseSseMessage(frame).data;

    if (data && utf8Encoder.encode(data).byteLength > maxSseDataBytes) {
      throw this.limitError('Workflow event data exceeded the browser safety limit. Start a new run with smaller event payloads.', lastEventIdentifier);
    }
  }

  private limitError(message: string, lastEventIdentifier: string | null) {
    return new WorkflowStreamLimitError({
      code: ExecutorDiagnosticCode.InternalError,
      stage: ExecutorStage.Stream,
      severity: ExecutorDiagnosticSeverity.Error,
      retryability: ExecutorDiagnosticRetryability.Never,
      message,
      subject: {
        type: ExecutorDiagnosticSubjectType.Stream,
        requested_after: lastEventIdentifier ?? undefined,
      },
    });
  }
}

async function readSseStream(
  stream: ReadableStream<Uint8Array> | null,
  tabId: string,
  acceptChunk: (chunk: string, tabId: string) => ExecutorEvent | null,
  progress: SseStreamProgress,
) {
  if (!stream) {
    throw new Error('Workflow stream response did not include a body.');
  }

  const reader = stream.getReader();
  const buffer = new BoundedSseBuffer();

  try {
    while (true) {
      const readResult = await reader.read();

      if (readResult.done) {
        break;
      }

      for (let byteOffset = 0; byteOffset < readResult.value.byteLength; byteOffset += maxSseLineBytes) {
        const byteChunk = readResult.value.subarray(byteOffset, byteOffset + maxSseLineBytes);
        const frames = buffer.append(byteChunk, progress.lastEventIdentifier);

        for (const frame of frames) {
          const sseMessage = parseSseMessage(frame);
          acceptSseMessage(frame, sseMessage, tabId, acceptChunk, progress);

          if (progress.historyGapDiagnostic) {
            await reader.cancel();

            return;
          }
        }
      }
    }

    for (const frame of buffer.finish(progress.lastEventIdentifier)) {
      const sseMessage = parseSseMessage(frame);
      acceptSseMessage(frame, sseMessage, tabId, acceptChunk, progress);

      if (progress.historyGapDiagnostic) {
        return;
      }
    }
  } catch (error) {
    await reader.cancel().catch(() => undefined);

    throw error;
  } finally {
    reader.releaseLock();
  }
}

function acceptSseMessage(
  chunk: string,
  sseMessage: SseMessage,
  tabId: string,
  acceptChunk: (chunk: string, tabId: string) => ExecutorEvent | null,
  progress: SseStreamProgress,
) {
  if (!sseMessage.data) {
    return;
  }

  if (sseMessage.eventIdentifier && progress.acceptedEventIdentifiers.has(sseMessage.eventIdentifier)) {
    return;
  }

  const event = acceptChunk(chunk, tabId);

  if (!event) {
    return;
  }

  if (event.kind === ExecutorEventKind.StreamGap) {
    progress.historyGapDiagnostic = event.diagnostic ?? null;
    progress.historyGapConfirmationGranted = false;
  } else if (sseMessage.eventIdentifier) {
    progress.lastEventIdentifier = sseMessage.eventIdentifier;
    progress.acceptedEventIdentifiers.add(sseMessage.eventIdentifier);
    progress.acceptedEventIdentifierOrder.push(sseMessage.eventIdentifier);

    if (progress.acceptedEventIdentifierOrder.length > maxAcceptedEventIdentifiers) {
      const expiredEventIdentifier = progress.acceptedEventIdentifierOrder.shift();

      if (expiredEventIdentifier) {
        progress.acceptedEventIdentifiers.delete(expiredEventIdentifier);
      }
    }
  }

  progress.events = retainEventHistory(progress.events, [event], false);

  if (isTerminalWorkflowEvent(event)) {
    progress.terminalEvent = event;
  }
}

function parseSseChunk(chunk: string): ExecutorEvent | null {
  const sseMessage = parseSseMessage(chunk);

  if (!sseMessage.data) {
    return null;
  }

  try {
    return parseExecutorEvent(parseJsonPreservingStreamCursors(sseMessage.data));
  } catch {
    return null;
  }
}

interface SseMessage {
  eventIdentifier: string | null;
  data: string | null;
}

function parseSseMessage(chunk: string): SseMessage {
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
  return event.kind === ExecutorEventKind.WorkflowCompleted
    || event.kind === ExecutorEventKind.WorkflowFailed
    || event.kind === ExecutorEventKind.WorkflowCancelled;
}

function latestTerminalWorkflowEvent(events: ExecutorEvent[]) {
  for (let eventIndex = events.length - 1; eventIndex >= 0; eventIndex -= 1) {
    const event = events[eventIndex];

    if (isTerminalWorkflowEvent(event)) {
      return event;
    }
  }

  return null;
}

function parseExecutorEvent(value: unknown): ExecutorEvent | null {
  if (!isRecord(value) || !enumMember<ExecutorEventKind>(executorEventKinds, value.kind) || typeof value.timestamp_ms !== 'number') {
    return null;
  }

  const diagnostic = value.diagnostic === undefined ? undefined : parseExecutionDiagnostic(value.diagnostic);

  if (value.diagnostic !== undefined && !diagnostic) {
    return null;
  }

  return {
    kind: value.kind,
    timestamp_ms: value.timestamp_ms,
    agent_name: typeof value.agent_name === 'string' ? value.agent_name : undefined,
    message: typeof value.message === 'string' ? value.message : undefined,
    diagnostic,
    data: value.data,
  } as ExecutorEvent;
}

function parseExecutionDiagnostic(value: unknown): ExecutionDiagnostic | null {
  if (
    !isRecord(value)
    || !enumMember<ExecutorDiagnosticCode>(executorDiagnosticCodes, value.code)
    || !enumMember<ExecutorStage>(executorStages, value.stage)
    || !enumMember<ExecutorDiagnosticSeverity>(executorDiagnosticSeverities, value.severity)
    || !enumMember<ExecutorDiagnosticRetryability>(executorDiagnosticRetryabilities, value.retryability)
    || typeof value.message !== 'string'
  ) {
    return null;
  }

  const subject = parseExecutionDiagnosticSubject(value.subject);
  const cause = value.cause === undefined ? undefined : parseExecutionDiagnostic(value.cause);
  const retryAfterMilliseconds = value.retry_after_ms;

  if (
    !subject
    || (value.cause !== undefined && !cause)
    || (
      retryAfterMilliseconds !== undefined
      && (typeof retryAfterMilliseconds !== 'number' || !Number.isInteger(retryAfterMilliseconds) || retryAfterMilliseconds < 0)
    )
  ) {
    return null;
  }

  return {
    code: value.code,
    stage: value.stage,
    severity: value.severity,
    retryability: value.retryability,
    message: value.message,
    subject,
    retry_after_ms: typeof retryAfterMilliseconds === 'number' ? retryAfterMilliseconds : undefined,
    cause: cause ?? undefined,
  };
}

function parseExecutionDiagnosticSubject(value: unknown): ExecutionDiagnosticSubject | null {
  if (!isRecord(value) || !enumMember<ExecutorDiagnosticSubjectType>(executorDiagnosticSubjectTypes, value.type)) {
    return null;
  }

  if (value.type === ExecutorDiagnosticSubjectType.Workflow) {
    return { type: value.type };
  }

  if (value.type === ExecutorDiagnosticSubjectType.Agent) {
    if (typeof value.agent_name !== 'string' || !optionalNonNegativeInteger(value.iteration_index)) {
      return null;
    }

    return {
      type: value.type,
      agent_name: value.agent_name,
      iteration_index: typeof value.iteration_index === 'number' ? value.iteration_index : undefined,
    };
  }

  if (value.type === ExecutorDiagnosticSubjectType.Provider) {
    if (
      typeof value.agent_name !== 'string'
      || !optionalString(value.provider_name)
      || !optionalString(value.model_name)
      || !optionalPositiveInteger(value.attempt)
      || !optionalInteger(value.http_status)
    ) {
      return null;
    }

    return {
      type: value.type,
      agent_name: value.agent_name,
      provider_name: typeof value.provider_name === 'string' ? value.provider_name : undefined,
      model_name: typeof value.model_name === 'string' ? value.model_name : undefined,
      attempt: typeof value.attempt === 'number' ? value.attempt : undefined,
      http_status: typeof value.http_status === 'number' ? value.http_status : undefined,
    };
  }

  if (value.type === ExecutorDiagnosticSubjectType.Tool) {
    if (typeof value.tool_name !== 'string' || !optionalString(value.agent_name)) {
      return null;
    }

    return {
      type: value.type,
      agent_name: typeof value.agent_name === 'string' ? value.agent_name : undefined,
      tool_name: value.tool_name,
    };
  }

  if (value.type === ExecutorDiagnosticSubjectType.Mcp) {
    if (!optionalString(value.agent_name) || !optionalString(value.server_name) || !optionalString(value.target_name)) {
      return null;
    }

    return {
      type: value.type,
      agent_name: typeof value.agent_name === 'string' ? value.agent_name : undefined,
      server_name: typeof value.server_name === 'string' ? value.server_name : undefined,
      target_name: typeof value.target_name === 'string' ? value.target_name : undefined,
    };
  }

  if (value.type === ExecutorDiagnosticSubjectType.Cache) {
    if (!enumMember<ExecutorCacheOperation>(executorCacheOperations, value.operation)) {
      return null;
    }

    return { type: value.type, operation: value.operation };
  }

  const requestedAfter = parseEventIdentifier(value.requested_after);
  const oldestAvailable = parseEventIdentifier(value.oldest_available);

  if (requestedAfter === null || oldestAvailable === null) {
    return null;
  }

  return {
    type: value.type,
    requested_after: requestedAfter,
    oldest_available: oldestAvailable,
  };
}

function parseEventIdentifier(value: unknown): string | undefined | null {
  if (value === undefined) {
    return undefined;
  }

  if (typeof value === 'string' && /^\d+$/.test(value)) {
    return BigInt(value).toString();
  }

  if (typeof value === 'number' && Number.isSafeInteger(value) && value >= 0) {
    return BigInt(value).toString();
  }

  return null;
}

function parseCancellationResponse(payload: Record<string, unknown>): CancellationResponse | null {
  if (!enumMember<CancellationTransition>(cancellationTransitions, payload.transition)) {
    return null;
  }

  return { transition: payload.transition };
}

function responseDiagnosticError(payload: Record<string, unknown>, fallbackMessage: string) {
  const diagnostic = diagnosticFromErrorPayload(payload);

  if (diagnostic) {
    return new WorkflowDiagnosticError(diagnostic);
  }

  return new Error(stringPayloadValue(payload.error) ?? fallbackMessage);
}

function diagnosticFromErrorPayload(payload: Record<string, unknown>) {
  return parseExecutionDiagnostic(payload.error);
}

function appendDiagnosticEvent(events: ExecutorEvent[], kind: ExecutorEventKind.StreamGap, diagnostic: ExecutionDiagnostic) {
  const duplicateEvent = events.some((event) => (
    event.kind === kind
    && event.diagnostic?.code === diagnostic.code
    && event.diagnostic.message === diagnostic.message
  ));

  if (duplicateEvent) {
    return events;
  }

  return retainEventHistory(events, [{
    kind,
    timestamp_ms: Date.now(),
    message: diagnostic.message,
    diagnostic,
    data: {},
  }], true);
}

function retainEventHistory(existingEvents: ExecutorEvent[], incomingEvents: ExecutorEvent[], includeTruncationNotice: boolean) {
  const retainedEvents = [...existingEvents, ...incomingEvents];
  const protectedEvents = new Set<ExecutorEvent>();
  const latestTerminalEvent = latestTerminalWorkflowEvent(retainedEvents);
  let latestStreamGapEvent: ExecutorEvent | null = null;
  let latestTruncationEvent: ExecutorEvent | null = null;

  for (let eventIndex = retainedEvents.length - 1; eventIndex >= 0; eventIndex -= 1) {
    const event = retainedEvents[eventIndex];

    if (!event || event.kind !== ExecutorEventKind.StreamGap) {
      continue;
    }

    if (!latestTruncationEvent && event.message === eventHistoryTruncationMessage) {
      latestTruncationEvent = event;
    } else if (!latestStreamGapEvent && event.message !== eventHistoryTruncationMessage) {
      latestStreamGapEvent = event;
    }

    if (latestStreamGapEvent && latestTruncationEvent) {
      break;
    }
  }

  if (latestTerminalEvent) {
    protectedEvents.add(latestTerminalEvent);
  }

  if (latestStreamGapEvent) {
    protectedEvents.add(latestStreamGapEvent);
  }

  if (latestTruncationEvent) {
    protectedEvents.add(latestTruncationEvent);
  }

  if (protectedEvents.size === 0 && retainedEvents.length > 0) {
    protectedEvents.add(retainedEvents.at(-1) as ExecutorEvent);
  }

  const eventByteLengths = retainedEvents.map(serializedEventByteLength);
  let retainedByteLength = eventByteLengths.reduce((totalBytes, eventBytes) => totalBytes + eventBytes, 0);
  let historyTruncated = false;

  while (retainedEvents.length > maxRetainedUiEvents || retainedByteLength > maxRetainedUiEventBytes) {
    const removalIndex = retainedEvents.findIndex((event) => !protectedEvents.has(event));

    if (removalIndex < 0) {
      break;
    }

    retainedByteLength -= eventByteLengths[removalIndex] ?? 0;
    retainedEvents.splice(removalIndex, 1);
    eventByteLengths.splice(removalIndex, 1);
    historyTruncated = true;
  }

  if (
    historyTruncated
    && includeTruncationNotice
    && !retainedEvents.some((event) => event.message === eventHistoryTruncationMessage)
  ) {
    const diagnostic: ExecutionDiagnostic = {
      code: ExecutorDiagnosticCode.StreamGap,
      stage: ExecutorStage.Stream,
      severity: ExecutorDiagnosticSeverity.Warning,
      retryability: ExecutorDiagnosticRetryability.Safe,
      message: eventHistoryTruncationMessage,
      subject: { type: ExecutorDiagnosticSubjectType.Stream },
    };

    return retainEventHistory(retainedEvents, [{
      kind: ExecutorEventKind.StreamGap,
      timestamp_ms: Date.now(),
      message: diagnostic.message,
      diagnostic,
      data: {},
    }], false);
  }

  return retainedEvents;
}

function serializedEventByteLength(event: ExecutorEvent) {
  const cachedByteLength = serializedExecutorEventByteLengths.get(event);

  if (cachedByteLength !== undefined) {
    return cachedByteLength;
  }

  try {
    const byteLength = utf8Encoder.encode(JSON.stringify(event)).byteLength;
    serializedExecutorEventByteLengths.set(event, byteLength);

    return byteLength;
  } catch {
    return maxRetainedUiEventBytes;
  }
}

function streamGapRequestedAfter(diagnostic: ExecutionDiagnostic | null) {
  if (diagnostic?.subject.type !== ExecutorDiagnosticSubjectType.Stream) {
    return null;
  }

  return diagnostic.subject.requested_after ?? null;
}

function streamGapOldestAvailable(diagnostic: ExecutionDiagnostic) {
  if (diagnostic.subject.type !== ExecutorDiagnosticSubjectType.Stream) {
    return null;
  }

  return diagnostic.subject.oldest_available ?? null;
}

function eventIdentifierBefore(eventIdentifier: string | null) {
  if (!eventIdentifier || !/^\d+$/.test(eventIdentifier)) {
    return null;
  }

  const numericEventIdentifier = BigInt(eventIdentifier);

  return numericEventIdentifier > 0n ? (numericEventIdentifier - 1n).toString() : null;
}

function unknownRunCancellationDiagnostic(): ExecutionDiagnostic {
  return {
    code: ExecutorDiagnosticCode.UnknownRun,
    stage: ExecutorStage.Cancellation,
    severity: ExecutorDiagnosticSeverity.Error,
    retryability: ExecutorDiagnosticRetryability.Never,
    message: 'The server no longer recognizes this workflow run.',
    subject: { type: ExecutorDiagnosticSubjectType.Workflow },
  };
}

function enumMember<EnumValue extends string>(values: Set<string>, value: unknown): value is EnumValue {
  return typeof value === 'string' && values.has(value);
}

function optionalString(value: unknown) {
  return value === undefined || typeof value === 'string';
}

function optionalInteger(value: unknown) {
  return value === undefined || (typeof value === 'number' && Number.isInteger(value));
}

function optionalNonNegativeInteger(value: unknown) {
  return value === undefined || (typeof value === 'number' && Number.isInteger(value) && value >= 0);
}

function optionalPositiveInteger(value: unknown) {
  return value === undefined || (typeof value === 'number' && Number.isInteger(value) && value > 0);
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
    const payload = parseJsonPreservingStreamCursors(responseText);

    if (isRecord(payload)) {
      return payload;
    }

    return { error: responseText };
  } catch {
    return { error: responseText };
  }
}

function parseJsonPreservingStreamCursors(source: string): unknown {
  let normalizedSource = '';
  let characterIndex = 0;

  while (characterIndex < source.length) {
    if (source[characterIndex] !== '"') {
      normalizedSource += source[characterIndex];
      characterIndex += 1;

      continue;
    }

    const stringStartIndex = characterIndex;
    characterIndex += 1;

    while (characterIndex < source.length) {
      if (source[characterIndex] === '\\') {
        characterIndex += 2;

        continue;
      }

      if (source[characterIndex] === '"') {
        characterIndex += 1;

        break;
      }

      characterIndex += 1;
    }

    const stringToken = source.slice(stringStartIndex, characterIndex);
    normalizedSource += stringToken;
    let delimiterIndex = characterIndex;

    while (/\s/.test(source[delimiterIndex] ?? '')) {
      delimiterIndex += 1;
    }

    if (source[delimiterIndex] !== ':') {
      continue;
    }

    let propertyName: unknown;

    try {
      propertyName = JSON.parse(stringToken) as unknown;
    } catch {
      continue;
    }

    if (propertyName !== 'requested_after' && propertyName !== 'oldest_available') {
      continue;
    }

    let valueIndex = delimiterIndex + 1;

    while (/\s/.test(source[valueIndex] ?? '')) {
      valueIndex += 1;
    }

    if (!/\d/.test(source[valueIndex] ?? '')) {
      continue;
    }

    let valueEndIndex = valueIndex;

    while (/\d/.test(source[valueEndIndex] ?? '')) {
      valueEndIndex += 1;
    }

    if (!/[\s,}\]]/.test(source[valueEndIndex] ?? '')) {
      continue;
    }

    normalizedSource += `${source.slice(characterIndex, valueIndex)}"${source.slice(valueIndex, valueEndIndex)}"`;
    characterIndex = valueEndIndex;
  }

  return JSON.parse(normalizedSource) as unknown;
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
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function ActionTooltip({ children, label }: { children: ReactElement; label: string }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>{children}</TooltipTrigger>
      <TooltipContent side="bottom" sideOffset={8}>{label}</TooltipContent>
    </Tooltip>
  );
}
