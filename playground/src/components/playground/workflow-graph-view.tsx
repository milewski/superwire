import '@xyflow/react/dist/style.css';
import { Background, Controls, Handle, MiniMap, Position, ReactFlow, ReactFlowProvider, useEdgesState, useNodesState, useReactFlow, useUpdateNodeInternals, type Edge, type Node, type NodeProps, type Viewport } from '@xyflow/react';
import { Box, CheckCircle2, ChevronDown, CircleDashed, Cloud, Cpu, DatabaseZap, Eye, GitBranch, Layers3, Loader2, PlugZap, RefreshCcw, Settings2, Sparkles } from 'lucide-react';
import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { Button } from '@/components/ui/button';
import JsonCodeEditor from '@/components/json-code-editor';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import type { ExecutorEvent, GraphState, RunState, WorkflowExecutionGraph, WorkflowExecutionGraphNode, WorkflowExecutionGraphTool } from '@/types';

interface WorkflowGraphViewProps {
  graph: WorkflowExecutionGraph | null;
  source: string;
  graphState: GraphState;
  runState: RunState;
  events: ExecutorEvent[];
  outputJson: string;
  message: string;
  onRefresh: () => void;
}

interface WorkflowGraphNodeData extends Record<string, unknown> {
  node: WorkflowExecutionGraphNode;
  config: GraphConfig;
  runState: RunState;
  activeRunCount: number;
  outputEntries: GraphOutputEntry[];
  failureEntry: GraphFailureEntry | null;
}

interface GraphOutputEntry {
  title: string;
  outputJson: string;
}

interface GraphFailureEntry {
  title: string;
  message: string;
}

type WorkflowGraphReactNode = Node<WorkflowGraphNodeData, 'workflowGraph'>;
type GraphDensity = 'compact' | 'comfortable';
type GraphEdgeType = 'smoothstep' | 'straight' | 'default' | 'simplebezier';
type GraphNodeStatus = 'idle' | 'running' | 'completed' | 'failed';
type GraphExecutionSlotStatus = 'completed' | 'running' | 'failed' | 'waiting' | 'idle';

interface GraphNodePosition {
  x: number;
  y: number;
}

interface GraphConfig {
  density: GraphDensity;
  collapseAll: boolean;
  edgeType: GraphEdgeType;
  showEdgeLabels: boolean;
}

const graphConfigStorageKey = 'superwire.playground.graphConfig.v1';
const graphViewportStorageKey = 'superwire.playground.graphViewport.v1';
const graphNodePositionsStorageKey = 'superwire.playground.graphNodePositions.v2';
const defaultGraphConfig: GraphConfig = { density: 'comfortable', collapseAll: false, edgeType: 'smoothstep', showEdgeLabels: true };
const graphLayoutColumnGap = 180;
const graphLayoutRowGap = 80;
const graphLayoutDefaultNodeWidth = 340;
const graphLayoutDefaultNodeHeight = 260;
const defaultGraphViewport: Viewport = { x: 0, y: 0, zoom: 0.85 };
const WorkflowGraphOpenObjectSchema = { type: 'object', additionalProperties: true };

const graphNodeTypes = {
  workflowGraph: WorkflowGraphNodeCard,
};

export default function WorkflowGraphView({ graph, source, graphState, runState, events, outputJson, message, onRefresh }: WorkflowGraphViewProps) {
  const [config, setConfig] = useState<GraphConfig>(() => restoreGraphConfig());
  const [layoutRequestCount, setLayoutRequestCount] = useState(0);
  const workflowDeclarations = useMemo(() => parseWorkflowGraphDeclarations(source), [source]);
  const activeRunCounts = useMemo(() => (runState === 'running' ? activeAgentRunCounts(events) : new Map<string, number>()), [runState, events]);
  const outputEntriesByNodeId = useMemo(() => graphOutputEntriesByNodeId(events, outputJson), [events, outputJson]);
  const failureEntriesByNodeId = useMemo(() => graphFailureEntriesByNodeId(events), [events]);
  const activeAgentSignature = Array.from(activeRunCounts.entries()).sort().map(([agentName, activeRunCount]) => `${agentName}:${activeRunCount}`).join(':');
  const displayGraph = useMemo(() => (graph ? graphWithProviderModelDeclarations(graph, workflowDeclarations) : null), [graph, workflowDeclarations]);
  const nodes = useMemo(() => (displayGraph ? reactFlowNodes(displayGraph, config, runState, activeRunCounts, outputEntriesByNodeId, failureEntriesByNodeId) : []), [displayGraph, config, runState, activeAgentSignature, outputEntriesByNodeId, failureEntriesByNodeId]);
  const edges = useMemo(() => (displayGraph ? reactFlowEdges(displayGraph, config, activeRunCounts, outputEntriesByNodeId, failureEntriesByNodeId) : []), [displayGraph, config, activeAgentSignature, outputEntriesByNodeId, failureEntriesByNodeId]);
  const graphSignature = displayGraph ? displayGraph.nodes.map((node) => node.id).join(':') : 'empty';

  useEffect(() => {
    localStorage.setItem(graphConfigStorageKey, JSON.stringify(config));
  }, [config]);

  return (
    <section className="graph-view">
      <div className="graph-view__canvas" data-empty={graph ? 'false' : 'true'}>
        <div className="graph-view__toolbar">
          <GraphStateBadge graphState={graphState} />
          <button type="button" className="graph-view__toolbar-button" onClick={() => setLayoutRequestCount((currentCount) => currentCount + 1)} disabled={!graph || graphState === 'loading'}>
            <GitBranch /> Arrange
          </button>
          <button type="button" className="graph-view__toolbar-button" onClick={onRefresh} disabled={graphState === 'loading'}>
            <RefreshCcw className={graphState === 'loading' ? 'animate-spin' : ''} /> Refresh
          </button>
          <GraphSettingsMenu config={config} graphState={graphState} onChange={setConfig} onRefresh={onRefresh} />
        </div>
        {graph ? (
          <div className="graph-view__flow">
            <ReactFlowProvider>
              <GraphCanvas nodes={nodes} edges={edges} graphSignature={graphSignature} layoutRequestCount={layoutRequestCount} runState={runState} />
            </ReactFlowProvider>
          </div>
        ) : (
          <div className="graph-view__empty">
            <GitBranch />
            <strong>{graphState === 'failed' ? 'Unable to build graph' : 'Graph not generated yet'}</strong>
            <p>{message}</p>
            <Button variant="secondary" size="lg" className="graph-view__button" onClick={onRefresh} disabled={graphState === 'loading'}>
              <RefreshCcw className={graphState === 'loading' ? 'animate-spin' : ''} /> Generate graph
            </Button>
          </div>
        )}
      </div>

      <p className={`graph-view__message graph-view__message--${graphState}`}>{message}</p>
    </section>
  );
}

function GraphStateBadge({ graphState }: { graphState: GraphState }) {
  return (
    <span className={`graph-view__state graph-view__state--${graphState}`}>
      {graphState === 'loading' ? <Loader2 /> : graphState === 'ready' ? <CheckCircle2 /> : <CircleDashed />}
      {graphState}
    </span>
  );
}

function GraphCanvas({ nodes: incomingNodes, edges: incomingEdges, graphSignature, layoutRequestCount, runState }: { nodes: WorkflowGraphReactNode[]; edges: Edge[]; graphSignature: string; layoutRequestCount: number; runState: RunState }) {
  const restoredViewportRef = useRef<Viewport | null>(restoreGraphViewport());
  const initialFitViewCompleteRef = useRef(false);
  const initialNodesRef = useRef<WorkflowGraphReactNode[] | null>(null);
  const currentViewportRef = useRef<Viewport>(restoredViewportRef.current ?? defaultGraphViewport);

  if (initialNodesRef.current === null) {
    initialNodesRef.current = restoreOrLayoutGraphNodePositions(incomingNodes, incomingEdges);
  }

  const [nodes, setNodes, onNodesChange] = useNodesState(initialNodesRef.current);
  const [edges, setEdges, onEdgesChange] = useEdgesState(incomingEdges);
  const [viewport, setViewport] = useState<Viewport>(currentViewportRef.current);
  const reactFlowInstance = useReactFlow();

  useEffect(() => {
    setNodes((currentNodes) => {
      const currentPositions = new Map(currentNodes.map((node) => [node.id, node.position]));
      const restoredPositions = restoreGraphNodePositionMap();
      const nextNodes = incomingNodes.map((incomingNode) => ({
        ...incomingNode,
        position: currentPositions.get(incomingNode.id) ?? restoredPositions[incomingNode.id] ?? incomingNode.position,
      }));

      if (nextNodes.some((node) => currentPositions.has(node.id) || restoredPositions[node.id])) {
        return nextNodes;
      }

      return layoutWorkflowGraphNodes(nextNodes, incomingEdges);
    });
  }, [graphSignature, setNodes]);

  useEffect(() => {
    // Runtime events can arrive many times during loop agents. They may change
    // labels, badges, and outputs, but must never fit, pan, or zoom the canvas.
    setNodes((currentNodes) => mergeRuntimeNodeUpdates(currentNodes, incomingNodes));
  }, [incomingNodes, setNodes]);

  useEffect(() => {
    // Keep edge status updates data-only as well; viewport control stays with
    // the user and the explicit Arrange action above.
    setEdges((currentEdges) => mergeRuntimeEdgeUpdates(currentEdges, incomingEdges));
  }, [incomingEdges, setEdges]);

  useEffect(() => {
    if (runState !== 'running') {
      return;
    }

    window.requestAnimationFrame(() => {
      const preservedViewport = currentViewportRef.current;
      const actualViewport = reactFlowInstance.getViewport();

      if (sameGraphViewport(actualViewport, preservedViewport)) {
        return;
      }

      setViewport(preservedViewport);
      void reactFlowInstance.setViewport(preservedViewport, { duration: 0 });
    });
  }, [incomingNodes, incomingEdges, reactFlowInstance, runState]);

  useEffect(() => {
    storeGraphNodePositions(nodes);
  }, [nodes]);

  useEffect(() => {
    if (layoutRequestCount === 0) {
      return;
    }

    setNodes((currentNodes) => layoutWorkflowGraphNodes(currentNodes, edges));

    window.requestAnimationFrame(() => {
      void reactFlowInstance.fitView({ padding: 0.16, duration: 420 }).then(() => {
        const nextViewport = reactFlowInstance.getViewport();

        currentViewportRef.current = nextViewport;
        setViewport(nextViewport);
        storeGraphViewport(nextViewport);
      });
    });
  }, [layoutRequestCount, edges, reactFlowInstance, setNodes, setViewport]);

  function handleViewportChange(nextViewport: Viewport) {
    currentViewportRef.current = nextViewport;
    setViewport(nextViewport);
    storeGraphViewport(nextViewport);
  }

  return (
    <ReactFlow
      nodes={nodes}
      edges={edges}
      onNodesChange={onNodesChange}
      onEdgesChange={onEdgesChange}
      nodeTypes={graphNodeTypes}
      viewport={viewport}
      onViewportChange={handleViewportChange}
      onInit={(initializedReactFlowInstance) => {
        if (restoredViewportRef.current) {
          const restoredViewport = restoredViewportRef.current;

          currentViewportRef.current = restoredViewport;
          setViewport(restoredViewport);
          void initializedReactFlowInstance.setViewport(restoredViewport, { duration: 0 });

          return;
        }

        if (initialFitViewCompleteRef.current) {
          return;
        }

        initialFitViewCompleteRef.current = true;

        window.requestAnimationFrame(() => {
          void initializedReactFlowInstance.fitView({ padding: 0.16, duration: 0 }).then(() => {
            const nextViewport = initializedReactFlowInstance.getViewport();

            currentViewportRef.current = nextViewport;
            setViewport(nextViewport);
            storeGraphViewport(nextViewport);
          });
        });
      }}
      minZoom={0.35}
      maxZoom={1.2}
      nodesConnectable={false}
      edgesReconnectable={false}
      onMoveEnd={(_event, viewport: Viewport) => {
        currentViewportRef.current = viewport;
        storeGraphViewport(viewport);
      }}
    >
      <Background color="var(--graph-grid-dot)" gap={18} size={1.1} />
      <MiniMap pannable zoomable nodeColor={nodeColor} />
      <Controls showInteractive={false} />
    </ReactFlow>
  );
}

