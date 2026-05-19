import '@xyflow/react/dist/style.css';
import { Background, Controls, Handle, MarkerType, MiniMap, Position, ReactFlow, ReactFlowProvider, useEdgesState, useNodesState, type Edge, type Node, type NodeProps, type Viewport } from '@xyflow/react';
import { ChevronDown, GitBranch, RefreshCcw, Settings2 } from 'lucide-react';
import { useEffect, useMemo, useState, type ReactNode } from 'react';
import { Button } from '@/components/ui/button';
import JsonCodeEditor from '@/components/json-code-editor';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import ViewHeader from '@/components/playground/view-header';
import type { ExecutorEvent, GraphState, RunState, WorkflowExecutionGraph, WorkflowExecutionGraphNode, WorkflowExecutionGraphTool } from '@/types';

interface WorkflowGraphViewProps {
  graph: WorkflowExecutionGraph | null;
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
  running: boolean;
  outputEntries: GraphOutputEntry[];
}

interface GraphOutputEntry {
  title: string;
  outputJson: string;
}

type WorkflowGraphReactNode = Node<WorkflowGraphNodeData, 'workflowGraph'>;
type GraphDensity = 'compact' | 'comfortable';
type GraphEdgeType = 'smoothstep' | 'straight' | 'default' | 'simplebezier';

interface GraphConfig {
  density: GraphDensity;
  collapseAll: boolean;
  edgeType: GraphEdgeType;
  showEdgeLabels: boolean;
}

const graphConfigStorageKey = 'superwire.playground.graphConfig.v1';
const graphViewportStorageKey = 'superwire.playground.graphViewport.v1';
const graphNodePositionsStorageKey = 'superwire.playground.graphNodePositions.v1';
const defaultGraphConfig: GraphConfig = { density: 'comfortable', collapseAll: false, edgeType: 'smoothstep', showEdgeLabels: false };

const graphNodeTypes = {
  workflowGraph: WorkflowGraphNodeCard,
};

