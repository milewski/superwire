import '@xyflow/react/dist/style.css';
import { Background, Controls, Handle, MarkerType, MiniMap, Position, ReactFlow, ReactFlowProvider, type Edge, type Node, type NodeProps } from '@xyflow/react';
import { GitBranch, RefreshCcw } from 'lucide-react';
import { Button } from '@/components/ui/button';
import ViewHeader from '@/components/playground/view-header';
import type { GraphState, WorkflowExecutionGraph, WorkflowExecutionGraphNode, WorkflowExecutionGraphTool } from '@/types';

interface WorkflowGraphViewProps {
  graph: WorkflowExecutionGraph | null;
  graphState: GraphState;
  message: string;
  onRefresh: () => void;
}

interface WorkflowGraphNodeData extends Record<string, unknown> {
  node: WorkflowExecutionGraphNode;
}

type WorkflowGraphReactNode = Node<WorkflowGraphNodeData, 'workflowGraph'>;

const graphNodeTypes = {
  workflowGraph: WorkflowGraphNodeCard,
};

export default function WorkflowGraphView({ graph, graphState, message, onRefresh }: WorkflowGraphViewProps) {
  const nodes = graph ? reactFlowNodes(graph) : [];
  const edges = graph ? reactFlowEdges(graph) : [];
  const description = graph ? `${graph.nodes.length} nodes, ${graph.edges.length} relationships.` : 'Generate a visual execution plan from the current workflow source.';

  return (
    <section className="graph-view">
      <div className="graph-view__header">
        <ViewHeader title="Graph" description={description} />
        <Button variant="outline" size="lg" className="graph-view__button" onClick={onRefresh} disabled={graphState === 'loading'}>
          <RefreshCcw className={graphState === 'loading' ? 'animate-spin' : ''} /> Refresh graph
        </Button>
      </div>

      <div className="graph-view__canvas" data-empty={graph ? 'false' : 'true'}>
        {graph ? (
          <div className="graph-view__flow">
            <ReactFlowProvider>
              <ReactFlow nodes={nodes} edges={edges} nodeTypes={graphNodeTypes} fitView minZoom={0.35} maxZoom={1.2} nodesDraggable={false}>
                <Background color="var(--border)" gap={24} />
                <MiniMap pannable zoomable nodeColor={nodeColor} />
                <Controls showInteractive={false} />
              </ReactFlow>
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

function WorkflowGraphNodeCard({ data }: NodeProps<WorkflowGraphReactNode>) {
  const node = data.node;

  return (
    <article className={`graph-node graph-node--${node.kind}`}>
      <Handle type="target" position={Position.Left} className="graph-node__handle" />
      <header className="graph-node__header">
        <span className="graph-node__kind">{node.kind}</span>
        {node.execution_index !== null ? <span className="graph-node__index">#{node.execution_index + 1}</span> : null}
      </header>
      <strong className="graph-node__title">{node.label}</strong>
      {node.provider_name || node.model ? <small className="graph-node__meta">{[node.provider_name, node.model].filter(Boolean).join(' / ')}</small> : null}
      <GraphPorts title="Input" ports={node.inputs} fallback={node.kind === 'input' ? 'External runtime values' : 'No upstream agent output'} />
      <GraphPorts title="Output" ports={node.outputs} />
      {node.tools.length > 0 ? <GraphTools tools={node.tools} /> : null}
      <Handle type="source" position={Position.Right} className="graph-node__handle" />
    </article>
  );
}

function GraphPorts({ title, ports, fallback }: { title: string; ports: WorkflowExecutionGraphNode['inputs']; fallback?: string }) {
  return (
    <section className="graph-node__section">
      <span>{title}</span>
      {ports.length > 0 ? (
        <ul>
          {ports.map((port) => (
            <li key={port.name}>
              <code>{port.name}</code>
              <small>{schemaSummary(port.schema)}</small>
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

function reactFlowNodes(graph: WorkflowExecutionGraph): WorkflowGraphReactNode[] {
  const agentNodes = graph.nodes.filter((node) => node.kind === 'agent');
  const lastColumn = Math.max(agentNodes.length + 1, 1);

  return graph.nodes.map((node) => ({
    id: node.id,
    type: 'workflowGraph',
    position: nodePosition(node, lastColumn),
    data: { node },
  }));
}

function reactFlowEdges(graph: WorkflowExecutionGraph): Edge[] {
  return graph.edges.map((edge) => ({
    id: edge.id,
    source: edge.source,
    target: edge.target,
    label: edge.label,
    type: 'smoothstep',
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

function schemaSummary(schema: unknown) {
  if (!isRecord(schema)) {
    return 'unknown schema';
  }

  if (isRecord(schema.properties)) {
    const fields = Object.keys(schema.properties);

    if (fields.length > 0) {
      return `object { ${fields.slice(0, 4).join(', ')}${fields.length > 4 ? ', ...' : ''} }`;
    }
  }

  if (typeof schema.type === 'string') {
    return schema.type;
  }

  if (Array.isArray(schema.anyOf)) {
    return 'union';
  }

  return 'schema';
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