function GraphSettingsMenu({ config, graphState, onChange, onRefresh }: { config: GraphConfig; graphState: GraphState; onChange: (config: GraphConfig) => void; onRefresh: () => void }) {
  return (
    <details className="graph-settings">
      <summary className="graph-settings__trigger">
        <Settings2 /> Settings
      </summary>

      <div className="graph-settings__menu">
        <div className="graph-settings__header">
          <div>
            <strong>Graph settings</strong>
            <small>Layout and rendering preferences.</small>
          </div>
          <button type="button" className="graph-settings__icon-button" onClick={onRefresh} disabled={graphState === 'loading'} aria-label="Refresh graph">
            <RefreshCcw className={graphState === 'loading' ? 'animate-spin' : ''} />
          </button>
        </div>

        <section className="graph-settings__section">
          <span>Density</span>
          <div className="graph-settings__segmented">
            <button type="button" data-active={config.density === 'compact'} onClick={() => onChange({ ...config, density: 'compact' })}>Compact</button>
            <button type="button" data-active={config.density === 'comfortable'} onClick={() => onChange({ ...config, density: 'comfortable' })}>Comfortable</button>
          </div>
        </section>

        <label className="graph-settings__toggle">
          <input type="checkbox" checked={config.collapseAll} onChange={(event) => onChange({ ...config, collapseAll: event.target.checked })} />
          <span className="graph-settings__switch" aria-hidden="true" />
          <span>
            <strong>Collapse all nodes</strong>
            <small>Show summary cards until disabled.</small>
          </span>
        </label>

        <label className="graph-settings__toggle">
          <input type="checkbox" checked={config.showEdgeLabels} onChange={(event) => onChange({ ...config, showEdgeLabels: event.target.checked })} />
          <span className="graph-settings__switch" aria-hidden="true" />
          <span>
            <strong>Show edge labels</strong>
            <small>Display target node names on relationships.</small>
          </span>
        </label>

        <section className="graph-settings__section">
          <span>Edge lines</span>
          <div className="graph-settings__edge-grid">
            <button type="button" data-active={config.edgeType === 'smoothstep'} onClick={() => onChange({ ...config, edgeType: 'smoothstep' })}>Smooth step</button>
            <button type="button" data-active={config.edgeType === 'straight'} onClick={() => onChange({ ...config, edgeType: 'straight' })}>Straight</button>
            <button type="button" data-active={config.edgeType === 'default'} onClick={() => onChange({ ...config, edgeType: 'default' })}>Bezier</button>
            <button type="button" data-active={config.edgeType === 'simplebezier'} onClick={() => onChange({ ...config, edgeType: 'simplebezier' })}>Curve</button>
          </div>
        </section>
      </div>
    </details>
  );
}

function WorkflowGraphNodeCard({ data }: NodeProps<WorkflowGraphReactNode>) {
  const node = data.node;
  const config = data.config;
  const runState = data.runState;
  const activeRunCount = data.activeRunCount;
  const outputEntries = data.outputEntries;
  const failureEntry = data.failureEntry;
  const [collapsed, setCollapsed] = useState(false);
  const [outputOpen, setOutputOpen] = useState(false);
  const [instructionOpen, setInstructionOpen] = useState(false);
  const [openOutputIndex, setOpenOutputIndex] = useState(0);
  const visiblyCollapsed = config.collapseAll || collapsed;
  const status = nodeStatus(node, activeRunCount, outputEntries, failureEntry);
  const visibleBindings = node.bindings.filter((binding) => binding.name !== 'instruction' && binding.name !== 'model');
  const localTools = node.tools.filter((tool) => tool.kind === 'local_tool');
  const mcpTools = node.tools.filter(isMcpTool);
  const inputsCollapsible = node.kind !== 'input';
  const outputsCollapsible = node.kind === 'agent' || node.kind === 'output';
  const updateNodeInternals = useUpdateNodeInternals();

  useEffect(() => {
    updateNodeInternals(node.id);
  }, [node.id, updateNodeInternals, visiblyCollapsed]);

  function openOutput(outputIndex: number) {
    setOpenOutputIndex(outputIndex);
    setOutputOpen(true);
  }

  return (
    <article className={`graph-node graph-node--${node.kind}`} data-collapsed={visiblyCollapsed ? 'true' : 'false'} data-density={config.density} data-status={status} data-running={activeRunCount > 0 ? 'true' : 'false'}>
      <svg className="graph-node__running-stroke" viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden="true">
        <rect x="1.5" y="1.5" width="97" height="97" rx="5" pathLength="100" />
      </svg>
      <GraphNodeHandles node={node} collapsed={visiblyCollapsed} showExpandedInstructionHandle={!node.instruction} />
      <button type="button" className="graph-node__header nodrag" aria-expanded={!visiblyCollapsed} onClick={() => setCollapsed((open) => !open)} disabled={config.collapseAll}>
        <div className="graph-node__identity">
          <span className="graph-node__icon">{nodeIcon(node)}</span>
          <span className="graph-node__title-block">
            <strong className="graph-node__title">{node.label}</strong>
            <small className="graph-node__subtitle">{nodeSubtitle(node)}</small>
          </span>
        </div>
        <NodeStatusBadge status={status} activeRunCount={activeRunCount} outputEntries={outputEntries} />
        <ChevronDown className="graph-node__header-chevron" />
      </button>

      <GraphExecutionStrip node={node} runState={runState} activeRunCount={activeRunCount} outputEntries={outputEntries} failureEntry={failureEntry} onOpenOutput={openOutput} />
      <GraphFailureNotice failureEntry={failureEntry} />
      {node.instruction ? <GraphInstructionPreview instruction={node.instruction} onOpen={() => setInstructionOpen(true)} /> : null}
      {node.loop_info ? <GraphLoopSummary node={node} config={config} /> : null}
      {visiblyCollapsed ? (
        <p className="graph-node__summary">{nodeSummary(node)}</p>
      ) : (
        <>
          {node.kind !== 'agent' && node.details.length > 0 ? <GraphDetails title={node.kind === 'mcp' ? 'MCP bindings' : 'Details'} details={node.details} collapsible={node.kind === 'mcp'} /> : null}
          {visibleBindings.length > 0 ? <GraphBindings bindings={visibleBindings} /> : null}
          <GraphPorts title="Inputs" ports={node.inputs} fallback={node.kind === 'input' ? 'External runtime values' : 'No upstream agent output'} config={config} collapsible={inputsCollapsible} targetHandleId={inputPortTargetHandleId(node)} />
          <GraphPorts title="Outputs" ports={node.outputs} config={config} collapsible={outputsCollapsible} defaultOpen={node.kind !== 'agent'} showPortNames={node.kind !== 'agent' && node.kind !== 'output'} sourceHandleId={outputPortSourceHandleId(node)} />
          {outputEntries.length > 0 ? <GraphOutputAction node={node} outputEntries={outputEntries} onOpen={() => openOutput(0)} /> : null}
          {node.kind === 'mcp' && node.tools.length > 0 ? <GraphMcpDefinitions tools={node.tools} config={config} /> : null}
          {node.kind === 'agent' && mcpTools.length > 0 ? <GraphMcpAccess tools={mcpTools} config={config} /> : null}
          {localTools.length > 0 ? <GraphTools title="Local tools" tools={localTools} config={config} /> : null}
        </>
      )}
      {outputEntries.length > 0 ? <GraphOutputDialog node={node} outputEntries={outputEntries} open={outputOpen} openOutputIndex={openOutputIndex} onOpenChange={setOutputOpen} /> : null}
      {node.instruction ? <GraphInstructionDialog node={node} open={instructionOpen} onOpenChange={setInstructionOpen} /> : null}
    </article>
  );
}

function GraphNodeHandles({ node, collapsed, showExpandedInstructionHandle }: { node: WorkflowExecutionGraphNode; collapsed: boolean; showExpandedInstructionHandle: boolean }) {
  if (!collapsed) {
    return node.kind === 'agent' && showExpandedInstructionHandle ? <Handle id="instruction" type="target" position={Position.Left} className="graph-node__handle graph-node__handle--instruction" isConnectable={false} /> : null;
  }

  const hasCollapsedTargetHandle = node.kind !== 'input';
  const hasCollapsedSourceHandle = node.kind !== 'output';

  return (
    <>
      {hasCollapsedTargetHandle ? <span className="graph-node__collapsed-handle graph-node__collapsed-handle--left" aria-hidden="true" /> : null}
      {hasCollapsedSourceHandle ? <span className="graph-node__collapsed-handle graph-node__collapsed-handle--right" aria-hidden="true" /> : null}
      {node.kind === 'model' ? <Handle id="client" type="target" position={Position.Left} className="graph-node__handle graph-node__handle--collapsed graph-node__handle--client" isConnectable={false} /> : null}
      {node.kind === 'agent' ? <Handle id="instruction" type="target" position={Position.Left} className="graph-node__handle graph-node__handle--collapsed graph-node__handle--instruction" isConnectable={false} /> : null}
      {node.kind === 'agent' ? <Handle id="mcp-access" type="target" position={Position.Left} className="graph-node__handle graph-node__handle--collapsed graph-node__handle--mcp-access" isConnectable={false} /> : null}
      {node.kind !== 'input' ? <Handle id="inputs" type="target" position={Position.Left} className="graph-node__handle graph-node__handle--collapsed graph-node__handle--inputs" isConnectable={false} /> : null}
      {node.kind === 'provider' ? <Handle id="client" type="source" position={Position.Right} className="graph-node__handle graph-node__handle--collapsed graph-node__handle--client" isConnectable={false} /> : null}
      {node.kind === 'model' ? <Handle id="model" type="source" position={Position.Right} className="graph-node__handle graph-node__handle--collapsed graph-node__handle--model" isConnectable={false} /> : null}
      {node.kind === 'mcp' ? <Handle id="mcp-items" type="source" position={Position.Right} className="graph-node__handle graph-node__handle--collapsed graph-node__handle--mcp-items" isConnectable={false} /> : null}
      {node.kind !== 'model' && node.kind !== 'mcp' && node.kind !== 'output' ? <Handle id="output" type="source" position={Position.Right} className="graph-node__handle graph-node__handle--collapsed graph-node__handle--output" isConnectable={false} /> : null}
    </>
  );
}

function NodeStatusBadge({ status, activeRunCount, outputEntries }: { status: GraphNodeStatus; activeRunCount: number; outputEntries: GraphOutputEntry[] }) {
  const label = status === 'completed' ? 'done' : status;
  const detail = status === 'running' && activeRunCount > 1 ? activeRunCount : status === 'completed' && outputEntries.length > 1 ? outputEntries.length : null;

  return (
    <span className={`graph-node__status graph-node__status--${status}`} aria-label={status}>
      <span className="graph-node__status-dot" />
      <span>{label}</span>
      {detail ? <small>{detail}</small> : null}
    </span>
  );
}

function GraphExecutionStrip({ node, runState, activeRunCount, outputEntries, failureEntry, onOpenOutput }: { node: WorkflowExecutionGraphNode; runState: RunState; activeRunCount: number; outputEntries: GraphOutputEntry[]; failureEntry: GraphFailureEntry | null; onOpenOutput: (outputIndex: number) => void }) {
  const executionStripElementRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const executionStripElement = executionStripElementRef.current;

    if (!executionStripElement) {
      return;
    }

    executionStripElement.scrollTo({ left: executionStripElement.scrollWidth, behavior: 'smooth' });
  }, [activeRunCount, outputEntries.length, failureEntry]);

  if (node.kind !== 'agent') {
    return null;
  }

  const completedCount = outputEntries.length;
  const visibleSlotCount = graphExecutionSlotCount(node, completedCount, activeRunCount, failureEntry !== null);

  return (
    <div ref={executionStripElementRef} className="graph-node__execution-strip" aria-label="Execution progress">
      {Array.from({ length: visibleSlotCount }).map((_, slotIndex) => {
        const slotStatus = executionSlotStatus(slotIndex, completedCount, activeRunCount, failureEntry !== null, runState);

        return (
          <button
            key={`${node.id}-slot-${slotIndex}`}
            type="button"
            className="nodrag"
            data-status={slotStatus}
            disabled={slotStatus !== 'completed'}
            onClick={() => onOpenOutput(slotIndex)}
            aria-label={slotStatus === 'completed' ? `Open ${node.label} output ${slotIndex + 1}` : `${node.label} ${slotStatus}`}
          />
        );
      })}
    </div>
  );
}

function GraphFailureNotice({ failureEntry }: { failureEntry: GraphFailureEntry | null }) {
  if (!failureEntry) {
    return null;
  }

  return (
    <section className="graph-node__failure" aria-label={failureEntry.title}>
      <strong>{failureEntry.title}</strong>
      <p>{failureEntry.message}</p>
    </section>
  );
}

function graphExecutionSlotCount(node: WorkflowExecutionGraphNode, completedCount: number, activeRunCount: number, hasFailure: boolean) {
  const loopBinding = node.bindings.find((binding) => binding.name === 'loop');
  const loopCount = loopBinding ? arrayLiteralItemCount(loopBinding.expression) : null;
  const failedCount = hasFailure ? 1 : 0;

  if (loopCount !== null) {
    return Math.max(loopCount, completedCount + activeRunCount + failedCount);
  }

  return Math.max(completedCount + activeRunCount + failedCount, 1);
}

