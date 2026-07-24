import '@xyflow/react/dist/style.css';
import { Background, Controls, Handle, MiniMap, Position, ReactFlow, ReactFlowProvider, useEdgesState, useNodesInitialized, useNodesState, useReactFlow, useUpdateNodeInternals, type Edge, type Node, type NodeProps, type Viewport } from '@xyflow/react';
import { Box, CheckCircle2, CircleDashed, Cloud, Cpu, DatabaseZap, Eye, GitBranch, Layers3, Loader2, PlugZap, RefreshCcw, Search, Settings2, Sparkles } from 'lucide-react';
import { useEffect, useLayoutEffect, useMemo, useRef, useState, type MutableRefObject, type ReactNode } from 'react';
import { Button } from '@/components/ui/button';
import JsonCodeEditor from '@/components/json-code-editor';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { ExecutorDiagnosticCode, ExecutorDiagnosticSeverity, ExecutorDiagnosticSubjectType, ExecutorEventKind, type ExecutorEvent, type GraphState, type RunState, type WorkflowExecutionGraph, type WorkflowExecutionGraphNode, type WorkflowExecutionGraphTool } from '@/types';

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
  plannedRunCount: number;
  executionSlots: GraphExecutionSlotStatus[];
  outputEntries: GraphOutputEntry[];
  failureEntry: GraphFailureEntry | null;
  onSelectNode: (nodeIdentifier: string) => void;
  selected: boolean;
}

interface GraphOutputEntry {
  title: string;
  outputJson: string;
  iterationIndex: number | null;
}

interface GraphFailureEntry {
  title: string;
  message: string;
  status: 'failed' | 'cancelled';
}

interface GraphRuntimeNotice {
  title: string;
  message: string;
  tone: 'error' | 'warning' | 'cancelled' | 'gap';
}
interface GraphRuntimeSummary {
  activeRunCounts: Map<string, number>;
  plannedRunCountsByNodeId: Record<string, number>;
  executionSlotsByNodeId: Record<string, GraphExecutionSlotStatus[]>;
  outputEntriesByNodeId: Record<string, GraphOutputEntry[]>;
  failureEntriesByNodeId: Record<string, GraphFailureEntry>;
  globalNotices: GraphRuntimeNotice[];
}

type WorkflowGraphReactNode = Node<WorkflowGraphNodeData, 'workflowGraph'>;
type GraphDensity = 'compact' | 'comfortable';
type GraphEdgeType = 'smoothstep' | 'straight' | 'default' | 'simplebezier';
type GraphNodeStatus = 'idle' | 'running' | 'completed' | 'failed' | 'cancelled';
type GraphExecutionSlotStatus = 'completed' | 'running' | 'failed' | 'cancelled' | 'waiting' | 'idle';
type GraphExecutionSummary = Record<GraphExecutionSlotStatus, number>;

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
const defaultGraphConfig: GraphConfig = { density: 'compact', collapseAll: true, edgeType: 'smoothstep', showEdgeLabels: false };
const graphLayoutColumnGap = 96;
const graphLayoutRowGap = 48;
const graphLayoutDefaultNodeWidth = 276;
const graphLayoutDefaultNodeHeight = 260;
const graphExecutionStripSlotRenderLimit = 240;
const graphOutputEntryRowHeight = 74;
const graphOutputEntryOverscanRows = 5;
const defaultGraphViewport: Viewport = { x: 0, y: 0, zoom: 0.85 };
const WorkflowGraphOpenObjectSchema = { type: 'object', additionalProperties: true };
const schemaInlineLengthLimit = 58;

const graphNodeTypes = {
  workflowGraph: WorkflowGraphNodeCard,
};

export default function WorkflowGraphView({ graph, source, graphState, runState, events, outputJson, message, onRefresh }: WorkflowGraphViewProps) {
  const [config, setConfig] = useState<GraphConfig>(() => restoreGraphConfig());
  const [layoutRequestCount, setLayoutRequestCount] = useState(0);
  const [selectedNodeIdentifier, setSelectedNodeIdentifier] = useState<string | null>(null);
  const workflowDeclarations = useMemo(() => parseWorkflowGraphDeclarations(source), [source]);
  const graphRuntimeSummary = useMemo(() => graphRuntimeSummaryFromEvents(events, outputJson, runState), [events, outputJson, runState]);
  const activeRunCounts = graphRuntimeSummary.activeRunCounts;
  const outputEntriesByNodeId = graphRuntimeSummary.outputEntriesByNodeId;
  const failureEntriesByNodeId = graphRuntimeSummary.failureEntriesByNodeId;
  const plannedRunCountsByNodeId = graphRuntimeSummary.plannedRunCountsByNodeId;
  const executionSlotsByNodeId = graphRuntimeSummary.executionSlotsByNodeId;
  const activeAgentSignature = Array.from(activeRunCounts.entries()).sort().map(([agentName, activeRunCount]) => `${agentName}:${activeRunCount}`).join(':');
  const enrichedGraph = useMemo(() => (graph ? graphWithProviderModelDeclarations(graph, workflowDeclarations) : null), [graph, workflowDeclarations]);
  const executionGraph = useMemo(() => (enrichedGraph ? runtimeExecutionGraph(enrichedGraph) : null), [enrichedGraph]);
  const canvasConfig = useMemo<GraphConfig>(() => ({ ...config, collapseAll: true }), [config]);
  const nodes = useMemo(() => (executionGraph ? reactFlowNodes(executionGraph, canvasConfig, runState, activeRunCounts, plannedRunCountsByNodeId, executionSlotsByNodeId, outputEntriesByNodeId, failureEntriesByNodeId, selectedNodeIdentifier, setSelectedNodeIdentifier) : []), [executionGraph, canvasConfig, runState, activeAgentSignature, plannedRunCountsByNodeId, executionSlotsByNodeId, outputEntriesByNodeId, failureEntriesByNodeId, selectedNodeIdentifier]);
  const edges = useMemo(() => (executionGraph ? reactFlowEdges(executionGraph, canvasConfig, runState, activeRunCounts, outputEntriesByNodeId, failureEntriesByNodeId) : []), [executionGraph, canvasConfig, runState, activeAgentSignature, outputEntriesByNodeId, failureEntriesByNodeId]);
  const graphSignature = executionGraph ? workflowGraphSignature(executionGraph) : 'empty';
  const selectedNode = executionGraph?.nodes.find((node) => node.id === selectedNodeIdentifier) ?? executionGraph?.nodes[0] ?? null;

  useEffect(() => {
    try {
      localStorage.setItem(graphConfigStorageKey, JSON.stringify(config));
    } catch (error) {
      console.warn('Unable to persist workflow graph config.', error);
    }
  }, [config]);

  useEffect(() => {
    if (!executionGraph || executionGraph.nodes.length === 0) {
      setSelectedNodeIdentifier(null);

      return;
    }

    if (!executionGraph.nodes.some((node) => node.id === selectedNodeIdentifier)) {
      setSelectedNodeIdentifier(executionGraph.nodes[0]?.id ?? null);
    }
  }, [executionGraph, selectedNodeIdentifier]);

  return (
    <section className="graph-view">
      <div className="graph-run-summary" role="status" aria-live="polite">
        <GraphStateBadge graphState={graphState} />
        <span><strong>{runState}</strong> run</span>
        <span>{events.length} events</span>
        <span>{executionGraph?.nodes.length ?? 0} execution nodes</span>
      </div>

      {graphRuntimeSummary.globalNotices.length > 0 ? (
        <div className="graph-global-failures" aria-live="polite">
          <strong>Runtime notices</strong>
          {graphRuntimeSummary.globalNotices.map((notice) => (
            <p key={`${notice.title}:${notice.message}`} data-tone={notice.tone}>
              <b>{notice.title}</b> {notice.message}
            </p>
          ))}
        </div>
      ) : null}

      {executionGraph ? (
        <div className="graph-view__workspace">
          <div className="graph-view__canvas graph-view__canvas--desktop" data-empty="false">
            <div className="graph-view__toolbar">
              <button type="button" className="graph-view__toolbar-button" onClick={() => setLayoutRequestCount((currentCount) => currentCount + 1)} disabled={graphState === 'loading'}>
                <GitBranch /> Arrange
              </button>
              <button type="button" className="graph-view__toolbar-button" onClick={onRefresh} disabled={graphState === 'loading'}>
                <RefreshCcw className={graphState === 'loading' ? 'animate-spin' : ''} /> Refresh
              </button>
              <GraphSettingsMenu config={config} graphState={graphState} onChange={setConfig} onRefresh={onRefresh} />
            </div>

            <div className="graph-view__flow">
              <ReactFlowProvider>
                <GraphCanvas
                  nodes={nodes}
                  edges={edges}
                  graphSignature={graphSignature}
                  layoutRequestCount={layoutRequestCount}
                  onSelectNode={setSelectedNodeIdentifier}
                />
              </ReactFlowProvider>
            </div>
          </div>

          <GraphMobileExecutionList nodes={nodes} selectedNodeIdentifier={selectedNode?.id ?? null} onSelectNode={setSelectedNodeIdentifier} />
          {selectedNode ? (
            <GraphSelectionInspector
              node={selectedNode}
              config={config}
              runState={runState}
              activeRunCount={activeRunCounts.get(selectedNode.id) ?? 0}
              outputEntries={outputEntriesByNodeId[selectedNode.id] ?? []}
              failureEntry={failureEntriesByNodeId[selectedNode.id] ?? null}
            />
          ) : null}
        </div>
      ) : (
        <div className="graph-view__canvas graph-view__canvas--empty" data-empty="true">
          <div className="graph-view__empty">
            <GitBranch />
            <strong>{graphState === 'failed' ? 'Unable to build graph' : 'Graph not generated yet'}</strong>
            <p>{message}</p>
            <Button variant="secondary" size="lg" className="graph-view__button" onClick={onRefresh} disabled={graphState === 'loading'}>
              <RefreshCcw className={graphState === 'loading' ? 'animate-spin' : ''} /> Generate graph
            </Button>
          </div>
        </div>
      )}

      <p className={`graph-view__message graph-view__message--${graphState}`} role={graphState === 'failed' ? 'alert' : 'status'} aria-live="polite">{message}</p>
    </section>
  );
}