export default function WorkflowGraphView({ graph, graphState, runState, events, outputJson, message, onRefresh }: WorkflowGraphViewProps) {
  const [config, setConfig] = useState<GraphConfig>(() => restoreGraphConfig());
  const runningAgentNames = runState === 'running' ? activeAgentNames(events) : new Set<string>();
  const outputEntriesByNodeId = useMemo(() => graphOutputEntriesByNodeId(events, outputJson), [events, outputJson]);
  const activeAgentSignature = Array.from(runningAgentNames).sort().join(':');
  const nodes = useMemo(() => (graph ? reactFlowNodes(graph, config, runningAgentNames, outputEntriesByNodeId) : []), [graph, config, activeAgentSignature, outputEntriesByNodeId]);
  const edges = useMemo(() => (graph ? reactFlowEdges(graph, config) : []), [graph, config]);
  const description = graph ? `${graph.nodes.length} nodes, ${graph.edges.length} relationships.` : 'Generate a visual execution plan from the current workflow source.';
  const graphSignature = graph ? graph.nodes.map((node) => node.id).join(':') : 'empty';

  useEffect(() => {
    localStorage.setItem(graphConfigStorageKey, JSON.stringify(config));
  }, [config]);

  return (
    <section className="graph-view">
      <div className="graph-view__header">
        <ViewHeader title="Graph" description={description} />
        <GraphSettingsMenu config={config} graphState={graphState} onChange={setConfig} onRefresh={onRefresh} />
      </div>

      <div className="graph-view__canvas" data-empty={graph ? 'false' : 'true'}>
        {graph ? (
          <div className="graph-view__flow">
            <ReactFlowProvider>
              <GraphCanvas nodes={nodes} edges={edges} graphSignature={graphSignature} />
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

function GraphCanvas({ nodes: incomingNodes, edges: incomingEdges, graphSignature }: { nodes: WorkflowGraphReactNode[]; edges: Edge[]; graphSignature: string }) {
  const restoredViewport = restoreGraphViewport();
  const [nodes, setNodes, onNodesChange] = useNodesState(restoreGraphNodePositions(incomingNodes));
  const [edges, setEdges, onEdgesChange] = useEdgesState(incomingEdges);

  useEffect(() => {
    setNodes((currentNodes) => {
      const currentPositions = new Map(currentNodes.map((node) => [node.id, node.position]));
      const restoredPositions = restoreGraphNodePositionMap();

      return incomingNodes.map((incomingNode) => ({
        ...incomingNode,
        position: currentPositions.get(incomingNode.id) ?? restoredPositions[incomingNode.id] ?? incomingNode.position,
      }));
    });
  }, [graphSignature, incomingNodes, setNodes]);

  useEffect(() => {
    setEdges(incomingEdges);
  }, [incomingEdges, setEdges]);

  useEffect(() => {
    storeGraphNodePositions(nodes);
  }, [nodes]);

  return (
    <ReactFlow
      nodes={nodes}
      edges={edges}
      onNodesChange={onNodesChange}
      onEdgesChange={onEdgesChange}
      nodeTypes={graphNodeTypes}
      defaultViewport={restoredViewport ?? undefined}
      fitView={!restoredViewport}
      minZoom={0.35}
      maxZoom={1.2}
      nodesConnectable={false}
      edgesReconnectable={false}
      onMoveEnd={(_event, viewport: Viewport) => storeGraphViewport(viewport)}
    >
      <Background color="var(--border)" gap={24} />
      <MiniMap pannable zoomable nodeColor={nodeColor} />
      <Controls showInteractive={false} />
    </ReactFlow>
  );
}

function GraphSettingsMenu({ config, graphState, onChange, onRefresh }: { config: GraphConfig; graphState: GraphState; onChange: (config: GraphConfig) => void; onRefresh: () => void }) {
  return (
    <details className="graph-settings">
      <summary className="graph-settings__trigger">
        <Settings2 /> Configure graph
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
  const running = data.running;
  const outputEntries = data.outputEntries;
  const [collapsed, setCollapsed] = useState(false);
  const [outputOpen, setOutputOpen] = useState(false);
  const visiblyCollapsed = config.collapseAll || collapsed;

  return (
    <article className={`graph-node graph-node--${node.kind}`} data-collapsed={visiblyCollapsed ? 'true' : 'false'} data-density={config.density} data-running={running ? 'true' : 'false'}>
      <svg className="graph-node__running-stroke" viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden="true">
        <rect x="1.5" y="1.5" width="97" height="97" rx="6" pathLength="100" />
      </svg>
      <Handle type="target" position={Position.Left} className="graph-node__handle" isConnectable={false} />
      <header className="graph-node__header">
        <div className="graph-node__badges">
          <span className="graph-node__kind">{node.kind}</span>
          {node.execution_index !== null ? <span className="graph-node__index">#{node.execution_index + 1}</span> : null}
        </div>
        <button type="button" className="graph-node__collapse nodrag" aria-expanded={!visiblyCollapsed} onClick={() => setCollapsed((open) => !open)} disabled={config.collapseAll}>
          <ChevronDown />
          <span>{visiblyCollapsed ? 'Expand' : 'Collapse'}</span>
        </button>
      </header>
      <strong className="graph-node__title">{node.label}</strong>
      {node.provider_name || node.model ? <small className="graph-node__meta">{[node.provider_name, node.model].filter(Boolean).join(' / ')}</small> : null}
      {node.loop_info ? <GraphLoopSummary node={node} config={config} /> : null}
      {visiblyCollapsed ? (
        <p className="graph-node__summary">{nodeSummary(node)}</p>
      ) : (
        <>
          <GraphPorts title="Input" ports={node.inputs} fallback={node.kind === 'input' ? 'External runtime values' : 'No upstream agent output'} config={config} />
          <GraphPorts title="Output" ports={node.outputs} config={config} />
          {outputEntries.length > 0 ? <GraphOutputAction node={node} outputEntries={outputEntries} onOpen={() => setOutputOpen(true)} /> : null}
          {node.tools.length > 0 ? <GraphTools tools={node.tools} /> : null}
        </>
      )}
      <Handle type="source" position={Position.Right} className="graph-node__handle" isConnectable={false} />
      {outputEntries.length > 0 ? <GraphOutputDialog node={node} outputEntries={outputEntries} open={outputOpen} onOpenChange={setOutputOpen} /> : null}
    </article>
  );
}

function GraphLoopSummary({ node, config }: { node: WorkflowExecutionGraphNode; config: GraphConfig }) {
  const loopInfo = node.loop_info;

  if (!loopInfo) {
    return null;
  }

  return (
    <section className="graph-node__section graph-node__loop">
      <span>Loop agent</span>
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
        View {node.kind === 'output' ? 'workflow result' : node.loop_info ? `${outputEntries.length} iteration outputs` : 'agent output'}
        <small>{outputByteSize(totalBytes)}</small>
      </button>
    </section>
  );
}

function GraphOutputDialog({ node, outputEntries, open, onOpenChange }: { node: WorkflowExecutionGraphNode; outputEntries: GraphOutputEntry[]; open: boolean; onOpenChange: (open: boolean) => void }) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="graph-output-dialog">
        <DialogHeader>
          <DialogTitle>{node.label} output</DialogTitle>
          <DialogDescription>{outputDescription(node, outputEntries)}</DialogDescription>
        </DialogHeader>
        <div className="graph-output-dialog__entries">
          {outputEntries.map((outputEntry, outputIndex) => (
            <details key={`${outputEntry.title}-${outputIndex}`} className="graph-output-dialog__entry" open={outputIndex === 0}>
              <summary>
                <strong>{outputEntry.title}</strong>
                <small>{outputByteSize(jsonByteSize(outputEntry.outputJson))}</small>
              </summary>
              <JsonCodeEditor value={outputEntry.outputJson} readOnly className="graph-output-dialog__json" />
            </details>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  );
}

function GraphPorts({ title, ports, fallback, config }: { title: string; ports: WorkflowExecutionGraphNode['inputs']; fallback?: string; config: GraphConfig }) {
  return (
    <section className="graph-node__section">
      <span>{title}</span>
      {ports.length > 0 ? (
        <ul>
          {ports.map((port) => (
            <li key={port.name}>
              <code>{port.name}</code>
              <pre className="graph-node__schema">{schemaBlock(port.schema, config)}</pre>
            </li>
          ))}
        </ul>
      ) : (
        <p>{fallback ?? 'No declared fields'}</p>
      )}
    </section>
  );
}

function GraphTools({ tools }: { tools: WorkflowExecutionGraphTool[] }) {
  return (
    <section className="graph-node__section graph-node__tools">
      <span>Tools and MCP</span>
      <ul>
        {tools.map((tool) => (
          <li key={`${tool.kind}:${tool.name}:${tool.server_name ?? ''}:${tool.item_name ?? ''}`}>
            <code>{tool.name}</code>
            <small>{toolLabel(tool)}</small>
          </li>
        ))}
      </ul>
    </section>
  );
}

function reactFlowNodes(graph: WorkflowExecutionGraph, config: GraphConfig, runningAgentNames: Set<string>, outputEntriesByNodeId: Record<string, GraphOutputEntry[]>): WorkflowGraphReactNode[] {
  const agentNodes = graph.nodes.filter((node) => node.kind === 'agent');
  const lastColumn = Math.max(agentNodes.length + 1, 1);

  return graph.nodes.map((node) => ({
    id: node.id,
    type: 'workflowGraph',
    position: nodePosition(node, lastColumn),
    data: { node, config, running: runningAgentNames.has(node.id), outputEntries: outputEntriesByNodeId[node.id] ?? [] },
  }));
}

function reactFlowEdges(graph: WorkflowExecutionGraph, config: GraphConfig): Edge[] {
  const nodeLabels = new Map(graph.nodes.map((node) => [node.id, node.label]));

  return graph.edges.map((edge) => ({
    id: edge.id,
    source: edge.source,
    target: edge.target,
    label: config.showEdgeLabels ? nodeLabels.get(edge.target) ?? edge.target : undefined,
    type: config.edgeType,
    markerEnd: { type: MarkerType.ArrowClosed },
    className: `graph-edge graph-edge--${edge.kind}`,
  }));
}

function nodePosition(node: WorkflowExecutionGraphNode, lastColumn: number) {
  if (node.kind === 'input') {
    return { x: 0, y: 220 };
  }

  if (node.kind === 'output') {
    return { x: lastColumn * 360, y: 220 };
  }

  const executionIndex = node.execution_index ?? 0;
  const verticalLane = executionIndex % 3;

  return {
    x: (executionIndex + 1) * 360,
    y: 40 + verticalLane * 210,
  };
}

function nodeSummary(node: WorkflowExecutionGraphNode) {
  const details = [`${node.inputs.length} input${node.inputs.length === 1 ? '' : 's'}`, `${node.outputs.length} output${node.outputs.length === 1 ? '' : 's'}`];

  if (node.tools.length > 0) {
    details.push(`${node.tools.length} tool${node.tools.length === 1 ? '' : 's'}`);
  }

  return details.join(' | ');
}

function nodeColor(node: Node) {
  if (node.id === 'input') {
    return '#38bdf8';
  }

  if (node.id === 'output') {
    return '#22c55e';
  }

  return '#ff9b32';
}

function toolLabel(tool: WorkflowExecutionGraphTool) {
  const source = [tool.server_name, tool.item_name].filter(Boolean).join(' / ');
  const maxCalls = tool.max_calls === null ? '' : ` max ${tool.max_calls}`;

  return `${tool.kind.replaceAll('_', ' ')}${source ? ` ${source}` : ''}${maxCalls}`;
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

function activeAgentNames(events: ExecutorEvent[]) {
  const runningAgentNames = new Set<string>();

  for (const event of events) {
    if (!event.agent_name) {
      continue;
    }

    if (event.kind === 'agent_started') {
      runningAgentNames.add(event.agent_name);
    }

    if (event.kind === 'agent_completed' || event.kind === 'workflow_failed') {
      runningAgentNames.delete(event.agent_name);
    }
  }

  return runningAgentNames;
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

function restoreGraphNodePositions(nodes: WorkflowGraphReactNode[]) {
  const restoredPositions = restoreGraphNodePositionMap();

  return nodes.map((node) => ({
    ...node,
    position: restoredPositions[node.id] ?? node.position,
  }));
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