function arrayLiteralItemCount(expression: string) {
  const trimmedExpression = expression.trim();

  if (!trimmedExpression.startsWith('[') || !trimmedExpression.endsWith(']')) {
    return null;
  }

  const innerExpression = trimmedExpression.slice(1, -1).trim();

  if (!innerExpression) {
    return 0;
  }

  return innerExpression.split(',').length;
}

function GraphInstructionPreview({ instruction, onOpen }: { instruction: string; onOpen: () => void }) {
  return (
    <button type="button" className="graph-node__instruction nodrag" onClick={onOpen}>
      <Handle id="instruction" type="target" position={Position.Left} className="graph-node__handle graph-node__handle--instruction graph-node__handle--instruction-section" isConnectable={false} />
      <p>{instruction}</p>
    </button>
  );
}

function GraphInstructionDialog({ node, open, onOpenChange }: { node: WorkflowExecutionGraphNode; open: boolean; onOpenChange: (open: boolean) => void }) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="graph-instruction-dialog">
        <DialogHeader>
          <DialogTitle>{node.label} instruction</DialogTitle>
          <DialogDescription>Prompt sent to this agent.</DialogDescription>
        </DialogHeader>
        <pre>{node.instruction}</pre>
      </DialogContent>
    </Dialog>
  );
}

function GraphDetails({ title, details, collapsible = false }: { title: string; details: WorkflowExecutionGraphNode['details']; collapsible?: boolean }) {
  const content = (
    <ul>
      {details.map((detail) => (
        <li key={`${detail.name}:${detail.value}`}>
          <small>{detail.name}</small>
          <code data-secret={detail.secret ? 'true' : 'false'}>{detail.value}</code>
        </li>
      ))}
    </ul>
  );

  if (collapsible) {
    return (
      <details className="graph-node__section graph-node__collapsible-section graph-node__details" open>
        <summary><span className="graph-node__section-label">{title}</span><small>{details.length}</small></summary>
        {content}
      </details>
    );
  }

  return (
    <section className="graph-node__section graph-node__details">
      <span>{title}</span>
      {content}
    </section>
  );
}

function GraphBindings({ bindings }: { bindings: WorkflowExecutionGraphNode['bindings'] }) {
  return (
    <section className="graph-node__section graph-node__bindings">
      <span>Bindings</span>
      <ul>
        {bindings.map((binding) => (
          <li key={`${binding.name}:${binding.expression}`}>
            <code>{binding.name}</code>
            <small>{binding.expression}</small>
          </li>
        ))}
      </ul>
    </section>
  );
}

function GraphLoopSummary({ node, config }: { node: WorkflowExecutionGraphNode; config: GraphConfig }) {
  const loopInfo = node.loop_info;

  if (!loopInfo) {
    return null;
  }

  return (
    <section className="graph-node__section graph-node__loop">
      <span><Layers3 /> Loop agent</span>
      <p>
        Runs once per <code>{loopInfo.pattern}</code> in iterable.
      </p>
      <div className="graph-node__loop-grid">
        <div>
          <small>Iterable</small>
          <pre className="graph-node__schema">{schemaBlock(loopInfo.iterable_schema, config)}</pre>
        </div>
        <div>
          <small>Iteration output</small>
          <pre className="graph-node__schema">{schemaBlock(loopInfo.iteration_output_schema, config)}</pre>
        </div>
      </div>
    </section>
  );
}

function GraphOutputAction({ node, outputEntries, onOpen }: { node: WorkflowExecutionGraphNode; outputEntries: GraphOutputEntry[]; onOpen: () => void }) {
  const totalBytes = outputEntries.reduce((bytes, outputEntry) => bytes + jsonByteSize(outputEntry.outputJson), 0);

  return (
    <section className="graph-node__section graph-node__runtime-output">
      <span>Latest output</span>
      <button type="button" className="graph-node__output-button nodrag" onClick={onOpen}>
        <Eye /> View {node.kind === 'output' ? 'workflow result' : node.loop_info ? `${outputEntries.length} iteration outputs` : 'agent output'}
        <small>{outputByteSize(totalBytes)}</small>
      </button>
    </section>
  );
}

function GraphOutputDialog({ node, outputEntries, open, openOutputIndex, onOpenChange }: { node: WorkflowExecutionGraphNode; outputEntries: GraphOutputEntry[]; open: boolean; openOutputIndex: number; onOpenChange: (open: boolean) => void }) {
  const [selectedOutputIndex, setSelectedOutputIndex] = useState(openOutputIndex);
  const entriesElementRef = useRef<HTMLDivElement | null>(null);
  const previousOutputEntryCountRef = useRef(outputEntries.length);
  const selectedOutputEntry = outputEntries[selectedOutputIndex] ?? outputEntries[0];

  useEffect(() => {
    setSelectedOutputIndex(Math.min(openOutputIndex, Math.max(outputEntries.length - 1, 0)));
  }, [openOutputIndex, outputEntries.length]);

  useEffect(() => {
    if (!open) {
      previousOutputEntryCountRef.current = outputEntries.length;

      return;
    }

    if (outputEntries.length > previousOutputEntryCountRef.current) {
      setSelectedOutputIndex(outputEntries.length - 1);
    }

    previousOutputEntryCountRef.current = outputEntries.length;
  }, [open, outputEntries.length]);

  useEffect(() => {
    const entriesElement = entriesElementRef.current;

    if (!entriesElement) {
      return;
    }

    // Loop agents can add many outputs while the dialog is open. Keep the
    // selector bounded and auto-scroll to the latest selected result.
    entriesElement.querySelector<HTMLElement>('[data-selected="true"]')?.scrollIntoView({ behavior: 'smooth', inline: 'end', block: 'nearest' });
  }, [selectedOutputIndex, outputEntries.length]);

  if (!selectedOutputEntry) {
    return null;
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="graph-output-dialog">
        <DialogHeader>
          <DialogTitle>{node.label} output</DialogTitle>
          <DialogDescription>{outputDescription(node, outputEntries)}</DialogDescription>
        </DialogHeader>
        <div className="graph-output-dialog__body">
          <div ref={entriesElementRef} className="graph-output-dialog__entries" role="tablist" aria-label={`${node.label} outputs`}>
            {outputEntries.map((outputEntry, outputIndex) => (
              <button
                key={`${outputEntry.title}-${outputIndex}`}
                type="button"
                className="graph-output-dialog__entry"
                role="tab"
                data-selected={outputIndex === selectedOutputIndex ? 'true' : 'false'}
                aria-selected={outputIndex === selectedOutputIndex}
                onClick={() => setSelectedOutputIndex(outputIndex)}
              >
                <strong>{outputEntry.title}</strong>
                <small>{outputByteSize(jsonByteSize(outputEntry.outputJson))}</small>
              </button>
            ))}
          </div>
          <JsonCodeEditor value={selectedOutputEntry.outputJson} readOnly className="graph-output-dialog__json" />
        </div>
      </DialogContent>
    </Dialog>
  );
}

function GraphPorts({ title, ports, fallback, config, collapsible = false, defaultOpen = true, showPortNames = true, targetHandleId, sourceHandleId }: { title: string; ports: WorkflowExecutionGraphNode['inputs']; fallback?: string; config: GraphConfig; collapsible?: boolean; defaultOpen?: boolean; showPortNames?: boolean; targetHandleId?: string; sourceHandleId?: string }) {
  const titleContent = <GraphPortTitle title={title} targetHandleId={targetHandleId} sourceHandleId={sourceHandleId} />;
  const visiblePorts = ports.filter((port) => schemaHasDisplayContent(port.schema));
  const schemaCount = visiblePorts.length;
  const schemaLabel = schemaCount === 1 ? '1 schema' : `${schemaCount} schemas`;
  const content = (
    <>
      {visiblePorts.length > 0 ? (
        <ul>
          {visiblePorts.map((port) => (
            <li key={port.name}>
              {showPortNames ? <code>{port.name}</code> : null}
              <pre className="graph-node__schema">{schemaBlock(port.schema, config)}</pre>
            </li>
          ))}
        </ul>
      ) : ports.length === 0 ? (
        <p>{fallback ?? 'No declared fields'}</p>
      ) : null}
    </>
  );

  if (collapsible) {
    return (
      <details className="graph-node__section graph-node__collapsible-section" open={defaultOpen}>
        <summary>{titleContent}<small>{schemaLabel}</small></summary>
        {content}
      </details>
    );
  }

  return (
    <section className="graph-node__section" data-empty={visiblePorts.length === 0 && ports.length > 0 ? 'true' : 'false'}>
      {titleContent}
      {content}
    </section>
  );
}

function schemaHasDisplayContent(schema: unknown): boolean {
  if (!isRecord(schema)) {
    return true;
  }

  if (isRecord(schema.properties)) {
    return Object.keys(schema.properties).length > 0;
  }

  if (Array.isArray(schema.enum) || Array.isArray(schema.anyOf) || Array.isArray(schema.oneOf) || Array.isArray(schema.prefixItems)) {
    return true;
  }

  if (schema.type === 'array') {
    return Boolean(schema.items);
  }

  if (schema.type === 'object') {
    return false;
  }

  return typeof schema.type === 'string';
}

function GraphPortTitle({ title, targetHandleId, sourceHandleId }: { title: string; targetHandleId?: string; sourceHandleId?: string }) {
  return (
    <span className="graph-node__section-label">
      {targetHandleId ? <Handle id={targetHandleId} type="target" position={Position.Left} className="graph-node__handle graph-node__handle--section graph-node__handle--inputs" isConnectable={false} /> : null}
      {title}
      {sourceHandleId ? <Handle id={sourceHandleId} type="source" position={Position.Right} className="graph-node__handle graph-node__handle--section graph-node__handle--output" isConnectable={false} /> : null}
    </span>
  );
}

function GraphMcpDefinitions({ tools, config }: { tools: WorkflowExecutionGraphTool[]; config: GraphConfig }) {
  const toolGroups = [
    { title: 'Tools', tools: tools.filter((tool) => tool.kind === 'mcp_tool') },
    { title: 'Prompts', tools: tools.filter((tool) => tool.kind === 'mcp_prompt') },
    { title: 'Resources', tools: tools.filter((tool) => tool.kind === 'mcp_resource') },
  ].filter((toolGroup) => toolGroup.tools.length > 0);

  return (
    <>
      {toolGroups.map((toolGroup, toolGroupIndex) => <GraphTools key={toolGroup.title} title={toolGroup.title} tools={toolGroup.tools} config={config} sourceHandleId={toolGroupIndex === 0 ? 'mcp-items' : undefined} collapsible />)}
    </>
  );
}

function GraphMcpAccess({ tools, config }: { tools: WorkflowExecutionGraphTool[]; config: GraphConfig }) {
  const [open, setOpen] = useState(false);
  const toolsByServerName = tools.reduce((groupedTools, tool) => {
    const serverName = tool.server_name ?? 'unknown';
    const serverTools = groupedTools.get(serverName) ?? [];

    serverTools.push(tool);
    groupedTools.set(serverName, serverTools);

    return groupedTools;
  }, new Map<string, WorkflowExecutionGraphTool[]>());
  const serverEntries = Array.from(toolsByServerName.entries());

  return (
    <>
      <section className="graph-node__section graph-node__tools graph-node__mcp-access">
        <span className="graph-node__section-label">
          <Handle id="mcp-access" type="target" position={Position.Left} className="graph-node__handle graph-node__handle--section graph-node__handle--mcp-access" isConnectable={false} />
          MCP access
        </span>
        <GraphToolSummaryButton title="View MCP access" tools={tools} detail={`${serverEntries.length} ${serverEntries.length === 1 ? 'server' : 'servers'}`} onOpen={() => setOpen(true)} />
      </section>
      <GraphMcpAccessDialog toolsByServerName={serverEntries} tools={tools} config={config} open={open} onOpenChange={setOpen} />
    </>
  );
}

function GraphTools({ title, tools, config, sourceHandleId, collapsible = false }: { title: string; tools: WorkflowExecutionGraphTool[]; config: GraphConfig; sourceHandleId?: string; collapsible?: boolean }) {
  const [open, setOpen] = useState(false);
  const content = <GraphToolSummaryButton title={`View ${title.toLowerCase()}`} tools={tools} onOpen={() => setOpen(true)} />;
  const titleContent = <GraphToolSectionTitle title={title} sourceHandleId={sourceHandleId} />;

  if (collapsible) {
    return (
      <>
        <details className="graph-node__section graph-node__collapsible-section graph-node__tools" open>
          <summary>{titleContent}<small>{tools.length}</small></summary>
          {content}
        </details>
        <GraphToolsDialog title={title} tools={tools} config={config} open={open} onOpenChange={setOpen} />
      </>
    );
  }

  return (
    <>
      <section className="graph-node__section graph-node__tools">
        {titleContent}
        {content}
      </section>
      <GraphToolsDialog title={title} tools={tools} config={config} open={open} onOpenChange={setOpen} />
    </>
  );
}

