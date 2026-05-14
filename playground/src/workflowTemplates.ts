import linearChainSource from './workflow-templates/linear-chain.wire?raw';
import minimumSource from './workflow-templates/minimum.wire?raw';
import parallelAgentsSource from './workflow-templates/parallel-agents.wire?raw';
import secretsSource from './workflow-templates/secrets.wire?raw';

type JsonObject = Record<string, unknown>;

export type WorkflowTemplate = {
  id: string;
  name: string;
  description: string;
  source: string;
  input: JsonObject;
  secrets: JsonObject;
};

export const workflowTemplates: WorkflowTemplate[] = [
  {
    id: 'minimum',
    name: 'Minimum workflow',
    description: 'Smallest valid provider-model-agent-output flow.',
    source: minimumSource,
    input: {},
    secrets: {},
  },
  {
    id: 'linear-chain',
    name: 'Linear chain',
    description: 'One agent feeds the next agent output.',
    source: linearChainSource,
    input: {
      topic: 'Summarize this week in AI tooling',
    },
    secrets: {},
  },
  {
    id: 'parallel-agents',
    name: 'Parallel agents',
    description: 'Run independent agents and merge output fields.',
    source: parallelAgentsSource,
    input: {
      product_name: 'Superwire Playground',
    },
    secrets: {},
  },
  {
    id: 'secrets',
    name: 'Secrets setup',
    description: 'Bind provider api_key from secrets object.',
    source: secretsSource,
    input: {},
    secrets: {
      api_key: 'test-api-key',
    },
  },
];