function GraphStateBadge({ graphState }: { graphState: GraphState }) {
  const label = graphState === 'ready' ? 'Plan ready' : graphState === 'loading' ? 'Building plan' : graphState === 'failed' ? 'Plan failed' : 'Plan idle';

  return (
    <span className={`graph-view__state graph-view__state--${graphState}`}>
      {graphState === 'loading' ? <Loader2 /> : graphState === 'ready' ? <CheckCircle2 /> : <CircleDashed />}
      {label}
    </span>
  );
}

function GraphMobileExecutionList({ nodes, selectedNodeIdentifier, onSelectNode }: { nodes: WorkflowGraphReactNode[]; selectedNodeIdentifier: string | null; onSelectNode: (nodeIdentifier: string) => void }) {
  const orderedNodes = [...nodes].sort((leftNode, rightNode) => {
    const leftExecutionIndex = leftNode.data.node.execution_index ?? Number.MAX_SAFE_INTEGER;
    const rightExecutionIndex = rightNode.data.node.execution_index ?? Number.MAX_SAFE_INTEGER;

    return leftExecutionIndex - rightExecutionIndex || leftNode.data.node.label.localeCompare(rightNode.data.node.label);
  });

  return (
    <section className="graph-mobile-list" aria-label="Ordered workflow execution">
      <header>
        <strong>Execution order</strong>
        <small>{orderedNodes.length} nodes</small>
      </header>
      <ol>
        {orderedNodes.map((reactNode, nodeIndex) => {
          const node = reactNode.data.node;
          const status = nodeStatus(node, reactNode.data.runState, reactNode.data.activeRunCount, reactNode.data.outputEntries, reactNode.data.failureEntry);

          return (
            <li key={node.id}>
              <button
                type="button"
                aria-pressed={selectedNodeIdentifier === node.id}
                data-selected={selectedNodeIdentifier === node.id ? 'true' : 'false'}
                onClick={() => onSelectNode(node.id)}
              >
                <span className="graph-mobile-list__index">{node.execution_index ?? nodeIndex + 1}</span>
                <span className="graph-mobile-list__identity">
                  <strong>{node.label}</strong>
                  <small>{nodeSubtitle(node)}</small>
                </span>
                <NodeStatusBadge status={status} activeRunCount={reactNode.data.activeRunCount} outputEntries={reactNode.data.outputEntries} />
              </button>
            </li>
          );
        })}
      </ol>
    </section>
  );
}

