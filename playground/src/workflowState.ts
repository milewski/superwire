import type { RuntimeField, RuntimeFieldKind, WorkflowTab } from './types';

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

export function createField(name = '', value = '', kind: RuntimeFieldKind = 'string'): RuntimeField {
  return {
    id: uniqueId(),
    name,
    value,
    kind,
  };
}

export function createWorkflowTab(name: string): WorkflowTab {
  return {
    id: uniqueId(),
    name,
    source: exampleWorkflow,
    inputFields: [createField('topic', 'agent workflow observability', 'string'), createField('audience', 'product engineers', 'string')],
    secretFields: [createField('openai_api_key', 'sk-...', 'string')],
    validationState: 'idle',
    runState: 'idle',
    message: 'Ready.',
    outputJson: '',
    eventLog: [],
    updatedAt: Date.now(),
  };
}

export function parseFieldValue(field: RuntimeField, label: string): unknown {
  if (field.kind === 'string') {
    return field.value;
  }

  if (field.kind === 'number') {
    const numericValue = Number(field.value);

    if (!Number.isFinite(numericValue)) {
      throw new Error(`${label}.${field.name} must be a valid number.`);
    }

    return numericValue;
  }

  if (field.kind === 'boolean') {
    return field.value === 'true';
  }

  try {
    return JSON.parse(field.value || 'null');
  } catch (error) {
    throw new Error(`${label}.${field.name} must be valid JSON: ${error instanceof Error ? error.message : String(error)}`);
  }
}

export function fieldsToObject(fields: RuntimeField[], label: string) {
  const result: Record<string, unknown> = {};

  for (const field of fields) {
    const fieldName = field.name.trim();

    if (!fieldName) {
      continue;
    }

    result[fieldName] = parseFieldValue(field, label);
  }

  return result;
}