function GraphToolSectionTitle({ title, sourceHandleId }: { title: string; sourceHandleId?: string }) {
  return (
    <span className="graph-node__section-label">
      {title}
      {sourceHandleId ? <Handle id={sourceHandleId} type="source" position={Position.Right} className="graph-node__handle graph-node__handle--section graph-node__handle--mcp-items" isConnectable={false} /> : null}
    </span>
  );
}

function GraphToolSummaryButton({ title, tools, detail, onOpen }: { title: string; tools: WorkflowExecutionGraphTool[]; detail?: string; onOpen: () => void }) {
  const itemLabel = tools.length === 1 ? '1 item' : `${tools.length} items`;

  return (
    <button type="button" className="graph-tool-summary-button nodrag" onClick={onOpen}>
      <span>{title}</span>
      <small>{detail ? `${itemLabel} / ${detail}` : itemLabel}</small>
    </button>
  );
}

function GraphToolsDialog({ title, tools, config, open, onOpenChange }: { title: string; tools: WorkflowExecutionGraphTool[]; config: GraphConfig; open: boolean; onOpenChange: (open: boolean) => void }) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="graph-tools-dialog">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{tools.length === 1 ? '1 available item.' : `${tools.length} available items.`}</DialogDescription>
        </DialogHeader>
        <GraphToolList tools={tools} config={config} />
      </DialogContent>
    </Dialog>
  );
}

