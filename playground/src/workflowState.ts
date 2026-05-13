import type { WorkflowTab } from './types';

export const exampleWorkflow = `provider openai from openai {
    endpoint: "https://api.openai.com/v1"
    api_key: secrets.openai_api_key
}

model openai_model from openai {
    id: "gpt-4.1-mini"
}

secrets {
    openai_api_key: string
}

input {
    topic: string
    audience: string
}

agent writer {
    model: model.openai_model
    instruction: "Write a concise product update about {{ input.topic }} for {{ input.audience }}."
    output {
        title: string
        summary: string
        bullets: [string; 3]
    }
}

output {
    title: agent.writer.title
    summary: agent.writer.summary
    bullets: agent.writer.bullets
}`;

export function uniqueId() {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }

  return `id-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

export function createWorkflowTab(name: string): WorkflowTab {
  return {
    id: uniqueId(),
    name,
    source: exampleWorkflow,
    inputJson: JSON.stringify(
      {
        topic: 'agent workflow observability',
        audience: 'product engineers',
      },
      null,
      2,
    ),
    secretsJson: JSON.stringify(
      {
        openai_api_key: 'sk-...',
      },
      null,
      2,
    ),
    validationState: 'idle',
    runState: 'idle',
    message: 'Ready.',
    outputJson: '',
    eventLog: [],
    updatedAt: Date.now(),
  };
}

export function parseJsonObject(source: string, label: string): Record<string, unknown> {
  const trimmedSource = source.trim();

  if (!trimmedSource) {
    return {};
  }

  const parsedValue = JSON.parse(trimmedSource) as unknown;

  if (!isJsonObject(parsedValue)) {
    throw new Error(`${label} must be a JSON object.`);
  }

  return parsedValue;
}

export function normalizeWorkflowTab(tab: unknown): WorkflowTab {
  const fallbackTab = createWorkflowTab('Launch brief');

  if (!isJsonObject(tab)) {
    return fallbackTab;
  }

  return {
    ...fallbackTab,
    ...tab,
    inputJson: typeof tab.inputJson === 'string' ? tab.inputJson : JSON.stringify(fieldsToObject(tab.inputFields), null, 2),
    secretsJson: typeof tab.secretsJson === 'string' ? tab.secretsJson : JSON.stringify(fieldsToObject(tab.secretFields), null, 2),
    eventLog: Array.isArray(tab.eventLog) ? tab.eventLog : [],
  };
}

function fieldsToObject(value: unknown): Record<string, unknown> {
  if (!Array.isArray(value)) {
    return {};
  }

  const result: Record<string, unknown> = {};

  for (const field of value) {
    if (!isJsonObject(field) || typeof field.name !== 'string') {
      continue;
    }

    const fieldName = field.name.trim();

    if (!fieldName) {
      continue;
    }

    result[fieldName] = parseLegacyFieldValue(field);
  }

  return result;
}

function parseLegacyFieldValue(field: Record<string, unknown>): unknown {
  const fieldValue = typeof field.value === 'string' ? field.value : '';

  if (field.kind === 'number') {
    return Number(fieldValue);
  }

  if (field.kind === 'boolean') {
    return fieldValue === 'true';
  }

  if (field.kind === 'json') {
    return JSON.parse(fieldValue || 'null') as unknown;
  }

  return fieldValue;
}

function isJsonObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