function GraphSelectionInspector({ node, config, runState, activeRunCount, outputEntries, failureEntry }: { node: WorkflowExecutionGraphNode; config: GraphConfig; runState: RunState; activeRunCount: number; outputEntries: GraphOutputEntry[]; failureEntry: GraphFailureEntry | null }) {
  const [outputOpen, setOutputOpen] = useState(false);
  const status = nodeStatus(node, runState, activeRunCount, outputEntries, failureEntry);

  return (
    <aside className="graph-selection-inspector" aria-label={`${node.label} details`}>
      <header className="graph-selection-inspector__header">
        <span>
          <small>Selected execution node</small>
          <strong>{node.label}</strong>
        </span>
        <NodeStatusBadge status={status} activeRunCount={activeRunCount} outputEntries={outputEntries} />
      </header>

      <div className="graph-selection-inspector__badges">
        <span>{node.kind}</span>
        {node.model ? <span>model {node.model}</span> : null}
        {node.provider_name ? <span>provider {node.provider_name}</span> : null}
      </div>

      {failureEntry ? <GraphFailureNotice failureEntry={failureEntry} /> : null}

      {node.instruction ? (
        <section>
          <h3>Instruction</h3>
          <pre>{node.instruction}</pre>
        </section>
      ) : null}

      {node.loop_info ? (
        <section>
          <h3>Loop execution</h3>
          <p>Runs once per <code>{node.loop_info.pattern}</code> in the configured iterable.</p>
          <small>{outputEntries.length} completed iteration outputs</small>
        </section>
      ) : null}

      {node.details.length > 0 ? (
        <section>
          <h3>Configuration</h3>
          <dl>
            {node.details.map((detail) => (
              <div key={`${detail.name}:${detail.value}`}>
                <dt>{detail.name}</dt>
                <dd>{detail.value}</dd>
              </div>
            ))}
          </dl>
        </section>
      ) : null}

      {node.bindings.length > 0 ? (
        <section>
          <h3>Bindings</h3>
          <dl>
            {node.bindings.map((binding) => (
              <div key={`${binding.name}:${binding.expression}`}>
                <dt>{binding.name}</dt>
                <dd>{binding.expression}</dd>
              </div>
            ))}
          </dl>
        </section>
      ) : null}

      {node.inputs.length > 0 ? (
        <section>
          <h3>Inputs</h3>
          {node.inputs.map((port) => (
            <div className="graph-selection-inspector__schema" key={port.name}>
              <strong>{port.name}</strong>
              <pre>{schemaBlock(port.schema, config)}</pre>
            </div>
          ))}
        </section>
      ) : null}

      {node.outputs.length > 0 ? (
        <section>
          <h3>Outputs</h3>
          {node.outputs.map((port) => (
            <div className="graph-selection-inspector__schema" key={port.name}>
              <strong>{port.name}</strong>
              <pre>{schemaBlock(port.schema, config)}</pre>
            </div>
          ))}
        </section>
      ) : null}

      {node.tools.length > 0 ? (
        <section>
          <h3>Tools and MCP access</h3>
          <ul>
            {node.tools.map((tool) => (
              <li key={mcpToolKey(tool)}>
                <details>
                  <summary>
                    <strong>{mcpToolDisplayName(tool)}</strong>
                    <small>{toolKindLabel(tool.kind)}{tool.server_name ? ` · ${tool.server_name}` : ''}{tool.max_calls === null ? '' : ` · max ${tool.max_calls}`}</small>
                  </summary>
                  {tool.description ? <p>{tool.description}</p> : null}
                  {(tool.bindings ?? []).length > 0 ? (
                    <dl>
                      {(tool.bindings ?? []).map((binding) => (
                        <div key={`${binding.name}:${binding.expression}`}>
                          <dt>{binding.name}</dt>
                          <dd>{binding.expression}</dd>
                        </div>
                      ))}
                    </dl>
                  ) : null}
                  {schemaHasDisplayContent(tool.input_schema) ? <div className="graph-selection-inspector__schema"><strong>Input schema</strong><pre>{schemaBlock(tool.input_schema, config)}</pre></div> : null}
                  {schemaHasDisplayContent(tool.output_schema) ? <div className="graph-selection-inspector__schema"><strong>Output schema</strong><pre>{schemaBlock(tool.output_schema, config)}</pre></div> : null}
                </details>
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      {outputEntries.length > 0 ? (
        <Button variant="outline" onClick={() => setOutputOpen(true)}><Eye /> View {outputEntries.length === 1 ? 'output' : `${outputEntries.length} outputs`}</Button>
      ) : null}

      {outputEntries.length > 0 ? (
        <GraphOutputDialog node={node} outputEntries={outputEntries} open={outputOpen} openOutputIndex={0} onOpenChange={setOutputOpen} />
      ) : null}
    </aside>
  );
}

function runtimeExecutionGraph(graph: WorkflowExecutionGraph): WorkflowExecutionGraph {
  const nodes = graph.nodes.filter((node) => {
    switch (node.kind) {
      case 'input':
      case 'agent':
      case 'dynamic':
      case 'compact':
      case 'output':
        return true;
      case 'provider':
      case 'model':
      case 'mcp':
        return false;
    }
  });
  const nodeIdentifiers = new Set(nodes.map((node) => node.id));
  const edgesByEndpoints = new Map<string, { edge: WorkflowExecutionGraph['edges'][number]; labels: string[] }>();

  for (const edge of graph.edges) {
    if (!nodeIdentifiers.has(edge.source) || !nodeIdentifiers.has(edge.target)) {
      continue;
    }

    const endpointKey = `${edge.source}\u0000${edge.target}`;
    const existingEntry = edgesByEndpoints.get(endpointKey);

    if (existingEntry) {
      if (!existingEntry.labels.includes(edge.label)) {
        existingEntry.labels.push(edge.label);
      }

      continue;
    }

    edgesByEndpoints.set(endpointKey, { edge, labels: [edge.label] });
  }

  return {
    ...graph,
    nodes,
    edges: Array.from(edgesByEndpoints.values()).map(({ edge, labels }) => ({
      ...edge,
      id: `execution:${edge.source}:${edge.target}`,
      label: labels.length === 1 ? labels[0] ?? edge.label : `${labels.length} bindings`,
    })),
  };
}

function workflowGraphSignature(graph: WorkflowExecutionGraph) {
  const nodeSignature = graph.nodes.map((node) => node.id).sort().join(':');
  const edgeSignature = graph.edges
    .map((edge) => `${edge.source}:${edge.target}:${edge.kind}:${edge.label}`)
    .sort()
    .join(':');

  return `${nodeSignature}|${edgeSignature}`;
}

function GraphCanvas({ nodes: incomingNodes, edges: incomingEdges, graphSignature, layoutRequestCount, onSelectNode }: { nodes: WorkflowGraphReactNode[]; edges: Edge[]; graphSignature: string; layoutRequestCount: number; onSelectNode: (nodeId: string) => void }) {
  const restoredViewportRef = useRef<Viewport | null>(restoreGraphViewport());
  const initialFitViewCompleteRef = useRef(false);
  const initialMeasuredLayoutSignatureRef = useRef<string | null>(null);
  const initialNodesRef = useRef<WorkflowGraphReactNode[] | null>(null);
  const currentViewportRef = useRef<Viewport>(restoredViewportRef.current ?? defaultGraphViewport);

  if (initialNodesRef.current === null) {
    initialNodesRef.current = layoutWorkflowGraphNodes(incomingNodes, incomingEdges);
  }

  const [nodes, setNodes, onNodesChange] = useNodesState(initialNodesRef.current);
  const [edges, setEdges, onEdgesChange] = useEdgesState(incomingEdges);
  const reactFlowInstance = useReactFlow();
  const nodesInitialized = useNodesInitialized();

  useEffect(() => {
    initialMeasuredLayoutSignatureRef.current = null;
    setEdges(incomingEdges);
    setNodes(layoutWorkflowGraphNodes(incomingNodes, incomingEdges));
  }, [graphSignature, setEdges, setNodes]);

  useEffect(() => {
    const preservedViewport = currentViewportRef.current;

    // Runtime events can arrive many times during loop agents. They may change
    // labels, badges, and outputs, but must never fit, pan, or zoom the canvas.
    setNodes((currentNodes) => mergeRuntimeNodeUpdates(currentNodes, incomingNodes));
    preserveGraphViewport(reactFlowInstance, currentViewportRef, preservedViewport);
  }, [incomingNodes, reactFlowInstance, setNodes]);

  useEffect(() => {
    const preservedViewport = currentViewportRef.current;

    // Keep edge status updates data-only as well; viewport control stays with
    // the user and the explicit Arrange action above.
    setEdges((currentEdges) => mergeRuntimeEdgeUpdates(currentEdges, incomingEdges));
    preserveGraphViewport(reactFlowInstance, currentViewportRef, preservedViewport);
  }, [incomingEdges, reactFlowInstance, setEdges]);

  useEffect(() => {
    if (!nodesInitialized || graphSignature === 'empty' || initialMeasuredLayoutSignatureRef.current === graphSignature) {
      return;
    }

    initialMeasuredLayoutSignatureRef.current = graphSignature;
    setNodes((currentNodes) => layoutWorkflowGraphNodes(currentNodes, edges));

    window.requestAnimationFrame(() => {
      void reactFlowInstance.fitView({ padding: 0.16, duration: 0 }).then(() => {
        const nextViewport = reactFlowInstance.getViewport();

        currentViewportRef.current = nextViewport;
        storeGraphViewport(nextViewport);
      });
    });
  }, [nodesInitialized, graphSignature, edges, reactFlowInstance, setNodes]);

  useEffect(() => {
    if (layoutRequestCount === 0) {
      return;
    }

    setNodes((currentNodes) => layoutWorkflowGraphNodes(currentNodes, edges));

    window.requestAnimationFrame(() => {
      void reactFlowInstance.fitView({ padding: 0.16, duration: 420 }).then(() => {
        const nextViewport = reactFlowInstance.getViewport();

        currentViewportRef.current = nextViewport;
        storeGraphViewport(nextViewport);
      });
    });
  }, [layoutRequestCount, edges, reactFlowInstance, setNodes]);

  return (
    <ReactFlow
      nodes={nodes}
      edges={edges}
      onNodesChange={onNodesChange}
      onNodeClick={(_event, node) => onSelectNode(node.id)}
      onEdgesChange={onEdgesChange}
      nodeTypes={graphNodeTypes}
      defaultViewport={currentViewportRef.current}
      onInit={(initializedReactFlowInstance) => {
        if (restoredViewportRef.current) {
          const restoredViewport = restoredViewportRef.current;

          currentViewportRef.current = restoredViewport;
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
            storeGraphViewport(nextViewport);
          });
        });
      }}
      minZoom={0.35}
      maxZoom={1.2}
      nodesConnectable={false}
      edgesReconnectable={false}
      onMove={(_event, viewport: Viewport) => {
        currentViewportRef.current = viewport;
      }}
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
            <button type="button" data-active={config.density === 'compact'} aria-pressed={config.density === 'compact'} onClick={() => onChange({ ...config, density: 'compact' })}>Compact</button>
            <button type="button" data-active={config.density === 'comfortable'} aria-pressed={config.density === 'comfortable'} onClick={() => onChange({ ...config, density: 'comfortable' })}>Comfortable</button>
          </div>
        </section>


        <label className="graph-settings__toggle">
          <input type="checkbox" checked={config.showEdgeLabels} onChange={(event) => onChange({ ...config, showEdgeLabels: event.target.checked })} />
          <span className="graph-settings__switch" aria-hidden="true" />
          <span>
            <strong>Show edge labels</strong>
            <small>Display aggregated relationship labels.</small>
          </span>
        </label>

        <section className="graph-settings__section">
          <span>Edge lines</span>
          <div className="graph-settings__edge-grid">
            <button type="button" data-active={config.edgeType === 'smoothstep'} aria-pressed={config.edgeType === 'smoothstep'} onClick={() => onChange({ ...config, edgeType: 'smoothstep' })}>Smooth step</button>
            <button type="button" data-active={config.edgeType === 'straight'} aria-pressed={config.edgeType === 'straight'} onClick={() => onChange({ ...config, edgeType: 'straight' })}>Straight</button>
            <button type="button" data-active={config.edgeType === 'default'} aria-pressed={config.edgeType === 'default'} onClick={() => onChange({ ...config, edgeType: 'default' })}>Bezier</button>
            <button type="button" data-active={config.edgeType === 'simplebezier'} aria-pressed={config.edgeType === 'simplebezier'} onClick={() => onChange({ ...config, edgeType: 'simplebezier' })}>Curve</button>
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
  const plannedRunCount = data.plannedRunCount;
  const executionSlots = data.executionSlots;
  const outputEntries = data.outputEntries;
  const failureEntry = data.failureEntry;
  const visiblyCollapsed = true;
  const [outputOpen, setOutputOpen] = useState(false);
  const [instructionOpen, setInstructionOpen] = useState(false);
  const [openOutputIndex, setOpenOutputIndex] = useState(0);
  const status = nodeStatus(node, runState, activeRunCount, outputEntries, failureEntry);
  const visibleBindings = node.bindings.filter((binding) => binding.name !== 'instruction' && binding.name !== 'model');
  const showSubtitle = config.density === 'comfortable';
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
      <GraphNodeHandles node={node} collapsed={visiblyCollapsed} showExpandedInstructionHandle={!node.instruction} />
      <button type="button" className="graph-node__header nodrag" aria-label={`Show ${node.label} details`} aria-pressed={data.selected} onClick={() => data.onSelectNode(node.id)}>
        <div className="graph-node__identity">
          <span className="graph-node__icon">{nodeIcon(node)}</span>
          <span className="graph-node__title-block">
            <strong className="graph-node__title">{node.label}</strong>
            {showSubtitle ? <small className="graph-node__subtitle">{nodeSubtitle(node)}</small> : null}
          </span>
        </div>
        <NodeStatusBadge status={status} activeRunCount={activeRunCount} outputEntries={outputEntries} />
        <Search className="graph-node__header-chevron" aria-hidden="true" />
      </button>

      <GraphExecutionStrip node={node} runState={runState} activeRunCount={activeRunCount} plannedRunCount={plannedRunCount} executionSlots={executionSlots} outputEntries={outputEntries} failureEntry={failureEntry} onOpenOutput={openOutput} />
      <GraphFailureNotice failureEntry={failureEntry} />
      {node.instruction ? <GraphInstructionPreview instruction={node.instruction} onOpen={() => setInstructionOpen(true)} /> : null}
      {node.loop_info ? <GraphLoopSummary node={node} config={config} /> : null}
      {visiblyCollapsed ? (
        <p className="graph-node__summary">{nodeSummary(node)}</p>
      ) : (
        <>
          {node.kind !== 'agent' && node.details.length > 0 ? <GraphDetails title={node.kind === 'mcp' ? 'MCP bindings' : 'Details'} details={node.details} collapsible={node.kind === 'mcp'} targetHandleByDetailName={detailTargetHandles(node)} /> : null}
          {visibleBindings.length > 0 ? <GraphBindings bindings={visibleBindings} targetHandleId={node.kind === 'dynamic' && mcpTools.length > 0 ? 'mcp-access' : undefined} /> : null}
          <GraphPorts title="Inputs" ports={node.inputs} fallback={inputPortFallback(node)} config={config} collapsible={inputsCollapsible} targetHandleId={inputPortTargetHandleId(node)} />
          <GraphPorts title="Outputs" ports={node.outputs} config={config} collapsible={outputsCollapsible} defaultOpen={node.kind !== 'agent'} showPortNames={node.kind !== 'agent' && node.kind !== 'output'} sourceHandleId={outputPortSourceHandleId(node)} />
          {outputEntries.length > 0 ? <GraphOutputAction node={node} outputEntries={outputEntries} onOpen={() => openOutput(0)} /> : null}
          {node.kind === 'mcp' && node.tools.length > 0 ? <GraphMcpDefinitions tools={node.tools} config={config} /> : null}
          {(node.kind === 'agent' || node.kind === 'dynamic') && mcpTools.length > 0 ? <GraphMcpAccess tools={mcpTools} config={config} showTargetHandle={node.kind !== 'dynamic'} /> : null}
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
    return (
      <>
        {node.kind === 'agent' && showExpandedInstructionHandle ? <Handle id="instruction" type="target" position={Position.Left} className="graph-node__handle graph-node__handle--instruction" isConnectable={false} /> : null}
      </>
    );
  }

  const hasCollapsedTargetHandle = node.kind !== 'input';
  const hasCollapsedSourceHandle = node.kind !== 'output';

  return (
    <>
      {hasCollapsedTargetHandle ? <span className="graph-node__collapsed-handle graph-node__collapsed-handle--left" aria-hidden="true" /> : null}
      {hasCollapsedSourceHandle ? <span className="graph-node__collapsed-handle graph-node__collapsed-handle--right" aria-hidden="true" /> : null}
      {node.kind === 'model' ? <Handle id="client" type="target" position={Position.Left} className="graph-node__handle graph-node__handle--collapsed graph-node__handle--client" isConnectable={false} /> : null}
      {node.kind === 'compact' ? <Handle id="model" type="target" position={Position.Left} className="graph-node__handle graph-node__handle--collapsed graph-node__handle--model" isConnectable={false} /> : null}
      {node.kind === 'agent' || node.kind === 'compact' ? <Handle id="instruction" type="target" position={Position.Left} className="graph-node__handle graph-node__handle--collapsed graph-node__handle--instruction" isConnectable={false} /> : null}
      {node.kind === 'agent' || node.kind === 'dynamic' ? <Handle id="mcp-access" type="target" position={Position.Left} className="graph-node__handle graph-node__handle--collapsed graph-node__handle--mcp-access" isConnectable={false} /> : null}
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

function GraphExecutionStrip({ node, runState, activeRunCount, plannedRunCount, executionSlots, outputEntries, failureEntry, onOpenOutput }: { node: WorkflowExecutionGraphNode; runState: RunState; activeRunCount: number; plannedRunCount: number; executionSlots: GraphExecutionSlotStatus[]; outputEntries: GraphOutputEntry[]; failureEntry: GraphFailureEntry | null; onOpenOutput: (outputIndex: number) => void }) {
  if (node.kind !== 'agent' || !node.loop_info) {
    return null;
  }

  const completedCount = outputEntries.length;
  const fallbackSlotCount = graphExecutionSlotCount(node, completedCount, activeRunCount, plannedRunCount, failureEntry !== null);
  const fallbackSlots = Array.from({ length: fallbackSlotCount }).map((_, slotIndex) => executionSlotStatus(slotIndex, completedCount, activeRunCount, failureEntry?.status ?? null, runState));
  const slots = executionSlots.length > 0 ? executionSlots : fallbackSlots;

  if (slots.length <= 1) {
    return null;
  }

  const executionSummary = graphExecutionSummary(slots);

  return (
    <section className="graph-node__execution" aria-label="Execution progress">
      <div className="graph-node__execution-summary">
        <span><strong>{executionSummary.completed}</strong> done</span>
        <span><strong>{executionSummary.running}</strong> running</span>
        <span><strong>{executionSummary.waiting}</strong> waiting</span>
        {executionSummary.failed > 0 ? <span><strong>{executionSummary.failed}</strong> failed</span> : null}
        {executionSummary.cancelled > 0 ? <span><strong>{executionSummary.cancelled}</strong> cancelled</span> : null}
      </div>

      {slots.length > graphExecutionStripSlotRenderLimit ? (
        <GraphExecutionStripSummary slots={slots} executionSummary={executionSummary} />
      ) : (
        <div className="graph-node__execution-strip">
          {slots.map((slotStatus, slotIndex) => {
            const outputEntryIndex = outputEntryIndexForSlot(outputEntries, slotIndex);

            return (
              <button
                key={`${node.id}-slot-${slotIndex}`}
                type="button"
                className="nodrag"
                data-status={slotStatus}
                disabled={outputEntryIndex === null}
                onClick={() => (outputEntryIndex !== null ? onOpenOutput(outputEntryIndex) : undefined)}
                aria-label={slotStatus === 'completed' ? `Open ${node.label} output ${slotIndex + 1}` : `${node.label} ${slotStatus}`}
              />
            );
          })}
        </div>
      )}
    </section>
  );
}

function GraphExecutionStripSummary({ slots, executionSummary }: { slots: GraphExecutionSlotStatus[]; executionSummary: GraphExecutionSummary }) {
  return (
    <div className="graph-node__execution-strip-summary" aria-label={`${slots.length} loop iterations`}>
      {Object.entries(executionSummary).map(([slotStatus, slotCount]) => (
        slotCount > 0 ? <span key={slotStatus} data-status={slotStatus} style={{ flexGrow: slotCount }} /> : null
      ))}
    </div>
  );
}

function GraphFailureNotice({ failureEntry }: { failureEntry: GraphFailureEntry | null }) {
  if (!failureEntry) {
    return null;
  }

  return (
    <section className="graph-node__failure" data-status={failureEntry.status} aria-label={failureEntry.title}>
      <strong>{failureEntry.title}</strong>
      <p>{failureEntry.message}</p>
    </section>
  );
}

function graphExecutionSlotCount(node: WorkflowExecutionGraphNode, completedCount: number, activeRunCount: number, plannedRunCount: number, hasFailure: boolean) {
  const loopBinding = node.bindings.find((binding) => binding.name === 'loop');
  const loopCount = loopBinding ? arrayLiteralItemCount(loopBinding.expression) : null;
  const failedCount = hasFailure ? 1 : 0;

  if (node.loop_info && plannedRunCount === 0) {
    return 0;
  }

  if (loopCount !== null) {
    return Math.max(loopCount, plannedRunCount, completedCount + activeRunCount + failedCount);
  }

  return Math.max(plannedRunCount, completedCount + activeRunCount + failedCount, 1);
}

function graphExecutionSummary(slots: GraphExecutionSlotStatus[]): GraphExecutionSummary {
  return slots.reduce<GraphExecutionSummary>((summary, slotStatus) => {
    summary[slotStatus] += 1;

    return summary;
  }, { completed: 0, running: 0, failed: 0, cancelled: 0, waiting: 0, idle: 0 });
}

function outputEntryIndexForSlot(outputEntries: GraphOutputEntry[], slotIndex: number) {
  const indexedOutputEntryIndex = outputEntries.findIndex((outputEntry) => outputEntry.iterationIndex === slotIndex);

  if (indexedOutputEntryIndex >= 0) {
    return indexedOutputEntryIndex;
  }

  if (outputEntries.some((outputEntry) => outputEntry.iterationIndex !== null)) {
    return null;
  }

  return slotIndex < outputEntries.length ? slotIndex : null;
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

function GraphDetails({ title, details, collapsible = false, targetHandleByDetailName = {} }: { title: string; details: WorkflowExecutionGraphNode['details']; collapsible?: boolean; targetHandleByDetailName?: Record<string, string> }) {
  const content = (
    <ul>
      {details.map((detail) => {
        const targetHandleId = targetHandleByDetailName[detail.name];

        return (
          <li key={`${detail.name}:${detail.value}`}>
            {targetHandleId ? <Handle id={targetHandleId} type="target" position={Position.Left} className="graph-node__handle graph-node__handle--detail graph-node__handle--model" isConnectable={false} /> : null}
            <small>{detail.name}</small>
            <code data-secret={detail.secret ? 'true' : 'false'}>{detail.value}</code>
          </li>
        );
      })}
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

function GraphBindings({ bindings, targetHandleId }: { bindings: WorkflowExecutionGraphNode['bindings']; targetHandleId?: string }) {
  return (
    <section className="graph-node__section graph-node__bindings">
      <span className="graph-node__section-label">
        {targetHandleId ? <Handle id={targetHandleId} type="target" position={Position.Left} className="graph-node__handle graph-node__handle--section graph-node__handle--mcp-access" isConnectable={false} /> : null}
        Bindings
      </span>
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
  const [searchQuery, setSearchQuery] = useState('');
  const previousOutputEntryCountRef = useRef(outputEntries.length);
  const selectedOutputEntry = outputEntries[selectedOutputIndex] ?? outputEntries[0];
  const normalizedSearchQuery = searchQuery.trim().toLowerCase();
  const filteredOutputEntries = useMemo(() => {
    const indexedOutputEntries = outputEntries.map((outputEntry, outputIndex) => ({ outputEntry, outputIndex }));

    if (!normalizedSearchQuery) {
      return indexedOutputEntries;
    }

    return indexedOutputEntries.filter(({ outputEntry }) => (
      outputEntry.title.toLowerCase().includes(normalizedSearchQuery)
      || outputEntry.outputJson.toLowerCase().includes(normalizedSearchQuery)
    ));
  }, [normalizedSearchQuery, outputEntries]);

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
          <aside className="graph-output-dialog__sidebar">
            <label className="graph-output-dialog__search">
              <Search />
              <input value={searchQuery} onChange={(event) => setSearchQuery(event.target.value)} placeholder="Search outputs" />
            </label>

            <GraphOutputEntryList
              outputEntries={filteredOutputEntries}
              selectedOutputIndex={selectedOutputIndex}
              label={`${node.label} outputs`}
              onSelectOutput={setSelectedOutputIndex}
            />
          </aside>

          <section className="graph-output-dialog__detail">
            <header className="graph-output-dialog__detail-header">
              <strong>{selectedOutputEntry.title}</strong>
              <small>{outputByteSize(jsonByteSize(selectedOutputEntry.outputJson))}</small>
            </header>
            <JsonCodeEditor value={selectedOutputEntry.outputJson} readOnly ariaLabel={`${node.label} selected output`} className="graph-output-dialog__json" />
          </section>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function GraphOutputEntryList({ outputEntries, selectedOutputIndex, label, onSelectOutput }: { outputEntries: Array<{ outputEntry: GraphOutputEntry; outputIndex: number }>; selectedOutputIndex: number; label: string; onSelectOutput: (outputIndex: number) => void }) {
  const viewportRef = useRef<HTMLDivElement | null>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewportHeight, setViewportHeight] = useState(320);
  const totalHeight = outputEntries.length * graphOutputEntryRowHeight;
  const startIndex = Math.max(0, Math.floor(scrollTop / graphOutputEntryRowHeight) - graphOutputEntryOverscanRows);
  const visibleRowCount = Math.ceil(viewportHeight / graphOutputEntryRowHeight) + graphOutputEntryOverscanRows * 2;
  const endIndex = Math.min(outputEntries.length, startIndex + visibleRowCount);
  const visibleOutputEntries = outputEntries.slice(startIndex, endIndex);

  useLayoutEffect(() => {
    const viewportElement = viewportRef.current;

    if (!viewportElement) {
      return undefined;
    }

    const updateViewportHeight = () => setViewportHeight(viewportElement.clientHeight);
    updateViewportHeight();

    const resizeObserver = new ResizeObserver(updateViewportHeight);
    resizeObserver.observe(viewportElement);

    return () => resizeObserver.disconnect();
  }, []);

  if (outputEntries.length === 0) {
    return <p className="graph-output-dialog__empty">No matching outputs.</p>;
  }

  return (
    <div
      ref={viewportRef}
      className="graph-output-dialog__entries"
      role="tablist"
      aria-label={label}
      onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
    >
      <div className="graph-output-dialog__entries-spacer" style={{ height: totalHeight }}>
        {visibleOutputEntries.map(({ outputEntry, outputIndex }, visibleOutputEntryIndex) => {
          const rowTop = (startIndex + visibleOutputEntryIndex) * graphOutputEntryRowHeight;

          return (
            <button
              key={`${outputEntry.title}-${outputIndex}`}
              type="button"
              className="graph-output-dialog__entry"
              role="tab"
              data-selected={outputIndex === selectedOutputIndex ? 'true' : 'false'}
              aria-selected={outputIndex === selectedOutputIndex}
              onClick={() => onSelectOutput(outputIndex)}
              style={{ transform: `translateY(${rowTop}px)` }}
            >
              <strong>{outputEntry.title}</strong>
              <span>{outputPreview(outputEntry.outputJson)}</span>
              <small>{outputByteSize(jsonByteSize(outputEntry.outputJson))}</small>
            </button>
          );
        })}
      </div>
    </div>
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

function GraphMcpAccess({ tools, config, showTargetHandle = true }: { tools: WorkflowExecutionGraphTool[]; config: GraphConfig; showTargetHandle?: boolean }) {
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
          {showTargetHandle ? <Handle id="mcp-access" type="target" position={Position.Left} className="graph-node__handle graph-node__handle--section graph-node__handle--mcp-access" isConnectable={false} /> : null}
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
    if (!nodeUsesModel(node)) {
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
    if (node.kind !== 'agent' && node.kind !== 'dynamic') {
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

function nodeUsesModel(node: WorkflowExecutionGraphNode) {
  return node.kind === 'agent' || node.kind === 'dynamic' || node.kind === 'compact';
}

function detailTargetHandles(node: WorkflowExecutionGraphNode): Record<string, string> {
  if (node.kind === 'compact') {
    return { model: 'model' };
  }

  return {};
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

function reactFlowNodes(graph: WorkflowExecutionGraph, config: GraphConfig, runState: RunState, activeRunCounts: Map<string, number>, plannedRunCountsByNodeId: Record<string, number>, executionSlotsByNodeId: Record<string, GraphExecutionSlotStatus[]>, outputEntriesByNodeId: Record<string, GraphOutputEntry[]>, failureEntriesByNodeId: Record<string, GraphFailureEntry>, selectedNodeIdentifier: string | null, onSelectNode: (nodeIdentifier: string) => void): WorkflowGraphReactNode[] {
  const graphNodes = graph.nodes;
  const agentNodes = graphNodes.filter((node) => node.kind === 'agent');
  const lastColumn = Math.max(agentNodes.length + 1, 1);

  return graphNodes.map((node) => ({
    id: node.id,
    type: 'workflowGraph',
    position: nodePosition(node, lastColumn, graphNodes),
    data: { node, config, runState, activeRunCount: activeRunCounts.get(node.id) ?? 0, plannedRunCount: plannedRunCountsByNodeId[node.id] ?? 0, executionSlots: executionSlotsByNodeId[node.id] ?? [], outputEntries: outputEntriesByNodeId[node.id] ?? [], failureEntry: failureEntriesByNodeId[node.id] ?? null, selected: node.id === selectedNodeIdentifier, onSelectNode },
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
    currentNode.data.plannedRunCount === incomingNode.data.plannedRunCount &&
    currentNode.data.selected === incomingNode.data.selected &&
    sameGraphExecutionSlots(currentNode.data.executionSlots, incomingNode.data.executionSlots) &&
    sameGraphOutputEntries(currentNode.data.outputEntries, incomingNode.data.outputEntries) &&
    sameGraphFailureEntry(currentNode.data.failureEntry, incomingNode.data.failureEntry)
  );
}

function sameGraphExecutionSlots(currentSlots: GraphExecutionSlotStatus[], incomingSlots: GraphExecutionSlotStatus[]) {
  if (currentSlots.length !== incomingSlots.length) {
    return false;
  }

  return currentSlots.every((currentSlot, slotIndex) => currentSlot === incomingSlots[slotIndex]);
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

function reactFlowEdges(graph: WorkflowExecutionGraph, config: GraphConfig, runState: RunState, activeRunCounts: Map<string, number>, outputEntriesByNodeId: Record<string, GraphOutputEntry[]>, failureEntriesByNodeId: Record<string, GraphFailureEntry>): Edge[] {
  const graphNodesById = new Map(graph.nodes.map((node) => normalizeWorkflowGraphNode(node)).map((node) => [node.id, node]));

  return graph.edges.map((edge) => ({
    id: edge.id,
    source: edge.source,
    target: edge.target,
    sourceHandle: graphEdgeSourceHandle(edge.kind),
    targetHandle: graphEdgeTargetHandle(edge.kind, graphNodesById.get(edge.target)),
    label: config.showEdgeLabels ? edge.label : undefined,
    type: config.edgeType,
    animated: (activeRunCounts.get(edge.target) ?? 0) > 0,
    className: graphEdgeClassName(edge.kind, graphNodesById.get(edge.target), runState, activeRunCounts, outputEntriesByNodeId, failureEntriesByNodeId),
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

function graphEdgeTargetHandle(edgeKind: string, targetNode?: WorkflowExecutionGraphNode) {
  if (edgeKind === 'provider_client') {
    return 'client';
  }

  if (edgeKind === 'model') {
    if (targetNode?.kind === 'compact') {
      return 'model';
    }

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

function inputPortFallback(node: WorkflowExecutionGraphNode) {
  if (node.kind === 'input') {
    return 'External runtime values';
  }

  if (node.kind === 'dynamic') {
    return 'No upstream runtime values';
  }

  if (node.kind === 'compact') {
    return 'No source context';
  }

  return 'No upstream agent output';
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
    inputs: normalizedWorkflowGraphInputs(node),
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

function normalizedWorkflowGraphInputs(node: WorkflowExecutionGraphNode) {
  const inputs = Array.isArray(node.inputs) ? node.inputs : [];

  if (node.kind !== 'compact' || inputs.some((input) => input.name === 'model')) {
    return inputs;
  }

  return [{ name: 'model', schema: { type: 'object', title: 'Language model' } }, ...inputs];
}

function normalizeWorkflowGraphTool(tool: WorkflowExecutionGraphTool): WorkflowExecutionGraphTool {
  return {
    ...tool,
    bindings: Array.isArray(tool.bindings) ? tool.bindings : [],
  };
}

function graphEdgeClassName(edgeKind: string, targetNode: WorkflowExecutionGraphNode | undefined, runState: RunState, activeRunCounts: Map<string, number>, outputEntriesByNodeId: Record<string, GraphOutputEntry[]>, failureEntriesByNodeId: Record<string, GraphFailureEntry>) {
  const targetStatus = targetNode ? nodeStatus(targetNode, runState, activeRunCounts.get(targetNode.id) ?? 0, outputEntriesByNodeId[targetNode.id] ?? [], failureEntriesByNodeId[targetNode.id] ?? null) : 'idle';

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

  if (node.data.node.kind === 'dynamic') {
    return 2;
  }

  if (node.data.node.kind === 'compact') {
    return 3;
  }

  if (node.data.node.kind === 'output') {
    return 5;
  }

  return 4;
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

  if (nodeKind === 'compact') {
    return 4;
  }

  if (nodeKind === 'dynamic') {
    return 5;
  }

  if (nodeKind === 'agent') {
    return 6;
  }

  return 7;
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

  if (node.kind === 'dynamic') {
    return { x: 720, y: 430 + nodeKindIndex(node, nodes) * 260 };
  }

  if (node.kind === 'compact') {
    const executionIndex = node.execution_index ?? 0;

    return { x: (executionIndex + 2) * 360, y: 275 + nodeKindIndex(node, nodes) * 170 };
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

function nodeStatus(node: WorkflowExecutionGraphNode, runState: RunState, activeRunCount: number, outputEntries: GraphOutputEntry[], failureEntry: GraphFailureEntry | null): GraphNodeStatus {
  if (failureEntry) {
    return failureEntry.status;
  }

  if (activeRunCount > 0) {
    return 'running';
  }

  if (outputEntries.length > 0 || (node.kind === 'input' && runState !== 'idle')) {
    return 'completed';
  }

  return 'idle';
}

function executionSlotStatus(slotIndex: number, completedCount: number, activeRunCount: number, terminalStatus: 'failed' | 'cancelled' | null, runState: RunState): GraphExecutionSlotStatus {
  if (slotIndex < completedCount) {
    return 'completed';
  }

  if (slotIndex < completedCount + activeRunCount) {
    return 'running';
  }

  if (terminalStatus && slotIndex === completedCount + activeRunCount) {
    return terminalStatus;
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

  if (node.kind === 'dynamic') {
    return 'Dynamic values';
  }

  if (node.kind === 'compact') {
    return 'Context compaction';
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

  if (node.kind === 'dynamic') {
    return <DatabaseZap />;
  }

  if (node.kind === 'compact') {
    return <RefreshCcw />;
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

  if (node.id === 'dynamic') {
    return '#4f8b7b';
  }

  if (node.id.startsWith('compact:')) {
    return '#8a6a2a';
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
    return unionSchemaLines(schema.anyOf, depth, config);
  }

  if (Array.isArray(schema.oneOf)) {
    return unionSchemaLines(schema.oneOf, depth, config);
  }

  if (schema.type === 'array' && Array.isArray(schema.prefixItems)) {
    return tupleSchemaLines(schema.prefixItems, depth, config);
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

function unionSchemaLines(memberSchemas: unknown[], depth: number, config: GraphConfig): string[] {
  const inlineUnion = `(${memberSchemas.map((memberSchema) => schemaInline(memberSchema, config)).join(' | ')})`;

  if (inlineUnion.length <= schemaInlineLengthLimit && memberSchemas.every((memberSchema) => schemaLines(memberSchema, depth, config).length === 1)) {
    return [inlineUnion];
  }

  const lines = ['('];

  for (const [memberIndex, memberSchema] of memberSchemas.entries()) {
    const memberLines = schemaLines(memberSchema, depth + 1, config);
    const firstLine = memberLines[0] ?? 'unknown';
    const separator = memberIndex < memberSchemas.length - 1 ? ' |' : '';
    const formattedMemberLines = [`${indent(depth + 1)}${firstLine}`, ...memberLines.slice(1)];
    const lastLineIndex = formattedMemberLines.length - 1;

    formattedMemberLines[lastLineIndex] = `${formattedMemberLines[lastLineIndex]}${separator}`;
    lines.push(...formattedMemberLines);
  }

  lines.push(`${indent(depth)})`);

  return lines;
}

function tupleSchemaLines(itemSchemas: unknown[], depth: number, config: GraphConfig): string[] {
  const inlineTuple = `(${itemSchemas.map((itemSchema) => schemaInline(itemSchema, config)).join(', ')})`;

  if (inlineTuple.length <= schemaInlineLengthLimit && itemSchemas.every((itemSchema) => schemaLines(itemSchema, depth, config).length === 1)) {
    return [inlineTuple];
  }

  const lines = ['('];

  for (const [itemIndex, itemSchema] of itemSchemas.entries()) {
    const itemLines = schemaLines(itemSchema, depth + 1, config);
    const firstLine = itemLines[0] ?? 'unknown';
    const separator = itemIndex < itemSchemas.length - 1 ? ',' : '';
    const formattedItemLines = [`${indent(depth + 1)}${firstLine}`, ...itemLines.slice(1)];
    const lastLineIndex = formattedItemLines.length - 1;

    formattedItemLines[lastLineIndex] = `${formattedItemLines[lastLineIndex]}${separator}`;
    lines.push(...formattedItemLines);
  }

  lines.push(`${indent(depth)})`);

  return lines;
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
    const itemLines = schemaLines(itemSchema, depth + 1, config);
    const canInline = itemLines.length === 1 && itemType.length <= schemaInlineLengthLimit;

    if (canInline) {
      return [fixedLength === null ? `[${itemType}]` : `[${itemType}; ${fixedLength}]`];
    }

    const lines = ['['];
    const firstLine = itemLines[0] ?? 'unknown';

    lines.push(`${indent(depth + 1)}${firstLine}`);

    for (const remainingLine of itemLines.slice(1)) {
      lines.push(remainingLine);
    }

    lines.push(fixedLength === null ? `${indent(depth)}]` : `${indent(depth)}; ${fixedLength}]`);

    return lines;
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

function graphRuntimeSummaryFromEvents(events: ExecutorEvent[], workflowOutputJson: string, runState: RunState): GraphRuntimeSummary {
  const pendingStatus: GraphExecutionSlotStatus = runState === 'running' ? 'waiting' : 'idle';
  const activeRunCounts = new Map<string, number>();
  const plannedRunCountsByNodeId: Record<string, number> = {};
  const executionSlotsByNodeId: Record<string, GraphExecutionSlotStatus[]> = {};
  const outputEntriesByNodeId: Record<string, GraphOutputEntry[]> = {};
  const failureEntriesByNodeId: Record<string, GraphFailureEntry> = {};
  const globalNotices: GraphRuntimeNotice[] = [];

  for (const event of events) {
    if (event.kind === ExecutorEventKind.WorkflowPlanned && isRecord(event.data) && Array.isArray(event.data.steps)) {
      for (const plannedStep of event.data.steps) {
        collectPlannedRunCounts(plannedStep, plannedRunCountsByNodeId, executionSlotsByNodeId, pendingStatus);
      }
    }

    if (event.kind === ExecutorEventKind.AgentLoopStarted && event.agent_name && isRecord(event.data) && typeof event.data.iteration_count === 'number') {
      plannedRunCountsByNodeId[event.agent_name] = event.data.iteration_count;
      executionSlotsByNodeId[event.agent_name] = emptyExecutionSlots(event.data.iteration_count, pendingStatus);
    }

    const agentName = graphEventAgentName(event);

    if (agentName) {
      const executionSlots = executionSlotsByNodeId[agentName] ?? [];
      executionSlotsByNodeId[agentName] = executionSlots;

      if (event.kind === ExecutorEventKind.AgentStarted) {
        activeRunCounts.set(agentName, (activeRunCounts.get(agentName) ?? 0) + 1);
        setAgentExecutionSlotStatus(executionSlots, eventIterationIndex(event), 'running', pendingStatus);
      }

      if (event.kind === ExecutorEventKind.AgentCompleted) {
        decrementActiveRunCount(activeRunCounts, agentName);
        setAgentExecutionSlotStatus(executionSlots, eventIterationIndex(event), 'completed', pendingStatus);
        collectGraphOutputEntry(outputEntriesByNodeId, event);
      }

      const terminalStatus = graphEventTerminalStatus(event);

      if (terminalStatus) {
        decrementActiveRunCount(activeRunCounts, agentName);
        setAgentExecutionSlotStatus(executionSlots, eventIterationIndex(event), terminalStatus, pendingStatus);
      }
    }

    const failureNodeIdentifier = collectGraphFailureEntry(failureEntriesByNodeId, event);

    if (event.diagnostic && graphEventHasGlobalNotice(event, failureNodeIdentifier)) {
      globalNotices.push(graphRuntimeNotice(event));
    }
  }

  if (workflowOutputJson.trim()) {
    outputEntriesByNodeId.output = [{ title: 'Workflow result', outputJson: workflowOutputJson, iterationIndex: null }];
  }

  return {
    activeRunCounts: runState === 'running' ? activeRunCounts : new Map<string, number>(),
    plannedRunCountsByNodeId,
    executionSlotsByNodeId,
    outputEntriesByNodeId,
    failureEntriesByNodeId,
    globalNotices,
  };
}

function collectPlannedRunCounts(
  plannedStep: unknown,
  plannedRunCountsByNodeId: Record<string, number>,
  executionSlotsByNodeId: Record<string, GraphExecutionSlotStatus[]>,
  pendingStatus: GraphExecutionSlotStatus,
) {
  if (!isRecord(plannedStep)) {
    return;
  }

  if (Array.isArray(plannedStep.parallel)) {
    for (const parallelStep of plannedStep.parallel) {
      collectPlannedRunCounts(parallelStep, plannedRunCountsByNodeId, executionSlotsByNodeId, pendingStatus);
    }

    return;
  }

  if (plannedStep.type !== 'agent' || typeof plannedStep.agent_name !== 'string') {
    return;
  }

  const plannedRunCount = typeof plannedStep.iteration_count === 'number' ? plannedStep.iteration_count : 1;
  plannedRunCountsByNodeId[plannedStep.agent_name] = plannedRunCount;
  executionSlotsByNodeId[plannedStep.agent_name] = emptyExecutionSlots(plannedRunCount, pendingStatus);
}

function emptyExecutionSlots(slotCount: number, pendingStatus: GraphExecutionSlotStatus) {
  return Array.from({ length: slotCount }).map(() => pendingStatus);
}

function decrementActiveRunCount(activeRunCounts: Map<string, number>, agentName: string) {
  const nextRunCount = Math.max((activeRunCounts.get(agentName) ?? 0) - 1, 0);

  if (nextRunCount === 0) {
    activeRunCounts.delete(agentName);
  } else {
    activeRunCounts.set(agentName, nextRunCount);
  }
}

function setAgentExecutionSlotStatus(slots: GraphExecutionSlotStatus[], iterationIndex: number | null, status: GraphExecutionSlotStatus, pendingStatus: GraphExecutionSlotStatus) {
  if (iterationIndex !== null) {
    while (slots.length <= iterationIndex) {
      slots.push(pendingStatus);
    }

    slots[iterationIndex] = status;

    return;
  }

  const runningSlotIndex = slots.findIndex((slotStatus) => slotStatus === 'running');
  const pendingSlotIndex = slots.findIndex((slotStatus) => slotStatus === 'waiting' || slotStatus === 'idle');
  const slotIndex = runningSlotIndex >= 0 ? runningSlotIndex : pendingSlotIndex;

  if (slotIndex >= 0) {
    slots[slotIndex] = status;

    return;
  }

  slots.push(status);
}

function eventIterationIndex(event: ExecutorEvent) {
  if (!isRecord(event.data) || !('iteration_index' in event.data) || typeof event.data.iteration_index !== 'number') {
    return null;
  }

  return event.data.iteration_index;
}

function collectGraphOutputEntry(outputEntriesByNodeId: Record<string, GraphOutputEntry[]>, event: ExecutorEvent) {
  if (!event.agent_name || !isRecord(event.data) || !('output' in event.data)) {
    return;
  }

  const outputEntries = outputEntriesByNodeId[event.agent_name] ?? [];
  const iterationIndex = eventIterationIndex(event);

  outputEntries.push({
    title: iterationIndex === null ? `Iteration ${outputEntries.length + 1}` : `Iteration ${iterationIndex + 1}`,
    outputJson: JSON.stringify(event.data.output, null, 2),
    iterationIndex,
  });
  outputEntriesByNodeId[event.agent_name] = outputEntries;
}

function graphEventAgentName(event: ExecutorEvent) {
  if (event.agent_name) {
    return event.agent_name;
  }

  const subject = event.diagnostic?.subject;

  if (subject && 'agent_name' in subject && subject.agent_name) {
    return subject.agent_name;
  }

  if (event.kind === ExecutorEventKind.WorkflowFailed && event.message) {
    return agentNameFromFailureMessage(event.message);
  }

  return null;
}

function graphEventTerminalStatus(event: ExecutorEvent): 'failed' | 'cancelled' | null {
  if (
    event.kind === ExecutorEventKind.AgentFailed
    || event.kind === ExecutorEventKind.AgentLoopFailed
    || event.kind === ExecutorEventKind.WorkflowFailed
  ) {
    return event.diagnostic?.code === ExecutorDiagnosticCode.Cancelled ? 'cancelled' : 'failed';
  }

  if (event.kind === ExecutorEventKind.AgentCancelled || event.kind === ExecutorEventKind.AgentLoopCancelled) {
    return 'cancelled';
  }

  return null;
}

function collectGraphFailureEntry(failureEntriesByNodeId: Record<string, GraphFailureEntry>, event: ExecutorEvent) {
  const status = graphEventTerminalStatus(event);
  const nodeIdentifier = status ? graphEventAgentName(event) : null;
  const failureMessage = event.diagnostic?.message ?? event.message ?? failureMessageFromData(event.data);

  if (!nodeIdentifier || !failureMessage || !status) {
    return null;
  }

  failureEntriesByNodeId[nodeIdentifier] = {
    title: status === 'cancelled'
      ? 'Execution cancelled'
      : event.kind === ExecutorEventKind.WorkflowFailed
        ? 'Workflow failed here'
        : 'Execution failed',
    message: failureMessage,
    status,
  };

  return nodeIdentifier;
}

function graphEventHasGlobalNotice(event: ExecutorEvent, failureNodeIdentifier: string | null) {
  return failureNodeIdentifier === null
    || event.kind === ExecutorEventKind.ProviderAttemptFailed
    || event.kind === ExecutorEventKind.CacheDegraded
    || event.kind === ExecutorEventKind.StreamGap
    || event.kind === ExecutorEventKind.WorkflowCancelled;
}

function graphRuntimeNotice(event: ExecutorEvent): GraphRuntimeNotice {
  const diagnostic = event.diagnostic;
  const tone = diagnostic?.code === ExecutorDiagnosticCode.Cancelled
    ? 'cancelled'
    : diagnostic?.code === ExecutorDiagnosticCode.StreamGap
      ? 'gap'
      : diagnostic?.severity === ExecutorDiagnosticSeverity.Warning
        ? 'warning'
        : 'error';

  return {
    title: graphRuntimeNoticeTitle(event),
    message: diagnostic?.message ?? event.message ?? failureMessageFromData(event.data) ?? 'Execution diagnostic',
    tone,
  };
}

function graphRuntimeNoticeTitle(event: ExecutorEvent) {
  if (event.kind === ExecutorEventKind.ProviderAttemptFailed && isRecord(event.data) && typeof event.data.attempt === 'number') {
    return `Provider attempt ${event.data.attempt} failed`;
  }

  if (event.kind === ExecutorEventKind.CacheDegraded) {
    return 'Cache degraded';
  }

  if (event.kind === ExecutorEventKind.StreamGap) {
    return 'Event history gap';
  }

  if (event.kind === ExecutorEventKind.WorkflowCancelled) {
    return 'Workflow cancelled';
  }

  if (event.diagnostic?.subject.type === ExecutorDiagnosticSubjectType.Provider) {
    return 'Provider failure';
  }

  return 'Execution diagnostic';
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

function outputPreview(outputJson: string) {
  return outputJson.replaceAll(/\s+/g, ' ').trim();
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
  const savedConfig = localStorageValue(graphConfigStorageKey);

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
  const savedViewport = localStorageValue(graphViewportStorageKey);

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

function localStorageValue(storageKey: string) {
  try {
    return localStorage.getItem(storageKey);
  } catch (error) {
    console.warn(`Unable to read ${storageKey} from local storage.`, error);

    return null;
  }
}

function storeGraphViewport(viewport: Viewport) {
  try {
    localStorage.setItem(graphViewportStorageKey, JSON.stringify(viewport));
  } catch (error) {
    console.warn('Unable to persist workflow graph viewport.', error);
  }
}

function preserveGraphViewport(reactFlowInstance: ReturnType<typeof useReactFlow>, currentViewportRef: MutableRefObject<Viewport>, preservedViewport: Viewport) {
  // Runtime event renders must not read the viewport back from React Flow here:
  // the library may already have recalculated bounds for changed node internals.
  // The ref is the user's last known viewport, so streaming updates restore that
  // value immediately and again after React has flushed the node/edge changes.
  currentViewportRef.current = preservedViewport;
  void reactFlowInstance.setViewport(preservedViewport, { duration: 0 });

  window.requestAnimationFrame(() => {
    void reactFlowInstance.setViewport(preservedViewport, { duration: 0 });

    window.requestAnimationFrame(() => {
      void reactFlowInstance.setViewport(preservedViewport, { duration: 0 });
    });
  });
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