function GraphMcpAccessDialog({ toolsByServerName, tools, config, open, onOpenChange }: { toolsByServerName: Array<[string, WorkflowExecutionGraphTool[]]>; tools: WorkflowExecutionGraphTool[]; config: GraphConfig; open: boolean; onOpenChange: (open: boolean) => void }) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="graph-tools-dialog">
        <DialogHeader>
          <DialogTitle>MCP access</DialogTitle>
          <DialogDescription>{tools.length === 1 ? '1 MCP item available to this agent.' : `${tools.length} MCP items available to this agent.`}</DialogDescription>
        </DialogHeader>
        <div className="graph-tool-dialog__groups">
          {toolsByServerName.map(([serverName, serverTools]) => (
            <details key={serverName} className="graph-tool-dialog__group" open>
              <summary><strong>{serverName}</strong><small>{serverTools.length}</small></summary>
              <GraphToolList tools={serverTools} config={config} />
            </details>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  );
}

function GraphToolList({ tools, config }: { tools: WorkflowExecutionGraphTool[]; config: GraphConfig }) {
  return (
    <div className="graph-tool-dialog__entries">
      {tools.map((tool) => (
        <GraphToolDetails key={`${tool.kind}:${tool.name}:${tool.server_name ?? ''}:${tool.item_name ?? ''}`} tool={tool} config={config} />
      ))}
    </div>
  );
}

function GraphToolDetails({ tool, config }: { tool: WorkflowExecutionGraphTool; config: GraphConfig }) {
  const inputSchemaVisible = schemaHasDisplayContent(tool.input_schema);
  const outputSchemaVisible = schemaHasDisplayContent(tool.output_schema);
  const maxCallsLabel = toolMaxCallsLabel(tool);

  return (
    <details className="graph-tool-dialog__entry" open>
      <summary>
        <span><code>{mcpToolDisplayName(tool)}</code><small>{toolKindLabel(tool.kind)}</small></span>
        {maxCallsLabel ? <small>{maxCallsLabel}</small> : null}
      </summary>
      <div className="graph-tool-dialog__body">
        {tool.description ? <p>{tool.description}</p> : null}
        {tool.bindings && tool.bindings.length > 0 ? <GraphToolBindings tool={tool} /> : null}
        {inputSchemaVisible ? <GraphToolSchema title="Input schema" schema={tool.input_schema} config={config} /> : null}
        {outputSchemaVisible ? <GraphToolSchema title="Output schema" schema={tool.output_schema} config={config} /> : null}
      </div>
    </details>
  );
}

function GraphToolBindings({ tool }: { tool: WorkflowExecutionGraphTool }) {
  return (
    <dl className="graph-tool-dialog__bindings">
      {tool.bindings?.map((binding) => (
        <div key={`${tool.name}:${binding.name}`}>
          <dt>{binding.name}</dt>
          <dd>{binding.expression}</dd>
        </div>
      ))}
    </dl>
  );
}

function GraphToolSchema({ title, schema, config }: { title: string; schema: unknown; config: GraphConfig }) {
  return (
    <section className="graph-tool-dialog__schema">
      <strong>{title}</strong>
      <pre className="graph-node__schema">{schemaBlock(schema, config)}</pre>
    </section>
  );
}

interface ParsedWorkflowGraphDeclarations {
  providers: Map<string, ParsedProviderDeclaration>;
  models: Map<string, ParsedModelDeclaration>;
  mcpDeclarations: Map<string, ParsedMcpDeclaration>;
  agents: Map<string, ParsedAgentDeclaration>;
}

interface ParsedProviderDeclaration {
  name: string;
  driverName: string;
  details: ParsedGraphDetail[];
}

interface ParsedModelDeclaration {
  name: string;
  providerName: string;
  details: ParsedGraphDetail[];
}

interface ParsedMcpDeclaration {
  name: string;
  details: ParsedGraphDetail[];
  imports: ParsedMcpImport[];
}

interface ParsedMcpImport {
  name: string;
  itemName: string;
  kind: WorkflowExecutionGraphTool['kind'];
  bindings: ParsedGraphBinding[];
}

interface ParsedMcpImportWithServer extends ParsedMcpImport {
  serverName: string;
}

interface ParsedAgentDeclaration {
  name: string;
  modelName: string | null;
  instruction: string | null;
  bindings: ParsedGraphBinding[];
}

interface ParsedGraphDetail {
  name: string;
  value: string;
  expression: string;
  secret: boolean;
}

interface ParsedGraphBinding {
  name: string;
  expression: string;
}

function graphWithProviderModelDeclarations(graph: WorkflowExecutionGraph, declarations: ParsedWorkflowGraphDeclarations): WorkflowExecutionGraph {
  const nodesById = new Map(graph.nodes.map((node) => [node.id, normalizeWorkflowGraphNode(node)]));
  const edgesById = new Map(graph.edges.map((edge) => [edge.id, edge]));

  for (const node of Array.from(nodesById.values())) {
    if (node.kind !== 'agent') {
      continue;
    }

    const agentDeclaration = declarations.agents.get(node.id);
    const modelName = agentDeclaration?.modelName ?? node.model;

    if (!modelName) {
      continue;
    }

    const modelDeclaration = declarations.models.get(modelName);
    const providerName = modelDeclaration?.providerName ?? node.provider_name;

    if (!providerName) {
      continue;
    }

    const providerNodeId = graphProviderNodeId(providerName);
    const modelNodeId = graphModelNodeId(modelName);

    if (!nodesById.has(providerNodeId)) {
      nodesById.set(providerNodeId, providerGraphNode(providerName, declarations.providers.get(providerName)));
    }

    if (!nodesById.has(modelNodeId)) {
      nodesById.set(modelNodeId, modelGraphNode(modelName, providerName, modelDeclaration));
    }

    nodesById.set(node.id, {
      ...node,
      provider_name: providerName,
      model: modelName,
      instruction: agentDeclaration?.instruction ?? node.instruction ?? null,
      bindings: mergeGraphBindings(node.bindings, agentDeclaration?.bindings ?? []),
    });

    addGraphEdge(edgesById, providerNodeId, modelNodeId, 'client', 'provider_client');
    addGraphEdge(edgesById, modelNodeId, node.id, 'model', 'model');
    addRuntimeReferenceEdges(edgesById, providerNodeId, declarations.providers.get(providerName)?.details ?? []);
    addRuntimeReferenceEdges(edgesById, modelNodeId, modelDeclaration?.details ?? []);
  }

  for (const node of Array.from(nodesById.values())) {
    if (node.kind !== 'agent') {
      continue;
    }

    const toolsByServerName = mcpToolsByServerName(node.tools);

    for (const [serverName, tools] of toolsByServerName.entries()) {
      const mcpNodeId = graphMcpNodeId(serverName);

      if (!nodesById.has(mcpNodeId)) {
        nodesById.set(mcpNodeId, mcpGraphNode(serverName, declarations.mcpDeclarations.get(serverName), tools));
      }

      addGraphEdge(edgesById, mcpNodeId, node.id, mcpAccessLabel(tools), 'mcp_access');
      addRuntimeReferenceEdges(edgesById, mcpNodeId, declarations.mcpDeclarations.get(serverName)?.details ?? []);

      for (const tool of tools) {
        addRuntimeReferenceEdges(edgesById, mcpNodeId, tool.bindings ?? []);
      }
    }
  }

  for (const mcpDeclaration of declarations.mcpDeclarations.values()) {
    const mcpNodeId = graphMcpNodeId(mcpDeclaration.name);

    if (!nodesById.has(mcpNodeId) && mcpDeclaration.imports.length > 0) {
      nodesById.set(mcpNodeId, mcpGraphNode(mcpDeclaration.name, mcpDeclaration, []));
    }
  }

  return {
    ...graph,
    nodes: Array.from(nodesById.values()),
    edges: Array.from(edgesById.values()),
  };
}

function parseWorkflowGraphDeclarations(source: string): ParsedWorkflowGraphDeclarations {
  const providers = new Map<string, ParsedProviderDeclaration>();
  const models = new Map<string, ParsedModelDeclaration>();
  const mcpDeclarations = new Map<string, ParsedMcpDeclaration>();
  const agents = new Map<string, ParsedAgentDeclaration>();

  for (const block of declarationBlocks(source, /\bprovider\s+([A-Za-z_][A-Za-z0-9_]*)\s+from\s+([A-Za-z_][A-Za-z0-9_]*)\s*\{/g)) {
    providers.set(block.name, {
      name: block.name,
      driverName: block.secondaryName ?? block.name,
      details: parseGraphDetails(block.body),
    });
  }

  for (const block of declarationBlocks(source, /\bmodel\s+([A-Za-z_][A-Za-z0-9_]*)\s+from\s+([A-Za-z_][A-Za-z0-9_]*)\s*\{/g)) {
    models.set(block.name, {
      name: block.name,
      providerName: block.secondaryName ?? '',
      details: parseGraphDetails(block.body),
    });
  }

  for (const block of declarationBlocks(source, /\bmcp\s+([A-Za-z_][A-Za-z0-9_]*)\s*\{/g)) {
    mcpDeclarations.set(block.name, {
      name: block.name,
      details: parseGraphDetails(block.body),
      imports: [],
    });
  }

  for (const mcpImport of parseMcpImports(source)) {
    const mcpDeclaration = mcpDeclarations.get(mcpImport.serverName) ?? { name: mcpImport.serverName, details: [], imports: [] };

    mcpDeclaration.imports.push({ name: mcpImport.name, itemName: mcpImport.itemName, kind: mcpImport.kind, bindings: mcpImport.bindings });
    mcpDeclarations.set(mcpDeclaration.name, mcpDeclaration);
  }

  for (const block of declarationBlocks(source, /\bagent\s+([A-Za-z_][A-Za-z0-9_]*)([^{}]*)\{/g)) {
    const modelBinding = firstGraphBinding(block.body, 'model');
    const instructionBinding = firstGraphBinding(block.body, 'instruction');
    const bindings = parseGraphBindings(block.body).filter((binding) => binding.name !== 'model' && binding.name !== 'instruction');
    const loopExpression = loopExpressionFromAgentHeader(block.secondaryName ?? '');

    if (loopExpression) {
      bindings.unshift({ name: 'loop', expression: loopExpression });
    }

    agents.set(block.name, {
      name: block.name,
      modelName: modelBinding ? modelNameFromExpression(modelBinding.expression) : null,
      instruction: instructionBinding?.expression ?? null,
      bindings,
    });
  }

  return { providers, models, mcpDeclarations, agents };
}

function parseMcpImports(source: string): ParsedMcpImportWithServer[] {
  return [...parseIndividualMcpImports(source), ...parseBatchMcpImports(source)];
}

function parseIndividualMcpImports(source: string): ParsedMcpImportWithServer[] {
  const imports: ParsedMcpImportWithServer[] = [];
  const pattern = /\b(tool|prompt|resource)\s+([A-Za-z_][A-Za-z0-9_]*)\s+from\s+mcp\.([A-Za-z_][A-Za-z0-9_]*)(?:\.(tool|prompt|resource))?\.([A-Za-z_][A-Za-z0-9_]*)\s*/g;
  let match: RegExpExecArray | null;

  while ((match = pattern.exec(source)) !== null) {
    const declarationKind = mcpImportKind(match[1] ?? match[4] ?? 'tool');
    const importName = match[2] ?? '';
    const serverName = match[3] ?? '';
    const itemName = match[5] ?? importName;
    const nextCharacterIndex = pattern.lastIndex;
    const blockBody = source[nextCharacterIndex] === '{' ? mcpImportBlockBody(source, nextCharacterIndex) : '';

    imports.push({
      name: importName,
      itemName,
      serverName,
      kind: declarationKind,
      bindings: parseBindingsBlock(blockBody),
    });

    if (blockBody) {
      pattern.lastIndex = nextCharacterIndex + blockBody.length + 2;
    }
  }

  return imports;
}

function parseBatchMcpImports(source: string): ParsedMcpImportWithServer[] {
  const imports: ParsedMcpImportWithServer[] = [];

  for (const block of declarationBlocks(source, /\bfrom\s+mcp\.([A-Za-z_][A-Za-z0-9_]*)(?:\.(tool|prompt|resource))?\s*\{/g)) {
    const serverName = block.name;
    const defaultKind = block.secondaryName ? mcpImportKind(block.secondaryName) : null;
    const sharedBindings = parseBindingsBlock(block.body);

    for (const item of parseBatchMcpImportItems(block.body, defaultKind)) {
      imports.push({
        ...item,
        serverName,
        bindings: [...sharedBindings, ...item.bindings],
      });
    }
  }

  return imports;
}

function parseBatchMcpImportItems(source: string, defaultKind: WorkflowExecutionGraphTool['kind'] | null): ParsedMcpImport[] {
  const imports: ParsedMcpImport[] = [];
  const pattern = /\b(tool|prompt|resource)\s+([A-Za-z_][A-Za-z0-9_]*)\s*/g;
  let match: RegExpExecArray | null;

  while ((match = pattern.exec(source)) !== null) {
    const itemStartIndex = match.index;

    if (braceDepthAtIndex(source, itemStartIndex) !== 0) {
      continue;
    }

    const kind = defaultKind ?? mcpImportKind(match[1] ?? 'tool');
    const name = match[2] ?? '';
    const nextCharacterIndex = pattern.lastIndex;
    const blockBody = source[nextCharacterIndex] === '{' ? mcpImportBlockBody(source, nextCharacterIndex) : '';

    imports.push({
      name,
      itemName: name,
      kind,
      bindings: parseBindingsBlock(blockBody),
    });

    if (blockBody) {
      pattern.lastIndex = nextCharacterIndex + blockBody.length + 2;
    }
  }

  return imports;
}

function mcpImportBlockBody(source: string, openBraceIndex: number) {
  const closeBraceIndex = matchingBraceIndex(source, openBraceIndex);

  return closeBraceIndex === null ? '' : source.slice(openBraceIndex + 1, closeBraceIndex);
}

function parseBindingsBlock(source: string): ParsedGraphBinding[] {
  const bindingsMatch = /\bbindings\s*\{/.exec(source);

  if (!bindingsMatch) {
    return [];
  }

  const openBraceIndex = bindingsMatch.index + bindingsMatch[0].length - 1;
  const blockBody = mcpImportBlockBody(source, openBraceIndex);

  return parseGraphBindings(blockBody);
}

function mcpImportKind(kind: string): WorkflowExecutionGraphTool['kind'] {
  if (kind === 'prompt') {
    return 'mcp_prompt';
  }

  if (kind === 'resource') {
    return 'mcp_resource';
  }

  return 'mcp_tool';
}

function providerGraphNode(providerName: string, providerDeclaration: ParsedProviderDeclaration | undefined): WorkflowExecutionGraphNode {
  return normalizeWorkflowGraphNode({
    id: graphProviderNodeId(providerName),
    label: providerName,
    kind: 'provider',
    inputs: [],
    outputs: [{ name: 'client', schema: { type: 'object', title: 'Provider client' } }],
    dependencies: [],
    provider_name: providerDeclaration?.driverName ?? providerName,
    model: null,
    instruction: null,
    details: providerDeclaration?.details ?? [{ name: 'driver', value: providerName, expression: providerName, secret: false }],
    bindings: [],
    tools: [],
    execution_index: null,
    loop_info: null,
  });
}

function modelGraphNode(modelName: string, providerName: string, modelDeclaration: ParsedModelDeclaration | undefined): WorkflowExecutionGraphNode {
  const modelId = modelDeclaration?.details.find((detail) => detail.name === 'id')?.value ?? modelName;

  return normalizeWorkflowGraphNode({
    id: graphModelNodeId(modelName),
    label: modelId,
    kind: 'model',
    inputs: [{ name: 'client', schema: { type: 'object', title: 'Provider client' } }],
    outputs: [{ name: 'model', schema: { type: 'object', title: 'Language model' } }],
    dependencies: [graphProviderNodeId(providerName)],
    provider_name: providerName,
    model: modelName,
    instruction: null,
    details: [{ name: 'provider', value: providerName, expression: providerName, secret: false }, ...(modelDeclaration?.details ?? [])],
    bindings: [],
    tools: [],
    execution_index: null,
    loop_info: null,
  });
}

function mcpGraphNode(serverName: string, mcpDeclaration: ParsedMcpDeclaration | undefined, usedTools: WorkflowExecutionGraphTool[]): WorkflowExecutionGraphNode {
  const declaredTools = (mcpDeclaration?.imports ?? []).map((mcpImport) => mcpImportGraphTool(serverName, mcpImport));
  const tools = mergeMcpGraphTools(declaredTools, usedTools.filter(isMcpTool));

  return normalizeWorkflowGraphNode({
    id: graphMcpNodeId(serverName),
    label: serverName,
    kind: 'mcp',
    inputs: [],
    outputs: [{ name: 'items', schema: { type: 'object', title: 'MCP items' } }],
    dependencies: [],
    provider_name: serverName,
    model: null,
    instruction: null,
    details: mcpDeclaration?.details ?? [],
    bindings: [],
    tools,
    execution_index: null,
    loop_info: null,
  });
}

function mcpImportGraphTool(serverName: string, mcpImport: ParsedMcpImport): WorkflowExecutionGraphTool {
  return {
    name: mcpImport.name,
    kind: mcpImport.kind,
    server_name: serverName,
    item_name: mcpImport.itemName,
    description: null,
    max_calls: null,
    input_schema: WorkflowGraphOpenObjectSchema,
    output_schema: WorkflowGraphOpenObjectSchema,
    bindings: mcpImport.bindings,
  };
}

function mergeMcpGraphTools(declaredTools: WorkflowExecutionGraphTool[], usedTools: WorkflowExecutionGraphTool[]) {
  const toolsByKey = new Map<string, WorkflowExecutionGraphTool>();

  for (const tool of declaredTools) {
    toolsByKey.set(mcpToolKey(tool), tool);
  }

  for (const tool of usedTools) {
    const existingTool = toolsByKey.get(mcpToolKey(tool));

    toolsByKey.set(mcpToolKey(tool), {
      ...tool,
      bindings: existingTool?.bindings ?? tool.bindings,
    });
  }

  return Array.from(toolsByKey.values()).sort(compareGraphTools);
}

function addGraphEdge(edgesById: Map<string, WorkflowExecutionGraph['edges'][number]>, source: string, target: string, label: string, kind: WorkflowExecutionGraph['edges'][number]['kind']) {
  const id = `${source}->${target}:${label}`;

  if (edgesById.has(id)) {
    return;
  }

  edgesById.set(id, { id, source, target, label, kind });
}

function addRuntimeReferenceEdges(edgesById: Map<string, WorkflowExecutionGraph['edges'][number]>, target: string, details: Array<{ name: string; expression: string }>) {
  for (const detail of details) {
    if (!expressionUsesRuntime(detail.expression)) {
      continue;
    }

    addGraphEdge(edgesById, 'input', target, detail.name, 'input');
  }
}

function expressionUsesRuntime(expression: string) {
  return /\b(input|secrets)\./.test(expression);
}

function mergeGraphBindings(existingBindings: WorkflowExecutionGraphNode['bindings'], parsedBindings: ParsedGraphBinding[]) {
  const bindings = [...existingBindings];
  const bindingNames = new Set(bindings.map((binding) => binding.name));

  for (const binding of parsedBindings) {
    if (!bindingNames.has(binding.name)) {
      bindings.push(binding);
    }
  }

  return bindings;
}

interface DeclarationBlock {
  name: string;
  secondaryName: string | null;
  body: string;
}

function declarationBlocks(source: string, pattern: RegExp): DeclarationBlock[] {
  const blocks: DeclarationBlock[] = [];
  let match: RegExpExecArray | null;

  while ((match = pattern.exec(source)) !== null) {
    const openBraceIndex = pattern.lastIndex - 1;
    const closeBraceIndex = matchingBraceIndex(source, openBraceIndex);

    if (closeBraceIndex === null) {
      continue;
    }

    blocks.push({
      name: match[1] ?? '',
      secondaryName: match[2] ?? null,
      body: source.slice(openBraceIndex + 1, closeBraceIndex),
    });
    pattern.lastIndex = closeBraceIndex + 1;
  }

  return blocks;
}

function matchingBraceIndex(source: string, openBraceIndex: number) {
  let depth = 0;
  let quoted = false;
  let escaped = false;
  let multiline = false;

  for (let sourceIndex = openBraceIndex; sourceIndex < source.length; sourceIndex += 1) {
    const character = source[sourceIndex];

    if (multiline) {
      if (source.startsWith('"""', sourceIndex)) {
        multiline = false;
        sourceIndex += 2;
      }

      continue;
    }

    if (quoted) {
      if (escaped) {
        escaped = false;
      } else if (character === '\\') {
        escaped = true;
      } else if (character === '"') {
        quoted = false;
      }

      continue;
    }

    if (source.startsWith('"""', sourceIndex)) {
      multiline = true;
      sourceIndex += 2;
    } else if (character === '"') {
      quoted = true;
    } else if (character === '{') {
      depth += 1;
    } else if (character === '}') {
      depth -= 1;

      if (depth === 0) {
        return sourceIndex;
      }
    }
  }

  return null;
}

function parseGraphDetails(body: string): ParsedGraphDetail[] {
  return parseTopLevelGraphFields(body).map((field) => ({
    name: field.name,
    value: field.secret ? maskedGraphValue(field.expression) : stripGraphExpressionQuotes(field.expression),
    expression: stripGraphExpressionQuotes(field.expression),
    secret: field.secret,
  }));
}

function parseGraphBindings(body: string): ParsedGraphBinding[] {
  return parseTopLevelGraphFields(body).map((field) => ({
    name: field.name,
    expression: stripGraphExpressionQuotes(field.expression),
  }));
}

function firstGraphBinding(body: string, bindingName: string) {
  return parseGraphBindings(body).find((binding) => binding.name === bindingName) ?? null;
}

function parseTopLevelGraphFields(body: string) {
  const fields: Array<{ name: string; expression: string; secret: boolean }> = [];
  let bodyIndex = 0;

  while (bodyIndex < body.length) {
    const fieldMatch = /^\s*([A-Za-z_][A-Za-z0-9_]*)\s*:/m.exec(body.slice(bodyIndex));

    if (!fieldMatch || fieldMatch.index === undefined) {
      break;
    }

    const fieldStartIndex = bodyIndex + fieldMatch.index;

    if (braceDepthAtIndex(body, fieldStartIndex) !== 0) {
      bodyIndex = fieldStartIndex + fieldMatch[0].length;

      continue;
    }

    const name = fieldMatch[1] ?? '';
    const expressionStartIndex = fieldStartIndex + fieldMatch[0].length;
    const expressionEndIndex = topLevelExpressionEndIndex(body, expressionStartIndex);
    const expression = body.slice(expressionStartIndex, expressionEndIndex).trim();
    const normalizedName = name.toLowerCase();

    fields.push({ name, expression, secret: normalizedName.includes('key') || normalizedName.includes('secret') || normalizedName.includes('token') });
    bodyIndex = expressionEndIndex;
  }

  return fields;
}

function braceDepthAtIndex(source: string, targetIndex: number) {
  let depth = 0;
  let quoted = false;
  let escaped = false;
  let multiline = false;

  for (let sourceIndex = 0; sourceIndex < targetIndex; sourceIndex += 1) {
    const character = source[sourceIndex];

    if (multiline) {
      if (source.startsWith('"""', sourceIndex)) {
        multiline = false;
        sourceIndex += 2;
      }

      continue;
    }

    if (quoted) {
      if (escaped) {
        escaped = false;
      } else if (character === '\\') {
        escaped = true;
      } else if (character === '"') {
        quoted = false;
      }

      continue;
    }

    if (source.startsWith('"""', sourceIndex)) {
      multiline = true;
      sourceIndex += 2;
    } else if (character === '"') {
      quoted = true;
    } else if (character === '{') {
      depth += 1;
    } else if (character === '}') {
      depth = Math.max(depth - 1, 0);
    }
  }

  return depth;
}

function topLevelExpressionEndIndex(source: string, expressionStartIndex: number) {
  let depth = 0;
  let quoted = false;
  let escaped = false;
  let multiline = false;

  for (let sourceIndex = expressionStartIndex; sourceIndex < source.length; sourceIndex += 1) {
    const character = source[sourceIndex];

    if (multiline) {
      if (source.startsWith('"""', sourceIndex)) {
        return sourceIndex + 3;
      }

      continue;
    }

    if (quoted) {
      if (escaped) {
        escaped = false;
      } else if (character === '\\') {
        escaped = true;
      } else if (character === '"') {
        quoted = false;
      }

      continue;
    }

    if (source.startsWith('"""', sourceIndex)) {
      multiline = true;
      sourceIndex += 2;
    } else if (character === '"') {
      quoted = true;
    } else if (character === '{' || character === '[' || character === '(') {
      depth += 1;
    } else if (character === '}' || character === ']' || character === ')') {
      depth = Math.max(depth - 1, 0);
    } else if (character === '\n' && depth === 0) {
      return sourceIndex;
    }
  }

  return source.length;
}

function loopExpressionFromAgentHeader(header: string) {
  const loopMatch = /\bfor\s+.+?\s+in\s+(.+)$/.exec(header.trim());

  return loopMatch ? stripGraphExpressionQuotes(loopMatch[1] ?? '') : null;
}

function modelNameFromExpression(expression: string) {
  const modelMatch = /^model\.([A-Za-z_][A-Za-z0-9_]*)$/.exec(expression.trim());

  return modelMatch?.[1] ?? null;
}

function stripGraphExpressionQuotes(expression: string) {
  const trimmedExpression = expression.trim();

  if (trimmedExpression.startsWith('"""') && trimmedExpression.endsWith('"""')) {
    return normalizeMultilineGraphString(trimmedExpression.slice(3, -3));
  }

  if (trimmedExpression.startsWith('"') && trimmedExpression.endsWith('"')) {
    return trimmedExpression.slice(1, -1);
  }

  return trimmedExpression;
}

function normalizeMultilineGraphString(value: string) {
  const lines = value.replace(/^\n/, '').replace(/\n\s*$/, '').split('\n');
  const indentation = lines
    .filter((line) => line.trim().length > 0)
    .reduce((minimumIndentation, line) => Math.min(minimumIndentation, line.match(/^\s*/)?.[0].length ?? 0), Number.POSITIVE_INFINITY);

  if (!Number.isFinite(indentation)) {
    return '';
  }

  return lines.map((line) => line.slice(indentation)).join('\n').trim();
}

function maskedGraphValue(expression: string) {
  const value = stripGraphExpressionQuotes(expression);

  if (value.length <= 8) {
    return '****';
  }

  return `${value.slice(0, 2)}****${value.slice(-4)}`;
}

function graphProviderNodeId(providerName: string) {
  return `provider:${providerName}`;
}

function graphModelNodeId(modelName: string) {
  return `model:${modelName}`;
}

function graphMcpNodeId(serverName: string) {
  return `mcp:${serverName}`;
}

function isMcpTool(tool: WorkflowExecutionGraphTool) {
  return tool.kind === 'mcp_tool' || tool.kind === 'mcp_prompt' || tool.kind === 'mcp_resource';
}

function mcpToolsByServerName(tools: WorkflowExecutionGraphTool[]) {
  const toolsByServerName = new Map<string, WorkflowExecutionGraphTool[]>();

  for (const tool of tools) {
    if (!isMcpTool(tool) || !tool.server_name) {
      continue;
    }

    const serverTools = toolsByServerName.get(tool.server_name) ?? [];

    serverTools.push(tool);
    toolsByServerName.set(tool.server_name, serverTools);
  }

  return toolsByServerName;
}

function mcpAccessLabel(tools: WorkflowExecutionGraphTool[]) {
  const count = tools.length;

  return `${count} MCP ${count === 1 ? 'item' : 'items'}`;
}

function mcpToolKey(tool: WorkflowExecutionGraphTool) {
  return `${tool.kind}:${tool.server_name ?? ''}:${tool.item_name ?? tool.name}`;
}

function compareGraphTools(leftTool: WorkflowExecutionGraphTool, rightTool: WorkflowExecutionGraphTool) {
  const leftKind = toolKindLabel(leftTool.kind);
  const rightKind = toolKindLabel(rightTool.kind);

  if (leftKind !== rightKind) {
    return leftKind.localeCompare(rightKind);
  }

  return mcpToolDisplayName(leftTool).localeCompare(mcpToolDisplayName(rightTool));
}

function reactFlowNodes(graph: WorkflowExecutionGraph, config: GraphConfig, runState: RunState, activeRunCounts: Map<string, number>, outputEntriesByNodeId: Record<string, GraphOutputEntry[]>, failureEntriesByNodeId: Record<string, GraphFailureEntry>): WorkflowGraphReactNode[] {
  const graphNodes = graph.nodes;
  const agentNodes = graphNodes.filter((node) => node.kind === 'agent');
  const lastColumn = Math.max(agentNodes.length + 1, 1);

  return graphNodes.map((node) => ({
    id: node.id,
    type: 'workflowGraph',
    position: nodePosition(node, lastColumn, graphNodes),
    data: { node, config, runState, activeRunCount: activeRunCounts.get(node.id) ?? 0, outputEntries: outputEntriesByNodeId[node.id] ?? [], failureEntry: failureEntriesByNodeId[node.id] ?? null },
  }));
}

function mergeRuntimeNodeUpdates(currentNodes: WorkflowGraphReactNode[], incomingNodes: WorkflowGraphReactNode[]): WorkflowGraphReactNode[] {
  const currentNodesById = new Map(currentNodes.map((node) => [node.id, node]));

  return incomingNodes.map((incomingNode) => {
    const currentNode = currentNodesById.get(incomingNode.id);

    if (!currentNode) {
      return incomingNode;
    }

    if (sameGraphNodeRuntime(currentNode, incomingNode)) {
      return currentNode;
    }

    return {
      ...currentNode,
      ...incomingNode,
      position: currentNode.position,
      selected: currentNode.selected,
      dragging: currentNode.dragging,
    };
  });
}

function sameGraphNodeRuntime(currentNode: WorkflowGraphReactNode, incomingNode: WorkflowGraphReactNode) {
  return (
    currentNode.type === incomingNode.type &&
    currentNode.data.node === incomingNode.data.node &&
    currentNode.data.config === incomingNode.data.config &&
    currentNode.data.runState === incomingNode.data.runState &&
    currentNode.data.activeRunCount === incomingNode.data.activeRunCount &&
    sameGraphOutputEntries(currentNode.data.outputEntries, incomingNode.data.outputEntries) &&
    sameGraphFailureEntry(currentNode.data.failureEntry, incomingNode.data.failureEntry)
  );
}

function sameGraphFailureEntry(currentEntry: GraphFailureEntry | null, incomingEntry: GraphFailureEntry | null) {
  return currentEntry?.title === incomingEntry?.title && currentEntry?.message === incomingEntry?.message;
}

function sameGraphOutputEntries(currentEntries: GraphOutputEntry[], incomingEntries: GraphOutputEntry[]) {
  if (currentEntries.length !== incomingEntries.length) {
    return false;
  }

  return currentEntries.every((currentEntry, entryIndex) => {
    const incomingEntry = incomingEntries[entryIndex];

    return incomingEntry?.title === currentEntry.title && incomingEntry.outputJson === currentEntry.outputJson;
  });
}

function mergeRuntimeEdgeUpdates(currentEdges: Edge[], incomingEdges: Edge[]): Edge[] {
  const currentEdgesById = new Map(currentEdges.map((edge) => [edge.id, edge]));

  return incomingEdges.map((incomingEdge) => {
    const currentEdge = currentEdgesById.get(incomingEdge.id);

    if (!currentEdge) {
      return incomingEdge;
    }

    return sameGraphEdgeRuntime(currentEdge, incomingEdge) ? currentEdge : { ...currentEdge, ...incomingEdge, selected: currentEdge.selected };
  });
}

function sameGraphEdgeRuntime(currentEdge: Edge, incomingEdge: Edge) {
  return (
    currentEdge.source === incomingEdge.source &&
    currentEdge.target === incomingEdge.target &&
    currentEdge.sourceHandle === incomingEdge.sourceHandle &&
    currentEdge.targetHandle === incomingEdge.targetHandle &&
    currentEdge.label === incomingEdge.label &&
    currentEdge.type === incomingEdge.type &&
    currentEdge.animated === incomingEdge.animated &&
    currentEdge.className === incomingEdge.className
  );
}

function reactFlowEdges(graph: WorkflowExecutionGraph, config: GraphConfig, activeRunCounts: Map<string, number>, outputEntriesByNodeId: Record<string, GraphOutputEntry[]>, failureEntriesByNodeId: Record<string, GraphFailureEntry>): Edge[] {
  const graphNodesById = new Map(graph.nodes.map((node) => normalizeWorkflowGraphNode(node)).map((node) => [node.id, node]));

  return graph.edges.map((edge) => ({
    id: edge.id,
    source: edge.source,
    target: edge.target,
    sourceHandle: graphEdgeSourceHandle(edge.kind),
    targetHandle: graphEdgeTargetHandle(edge.kind),
    label: config.showEdgeLabels ? edge.label : undefined,
    type: config.edgeType,
    animated: (activeRunCounts.get(edge.target) ?? 0) > 0,
    className: graphEdgeClassName(edge.kind, graphNodesById.get(edge.target), activeRunCounts, outputEntriesByNodeId, failureEntriesByNodeId),
  }));
}

function graphEdgeSourceHandle(edgeKind: string) {
  if (edgeKind === 'provider_client') {
    return 'client';
  }

  if (edgeKind === 'model') {
    return 'model';
  }

  if (edgeKind === 'mcp_access') {
    return 'mcp-items';
  }

  return 'output';
}

function graphEdgeTargetHandle(edgeKind: string) {
  if (edgeKind === 'provider_client') {
    return 'client';
  }

  if (edgeKind === 'model') {
    return 'instruction';
  }

  if (edgeKind === 'mcp_access') {
    return 'mcp-access';
  }

  return 'inputs';
}

function inputPortTargetHandleId(node: WorkflowExecutionGraphNode) {
  if (node.kind === 'model') {
    return 'client';
  }

  if (node.kind !== 'input') {
    return 'inputs';
  }

  return undefined;
}

function outputPortSourceHandleId(node: WorkflowExecutionGraphNode) {
  if (node.kind === 'provider') {
    return 'client';
  }

  if (node.kind === 'model') {
    return 'model';
  }

  if (node.kind === 'mcp') {
    return undefined;
  }

  if (node.kind !== 'output') {
    return 'output';
  }

  return undefined;
}

function normalizeWorkflowGraphNode(node: WorkflowExecutionGraphNode): WorkflowExecutionGraphNode {
  return {
    ...node,
    inputs: Array.isArray(node.inputs) ? node.inputs : [],
    outputs: Array.isArray(node.outputs) ? node.outputs : [],
    dependencies: Array.isArray(node.dependencies) ? node.dependencies : [],
    instruction: typeof node.instruction === 'string' ? node.instruction : null,
    details: Array.isArray(node.details) ? node.details : [],
    bindings: Array.isArray(node.bindings) ? node.bindings : [],
    tools: Array.isArray(node.tools) ? node.tools.map(normalizeWorkflowGraphTool) : [],
    execution_index: typeof node.execution_index === 'number' ? node.execution_index : null,
    loop_info: node.loop_info ?? null,
    model: node.model ?? null,
    provider_name: node.provider_name ?? null,
  };
}

function normalizeWorkflowGraphTool(tool: WorkflowExecutionGraphTool): WorkflowExecutionGraphTool {
  return {
    ...tool,
    bindings: Array.isArray(tool.bindings) ? tool.bindings : [],
  };
}

function graphEdgeClassName(edgeKind: string, targetNode: WorkflowExecutionGraphNode | undefined, activeRunCounts: Map<string, number>, outputEntriesByNodeId: Record<string, GraphOutputEntry[]>, failureEntriesByNodeId: Record<string, GraphFailureEntry>) {
  const targetStatus = targetNode ? nodeStatus(targetNode, activeRunCounts.get(targetNode.id) ?? 0, outputEntriesByNodeId[targetNode.id] ?? [], failureEntriesByNodeId[targetNode.id] ?? null) : 'idle';

  return `graph-edge graph-edge--${edgeKind} graph-edge--${targetStatus}`;
}

function layoutWorkflowGraphNodes(currentNodes: WorkflowGraphReactNode[], currentEdges: Edge[]): WorkflowGraphReactNode[] {
  if (currentNodes.length === 0) {
    return currentNodes;
  }

  const nodeIdentifiers = new Set(currentNodes.map((node) => node.id));
  const layoutEdges = currentEdges.filter((edge) => nodeIdentifiers.has(edge.source) && nodeIdentifiers.has(edge.target));
  const ranksByNodeIdentifier = workflowGraphNodeRanks(currentNodes, layoutEdges);
  const columnsByRank = new Map<number, WorkflowGraphReactNode[]>();

  for (const node of currentNodes) {
    const rank = ranksByNodeIdentifier.get(node.id) ?? 0;
    const columnNodes = columnsByRank.get(rank) ?? [];

    columnNodes.push(node);
    columnsByRank.set(rank, columnNodes);
  }

  const sortedRanks = Array.from(columnsByRank.keys()).sort((leftRank, rightRank) => leftRank - rightRank);
  const columnWidths = sortedRanks.map((rank) => Math.max(...(columnsByRank.get(rank) ?? []).map(workflowGraphNodeWidth), graphLayoutDefaultNodeWidth));
  const totalWidth = columnWidths.reduce((widthTotal, columnWidth) => widthTotal + columnWidth, 0) + Math.max(sortedRanks.length - 1, 0) * graphLayoutColumnGap;
  const positionsByNodeIdentifier = new Map<string, GraphNodePosition>();
  let currentX = -totalWidth / 2;

  for (const [rankIndex, rank] of sortedRanks.entries()) {
    const columnNodes = [...(columnsByRank.get(rank) ?? [])].sort(compareWorkflowGraphNodesForLayout);
    const rowHeights = columnNodes.map(workflowGraphNodeHeight);
    const columnHeight = rowHeights.reduce((heightTotal, rowHeight) => heightTotal + rowHeight, 0) + Math.max(columnNodes.length - 1, 0) * graphLayoutRowGap;
    let currentY = -columnHeight / 2;

    for (const [nodeIndex, node] of columnNodes.entries()) {
      const rowHeight = rowHeights[nodeIndex] ?? graphLayoutDefaultNodeHeight;

      positionsByNodeIdentifier.set(node.id, {
        x: currentX + (columnWidths[rankIndex] ?? graphLayoutDefaultNodeWidth) / 2 - workflowGraphNodeWidth(node) / 2,
        y: currentY,
      });

      currentY += rowHeight + graphLayoutRowGap;
    }

    currentX += (columnWidths[rankIndex] ?? graphLayoutDefaultNodeWidth) + graphLayoutColumnGap;
  }

  return currentNodes.map((node) => ({
    ...node,
    position: positionsByNodeIdentifier.get(node.id) ?? node.position,
  }));
}

function workflowGraphNodeRanks(currentNodes: WorkflowGraphReactNode[], currentEdges: Edge[]) {
  const ranksByNodeIdentifier = new Map(currentNodes.map((node) => [node.id, initialWorkflowGraphNodeRank(node)]));
  const remainingIncomingCountByNodeIdentifier = new Map(currentNodes.map((node) => [node.id, 0]));
  const outgoingTargetsByNodeIdentifier = new Map(currentNodes.map((node) => [node.id, [] as string[]]));

  for (const edge of currentEdges) {
    remainingIncomingCountByNodeIdentifier.set(edge.target, (remainingIncomingCountByNodeIdentifier.get(edge.target) ?? 0) + 1);
    outgoingTargetsByNodeIdentifier.get(edge.source)?.push(edge.target);
  }

  const queuedNodeIdentifiers = currentNodes
    .filter((node) => (remainingIncomingCountByNodeIdentifier.get(node.id) ?? 0) === 0)
    .sort(compareWorkflowGraphNodesForLayout)
    .map((node) => node.id);

  while (queuedNodeIdentifiers.length > 0) {
    const nodeIdentifier = queuedNodeIdentifiers.shift();

    if (!nodeIdentifier) {
      continue;
    }

    const sourceRank = ranksByNodeIdentifier.get(nodeIdentifier) ?? 0;
    const outgoingTargets = [...(outgoingTargetsByNodeIdentifier.get(nodeIdentifier) ?? [])].sort();

    for (const targetIdentifier of outgoingTargets) {
      ranksByNodeIdentifier.set(targetIdentifier, Math.max(ranksByNodeIdentifier.get(targetIdentifier) ?? 0, sourceRank + 1));

      const remainingIncomingCount = (remainingIncomingCountByNodeIdentifier.get(targetIdentifier) ?? 0) - 1;
      remainingIncomingCountByNodeIdentifier.set(targetIdentifier, remainingIncomingCount);

      if (remainingIncomingCount === 0) {
        queuedNodeIdentifiers.push(targetIdentifier);
      }
    }
  }

  return ranksByNodeIdentifier;
}

function initialWorkflowGraphNodeRank(node: WorkflowGraphReactNode) {
  if (node.data.node.kind === 'provider' || node.data.node.kind === 'input') {
    return 0;
  }

  if (node.data.node.kind === 'model' || node.data.node.kind === 'mcp') {
    return 1;
  }

  if (node.data.node.kind === 'output') {
    return 3;
  }

  return 2;
}

function compareWorkflowGraphNodesForLayout(leftNode: WorkflowGraphReactNode, rightNode: WorkflowGraphReactNode) {
  const leftKindWeight = workflowGraphNodeKindLayoutWeight(leftNode.data.node.kind);
  const rightKindWeight = workflowGraphNodeKindLayoutWeight(rightNode.data.node.kind);

  if (leftKindWeight !== rightKindWeight) {
    return leftKindWeight - rightKindWeight;
  }

  const leftExecutionIndex = leftNode.data.node.execution_index ?? Number.MAX_SAFE_INTEGER;
  const rightExecutionIndex = rightNode.data.node.execution_index ?? Number.MAX_SAFE_INTEGER;

  if (leftExecutionIndex !== rightExecutionIndex) {
    return leftExecutionIndex - rightExecutionIndex;
  }

  return leftNode.data.node.label.localeCompare(rightNode.data.node.label);
}

function workflowGraphNodeKindLayoutWeight(nodeKind: WorkflowExecutionGraphNode['kind']) {
  if (nodeKind === 'provider') {
    return 0;
  }

  if (nodeKind === 'input') {
    return 1;
  }

  if (nodeKind === 'model') {
    return 2;
  }

  if (nodeKind === 'mcp') {
    return 3;
  }

  if (nodeKind === 'agent') {
    return 4;
  }

  return 5;
}

function workflowGraphNodeWidth(node: WorkflowGraphReactNode) {
  return node.measured?.width ?? node.width ?? graphLayoutDefaultNodeWidth;
}

function workflowGraphNodeHeight(node: WorkflowGraphReactNode) {
  return node.measured?.height ?? node.height ?? graphLayoutDefaultNodeHeight;
}

function nodePosition(node: WorkflowExecutionGraphNode, lastColumn: number, nodes: WorkflowExecutionGraphNode[]) {
  if (node.kind === 'provider') {
    return { x: 0, y: 120 + nodeKindIndex(node, nodes) * 260 };
  }

  if (node.kind === 'model') {
    return { x: 360, y: 120 + nodeKindIndex(node, nodes) * 260 };
  }

  if (node.kind === 'mcp') {
    return { x: 360, y: 430 + nodeKindIndex(node, nodes) * 260 };
  }

  if (node.kind === 'input') {
    return { x: 0, y: 430 };
  }

  if (node.kind === 'output') {
    return { x: (lastColumn + 2) * 360, y: 220 };
  }

  const executionIndex = node.execution_index ?? 0;
  const verticalLane = executionIndex % 3;

  return {
    x: (executionIndex + 2) * 360,
    y: 40 + verticalLane * 210,
  };
}

function nodeKindIndex(node: WorkflowExecutionGraphNode, nodes: WorkflowExecutionGraphNode[]) {
  return nodes.filter((candidateNode) => candidateNode.kind === node.kind).findIndex((candidateNode) => candidateNode.id === node.id);
}

function nodeSummary(node: WorkflowExecutionGraphNode) {
  const details = [`${node.inputs.length} input${node.inputs.length === 1 ? '' : 's'}`, `${node.outputs.length} output${node.outputs.length === 1 ? '' : 's'}`];

  if (node.tools.length > 0) {
    details.push(`${node.tools.length} tool${node.tools.length === 1 ? '' : 's'}`);
  }

  return details.join(' | ');
}

function nodeStatus(node: WorkflowExecutionGraphNode, activeRunCount: number, outputEntries: GraphOutputEntry[], failureEntry: GraphFailureEntry | null): GraphNodeStatus {
  if (failureEntry) {
    return 'failed';
  }

  if (activeRunCount > 0) {
    return 'running';
  }

  if (node.kind === 'provider' || node.kind === 'model' || node.kind === 'mcp' || node.kind === 'input' || outputEntries.length > 0) {
    return 'completed';
  }

  return 'idle';
}

function executionSlotStatus(slotIndex: number, completedCount: number, activeRunCount: number, hasFailure: boolean, runState: RunState): GraphExecutionSlotStatus {
  if (slotIndex < completedCount) {
    return 'completed';
  }

  if (slotIndex < completedCount + activeRunCount) {
    return 'running';
  }

  if (hasFailure && slotIndex === completedCount + activeRunCount) {
    return 'failed';
  }

  if (runState === 'running') {
    return 'waiting';
  }

  return 'idle';
}

function nodeSubtitle(node: WorkflowExecutionGraphNode) {
  if (node.kind === 'provider') {
    return 'Provider';
  }

  if (node.kind === 'model') {
    return 'Model';
  }

  if (node.kind === 'mcp') {
    return 'MCP server';
  }

  if (node.kind === 'input') {
    return 'Runtime values';
  }

  if (node.kind === 'output') {
    return 'Final payload';
  }

  if (node.loop_info) {
    return 'Agent loop';
  }

  return 'Agent';
}

function nodeIcon(node: WorkflowExecutionGraphNode) {
  if (node.kind === 'provider') {
    return <Cloud />;
  }

  if (node.kind === 'model') {
    return <Box />;
  }

  if (node.kind === 'mcp') {
    return <PlugZap />;
  }

  if (node.kind === 'input') {
    return <Layers3 />;
  }

  if (node.kind === 'output') {
    return <CheckCircle2 />;
  }

  if (node.loop_info) {
    return <Sparkles />;
  }

  if (node.tools.length > 0) {
    return <DatabaseZap />;
  }

  return <Cpu />;
}

function nodeColor(node: Node) {
  if (node.id.startsWith('provider:')) {
    return '#247ea3';
  }

  if (node.id.startsWith('model:')) {
    return '#8065c8';
  }

  if (node.id.startsWith('mcp:')) {
    return '#738069';
  }

  if (node.id === 'input') {
    return '#247ea3';
  }

  if (node.id === 'output') {
    return '#3f8f5f';
  }

  return '#c76500';
}

function toolMaxCallsLabel(tool: WorkflowExecutionGraphTool) {
  return tool.max_calls === null ? '' : `max ${tool.max_calls}`;
}

function toolKindLabel(kind: WorkflowExecutionGraphTool['kind']) {
  if (kind === 'mcp_tool') {
    return 'tool';
  }

  if (kind === 'mcp_prompt') {
    return 'prompt';
  }

  if (kind === 'mcp_resource') {
    return 'resource';
  }

  return 'local';
}

function mcpToolDisplayName(tool: WorkflowExecutionGraphTool | undefined) {
  if (!tool) {
    return 'MCP item';
  }

  return tool.name;
}

function schemaBlock(schema: unknown, config: GraphConfig) {
  return schemaLines(schema, 0, config).map((line, lineIndex, lines) => (
    <span key={`${line}-${lineIndex}`} className="graph-node__schema-line">
      {highlightSchemaLine(line)}
      {lineIndex < lines.length - 1 ? '\n' : null}
    </span>
  ));
}

function schemaLines(schema: unknown, depth: number, config: GraphConfig): string[] {
  if (!isRecord(schema)) {
    return ['unknown'];
  }

  const maybeSchema = nullableMember(schema);

  if (maybeSchema !== null) {
    const maybeLines = schemaLines(maybeSchema, depth, config);
    const firstLine = maybeLines[0] ?? 'unknown';

    return [`maybe ${firstLine}`, ...maybeLines.slice(1)];
  }

  const variantLines = variantSchemaLines(schema, depth, config);

  if (variantLines !== null) {
    return variantLines;
  }

  if (isRecord(schema.properties)) {
    const fields = Object.entries(schema.properties);

    if (fields.length > 0) {
      const lines = ['{'];

      for (const [fieldName, fieldSchema] of fields) {
        const fieldLines = schemaLines(fieldSchema, depth + 1, config);
        const firstLine = fieldLines[0] ?? 'unknown';
        const remainingLines = fieldLines.slice(1);

        lines.push(`${indent(depth + 1)}${fieldName}: ${firstLine}`);

        for (const remainingLine of remainingLines) {
          lines.push(remainingLine);
        }
      }

      lines.push(`${indent(depth)}}`);

      return lines;
    }
  }

  if (Array.isArray(schema.enum) && schema.enum.length > 0) {
    return [`enum { ${schema.enum.map(String).join(', ')} }`];
  }

  if (Array.isArray(schema.anyOf)) {
    return [`(${schema.anyOf.map((option) => schemaInline(option, config)).join(' | ')})`];
  }

  if (Array.isArray(schema.oneOf)) {
    return [`(${schema.oneOf.map((option) => schemaInline(option, config)).join(' | ')})`];
  }

  if (schema.type === 'array' && Array.isArray(schema.prefixItems)) {
    return [`(${schema.prefixItems.map((itemSchema) => schemaInline(itemSchema, config)).join(', ')})`];
  }

  if (schema.type === 'array' && isRecord(schema.items)) {
    return arraySchemaLines(schema, schema.items, depth, config);
  }

  if (schema.type === 'object') {
    return ['{', `${indent(depth)}}`];
  }

  if (typeof schema.type === 'string') {
    return [schema.type];
  }

  return ['unknown'];
}

function schemaInline(schema: unknown, config: GraphConfig): string {
  const lines = schemaLines(schema, 0, config);

  return lines.join(' ').replaceAll(/\s+/g, ' ');
}

function variantSchemaLines(schema: Record<string, unknown>, depth: number, config: GraphConfig): string[] | null {
  const discriminator = discriminatorName(schema);

  if (discriminator === null || !Array.isArray(schema.oneOf)) {
    return null;
  }

  const lines = [`variant ${discriminator} {`];

  for (const caseSchema of schema.oneOf) {
    if (!isRecord(caseSchema) || !isRecord(caseSchema.properties)) {
      continue;
    }

    const discriminatorSchema = caseSchema.properties[discriminator];
    const caseName = isRecord(discriminatorSchema) && typeof discriminatorSchema.const === 'string' ? discriminatorSchema.const : 'case';
    lines.push(`${indent(depth + 1)}${caseName} {`);

    for (const [fieldName, fieldSchema] of Object.entries(caseSchema.properties)) {
      if (fieldName === discriminator) {
        continue;
      }

      const fieldLines = schemaLines(fieldSchema, depth + 2, config);
      const firstLine = fieldLines[0] ?? 'unknown';
      const remainingLines = fieldLines.slice(1);
      lines.push(`${indent(depth + 2)}${fieldName}: ${firstLine}`);

      for (const remainingLine of remainingLines) {
        lines.push(remainingLine);
      }
    }

    lines.push(`${indent(depth + 1)}}`);
  }

  lines.push(`${indent(depth)}}`);

  return lines;
}

function arraySchemaLines(arraySchema: Record<string, unknown>, itemSchema: Record<string, unknown>, depth: number, config: GraphConfig): string[] {
  if (!isObjectSchema(itemSchema)) {
    const fixedLength = fixedArrayLength(arraySchema);
    const itemType = schemaInline(itemSchema, config);

    return [fixedLength === null ? `[${itemType}]` : `[${itemType}; ${fixedLength}]`];
  }

  if (isRecord(itemSchema.properties)) {
    const lines = ['[{'];

    for (const [fieldName, fieldSchema] of Object.entries(itemSchema.properties)) {
      const fieldLines = schemaLines(fieldSchema, depth + 1, config);
      const firstLine = fieldLines[0] ?? 'unknown';
      const remainingLines = fieldLines.slice(1);
      lines.push(`${indent(depth + 1)}${fieldName}: ${firstLine}`);

      for (const remainingLine of remainingLines) {
        lines.push(remainingLine);
      }
    }

    lines.push(`${indent(depth)}}]`);

    return lines;
  }

  const itemLines = schemaLines(itemSchema, depth + 1, config);
  const firstLine = itemLines[0] ?? 'unknown';
  const remainingLines = itemLines.slice(1);
  const lines = ['[', `${indent(depth + 1)}${firstLine}`];

  for (const remainingLine of remainingLines) {
    lines.push(remainingLine);
  }

  lines.push(`${indent(depth)}]`);

  return lines;
}

function isObjectSchema(schema: Record<string, unknown>) {
  return schema.type === 'object' || isRecord(schema.properties);
}

function fixedArrayLength(schema: Record<string, unknown>) {
  if (typeof schema.minItems !== 'number' || schema.minItems !== schema.maxItems) {
    return null;
  }

  return schema.minItems;
}

function activeAgentRunCounts(events: ExecutorEvent[]) {
  const activeRunCounts = new Map<string, number>();

  for (const event of events) {
    const agentName = event.agent_name ?? (event.kind === 'workflow_failed' && event.message ? agentNameFromFailureMessage(event.message) : null);

    if (!agentName) {
      continue;
    }

    if (event.kind === 'agent_started') {
      activeRunCounts.set(agentName, (activeRunCounts.get(agentName) ?? 0) + 1);
    }

    if (event.kind === 'agent_completed' || event.kind === 'workflow_failed') {
      const nextRunCount = Math.max((activeRunCounts.get(agentName) ?? 0) - 1, 0);

      if (nextRunCount === 0) {
        activeRunCounts.delete(agentName);
      } else {
        activeRunCounts.set(agentName, nextRunCount);
      }
    }
  }

  return activeRunCounts;
}

function graphOutputEntriesByNodeId(events: ExecutorEvent[], workflowOutputJson: string) {
  const outputEntriesByNodeId: Record<string, GraphOutputEntry[]> = {};

  for (const event of events) {
    if (event.kind !== 'agent_completed' || !event.agent_name || !isRecord(event.data) || !('output' in event.data)) {
      continue;
    }

    const outputEntries = outputEntriesByNodeId[event.agent_name] ?? [];
    outputEntries.push({
      title: `Iteration ${outputEntries.length + 1}`,
      outputJson: JSON.stringify(event.data.output, null, 2),
    });
    outputEntriesByNodeId[event.agent_name] = outputEntries;
  }

  if (workflowOutputJson.trim()) {
    outputEntriesByNodeId.output = [{ title: 'Workflow result', outputJson: workflowOutputJson }];
  }

  return outputEntriesByNodeId;
}

function graphFailureEntriesByNodeId(events: ExecutorEvent[]) {
  const failureEntriesByNodeId: Record<string, GraphFailureEntry> = {};

  for (const event of events) {
    const nodeIdentifier = failureNodeIdentifier(event);
    const failureMessage = event.message ?? failureMessageFromData(event.data);

    if (!nodeIdentifier || !failureMessage) {
      continue;
    }

    failureEntriesByNodeId[nodeIdentifier] = {
      title: event.kind === 'workflow_failed' ? 'Workflow failed here' : 'Execution failed',
      message: failureMessage,
    };
  }

  return failureEntriesByNodeId;
}

function failureNodeIdentifier(event: ExecutorEvent) {
  if (!event.kind.endsWith('_failed')) {
    return null;
  }

  if (event.agent_name) {
    return event.agent_name;
  }

  if (!event.message) {
    return null;
  }

  return agentNameFromFailureMessage(event.message);
}

function agentNameFromFailureMessage(message: string) {
  const patterns = [/agent execution failed for `([^`]+)`/, /agent `([^`]+)` output does not match/];

  for (const pattern of patterns) {
    const match = pattern.exec(message);

    if (typeof match?.[1] === 'string') {
      return match[1];
    }
  }

  return null;
}

function failureMessageFromData(data: unknown) {
  if (!isRecord(data)) {
    return null;
  }

  const error = data.error;

  if (typeof error === 'string') {
    return error;
  }

  return null;
}

function jsonByteSize(outputJson: string) {
  return new TextEncoder().encode(outputJson).length;
}

function outputByteSize(bytes: number) {
  if (bytes < 1024) {
    return `${bytes} B`;
  }

  return `${(bytes / 1024).toFixed(1)} KB`;
}

function outputDescription(node: WorkflowExecutionGraphNode, outputEntries: GraphOutputEntry[]) {
  if (node.kind === 'output') {
    return 'Final workflow result streamed by the executor.';
  }

  if (node.loop_info) {
    return `${outputEntries.length} loop iteration output${outputEntries.length === 1 ? '' : 's'} in completion order.`;
  }

  return 'Latest agent output streamed by the executor.';
}

function restoreGraphConfig(): GraphConfig {
  const savedConfig = localStorage.getItem(graphConfigStorageKey);

  if (!savedConfig) {
    return defaultGraphConfig;
  }

  try {
    const parsedConfig = JSON.parse(savedConfig) as Partial<GraphConfig>;

    return {
      density: parsedConfig.density === 'compact' || parsedConfig.density === 'comfortable' ? parsedConfig.density : defaultGraphConfig.density,
      collapseAll: typeof parsedConfig.collapseAll === 'boolean' ? parsedConfig.collapseAll : defaultGraphConfig.collapseAll,
      edgeType: isGraphEdgeType(parsedConfig.edgeType) ? parsedConfig.edgeType : defaultGraphConfig.edgeType,
      showEdgeLabels: typeof parsedConfig.showEdgeLabels === 'boolean' ? parsedConfig.showEdgeLabels : defaultGraphConfig.showEdgeLabels,
    };
  } catch {
    return defaultGraphConfig;
  }
}

function restoreGraphViewport(): Viewport | null {
  const savedViewport = localStorage.getItem(graphViewportStorageKey);

  if (!savedViewport) {
    return null;
  }

  try {
    const viewport = JSON.parse(savedViewport) as Partial<Viewport>;

    if (typeof viewport.x === 'number' && typeof viewport.y === 'number' && typeof viewport.zoom === 'number') {
      return { x: viewport.x, y: viewport.y, zoom: viewport.zoom };
    }
  } catch {
    return null;
  }

  return null;
}

function storeGraphViewport(viewport: Viewport) {
  localStorage.setItem(graphViewportStorageKey, JSON.stringify(viewport));
}

function sameGraphViewport(currentViewport: Viewport, nextViewport: Viewport) {
  const viewportTolerance = 0.001;

  return Math.abs(currentViewport.x - nextViewport.x) < viewportTolerance && Math.abs(currentViewport.y - nextViewport.y) < viewportTolerance && Math.abs(currentViewport.zoom - nextViewport.zoom) < viewportTolerance;
}

function restoreOrLayoutGraphNodePositions(nodes: WorkflowGraphReactNode[], edges: Edge[]) {
  const restoredPositions = restoreGraphNodePositionMap();
  const restoredNodes = nodes.map((node) => ({
    ...node,
    position: restoredPositions[node.id] ?? node.position,
  }));

  if (nodes.some((node) => restoredPositions[node.id])) {
    return restoredNodes;
  }

  return layoutWorkflowGraphNodes(restoredNodes, edges);
}

function restoreGraphNodePositionMap(): Record<string, { x: number; y: number }> {
  const savedPositions = localStorage.getItem(graphNodePositionsStorageKey);

  if (!savedPositions) {
    return {};
  }

  try {
    const positions = JSON.parse(savedPositions) as Record<string, { x?: unknown; y?: unknown }>;
    const restoredPositions: Record<string, { x: number; y: number }> = {};

    for (const [nodeId, position] of Object.entries(positions)) {
      if (typeof position.x === 'number' && typeof position.y === 'number') {
        restoredPositions[nodeId] = { x: position.x, y: position.y };
      }
    }

    return restoredPositions;
  } catch {
    return {};
  }
}

function storeGraphNodePositions(nodes: WorkflowGraphReactNode[]) {
  const positions = Object.fromEntries(nodes.map((node) => [node.id, node.position]));
  localStorage.setItem(graphNodePositionsStorageKey, JSON.stringify(positions));
}

function isGraphEdgeType(value: unknown): value is GraphEdgeType {
  return value === 'smoothstep' || value === 'straight' || value === 'default' || value === 'simplebezier';
}

function nullableMember(schema: Record<string, unknown>): unknown | null {
  const unionMembers = Array.isArray(schema.oneOf) ? schema.oneOf : schema.anyOf;

  if (!Array.isArray(unionMembers)) {
    return null;
  }

  const nonNullMembers = unionMembers.filter((unionMember) => !(isRecord(unionMember) && (unionMember.type === 'null' || unionMember.const === null)));

  if (nonNullMembers.length !== 1 || nonNullMembers.length === unionMembers.length) {
    return null;
  }

  return nonNullMembers[0] ?? null;
}

function discriminatorName(schema: Record<string, unknown>) {
  if (!isRecord(schema.discriminator) || typeof schema.discriminator.propertyName !== 'string') {
    return null;
  }

  return schema.discriminator.propertyName;
}

function highlightSchemaLine(line: string): ReactNode[] {
  const tokens = line.split(/(\s+|[{}[\]():;,]|\|)/g).filter((token) => token.length > 0);

  return tokens.map((token, tokenIndex) => {
    const className = schemaTokenClassName(token);

    if (className === null) {
      return token;
    }

    return (
      <span key={`${token}-${tokenIndex}`} className={className}>
        {token}
      </span>
    );
  });
}

function schemaTokenClassName(token: string) {
  if (/^\s+$/.test(token)) {
    return null;
  }

  if (['string', 'number', 'float', 'boolean', 'integer', 'null'].includes(token)) {
    return 'graph-node__schema-type';
  }

  if (['enum', 'maybe', 'variant'].includes(token)) {
    return 'graph-node__schema-keyword';
  }

  if (/^[{}[\]():;,|]$/.test(token)) {
    return 'graph-node__schema-punctuation';
  }

  return null;
}

function indent(depth: number) {
  return '  '.repeat(depth);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
