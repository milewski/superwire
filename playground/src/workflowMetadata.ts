export interface WorkflowSourceMetadata {
  name: string | null;
  inputJson: string | null;
  secretsJson: string | null;
  source: string;
}

const metadataBoundary = '//------';
const nameMetadataPrefix = '// name:';
const inputMetadataPrefix = '// inputs:';
const secretsMetadataPrefix = '// secrets:';

export function workflowSourceWithMetadata(source: string, name: string, inputJson: string, secretsJson: string): string {
  const sourceWithoutMetadata = workflowSourceWithoutMetadata(source);
  const inputMetadata = compactJsonObject(inputJson);
  const secretsMetadata = compactJsonObject(secretsJson);

  return [
    metadataBoundary,
    `${nameMetadataPrefix} ${name}`,
    `${inputMetadataPrefix} ${inputMetadata}`,
    `${secretsMetadataPrefix} ${secretsMetadata}`,
    metadataBoundary,
    sourceWithoutMetadata,
  ].join('\n');
}

export function workflowSourceWithoutMetadata(source: string): string {
  return parseWorkflowSourceMetadata(source).source;
}

export function parseWorkflowSourceMetadata(source: string): WorkflowSourceMetadata {
  const lines = source.split('\n');
  const firstContentLineIndex = lines.findIndex((line) => line.trim() !== '');

  if (firstContentLineIndex < 0 || lines[firstContentLineIndex]?.trim() !== metadataBoundary) {
    return { name: null, inputJson: null, secretsJson: null, source };
  }

  const closingBoundaryIndex = lines.findIndex((line, lineIndex) => (
    lineIndex > firstContentLineIndex && line.trim() === metadataBoundary
  ));

  if (closingBoundaryIndex < 0) {
    return { name: null, inputJson: null, secretsJson: null, source };
  }

  const metadataLines = lines.slice(firstContentLineIndex + 1, closingBoundaryIndex);
  const name = metadataTextValue(metadataLines, nameMetadataPrefix);
  const inputJson = metadataJsonValue(metadataLines, inputMetadataPrefix);
  const secretsJson = metadataJsonValue(metadataLines, secretsMetadataPrefix);
  const remainingLines = [
    ...lines.slice(0, firstContentLineIndex),
    ...lines.slice(closingBoundaryIndex + 1),
  ];

  return {
    name,
    inputJson,
    secretsJson,
    source: trimSingleLeadingNewline(remainingLines.join('\n')),
  };
}

function metadataTextValue(metadataLines: string[], prefix: string) {
  const metadataLine = metadataLines.find((line) => line.trimStart().startsWith(prefix));
  const rawValue = metadataLine?.trimStart().slice(prefix.length).trim();

  return rawValue || null;
}

function metadataJsonValue(metadataLines: string[], prefix: string) {
  const metadataLine = metadataLines.find((line) => line.trimStart().startsWith(prefix));
  const rawValue = metadataLine?.trimStart().slice(prefix.length).trim();

  if (!rawValue) {
    return null;
  }

  try {
    const parsedValue = JSON.parse(rawValue) as unknown;

    if (isRecord(parsedValue)) {
      return JSON.stringify(parsedValue, null, 2);
    }
  } catch {
    return null;
  }

  return null;
}

function compactJsonObject(jsonText: string) {
  try {
    const parsedValue = JSON.parse(jsonText) as unknown;

    if (isRecord(parsedValue)) {
      return JSON.stringify(parsedValue);
    }
  } catch {
    return '{}';
  }

  return '{}';
}

function trimSingleLeadingNewline(source: string) {
  return source.startsWith('\n') ? source.slice(1) : source;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
