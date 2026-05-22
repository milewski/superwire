import type { WorkflowCodeFragment } from './types';

export interface WorkflowFragmentParseResult {
  fragments: WorkflowCodeFragment[];
  useMarkers: boolean;
}

export interface WorkflowCodeFragmentSourceMap {
  fragment: WorkflowCodeFragment;
  fullStartOffset: number;
  fullEndOffset: number;
  sourceStartOffset: number;
  sourceEndOffset: number;
}

const codeFragmentMarkerPattern = /^\s*\/\/--\s*(.+?)\s*$/;

export function createWorkflowCodeFragment(name: string, source = ''): WorkflowCodeFragment {
  return {
    id: uniqueWorkflowCodeFragmentId(),
    name,
    source,
  };
}

export function parseWorkflowSourceFragments(source: string, defaultName: string): WorkflowFragmentParseResult {
  const lines = source.split('\n');
  const fragments: WorkflowCodeFragment[] = [];
  let currentName = defaultName;
  let currentLines: string[] = [];
  let hasMarkers = false;

  for (const sourceLine of lines) {
    const markerMatch = sourceLine.match(codeFragmentMarkerPattern);

    if (!markerMatch) {
      currentLines.push(sourceLine);

      continue;
    }

    hasMarkers = true;

    if (fragments.length > 0 || currentLines.some((lineText) => lineText.trim() !== '')) {
      fragments.push(createWorkflowCodeFragment(currentName, currentLines.join('\n')));
    }

    currentName = markerMatch[1]?.trim() || nextUntitledFragmentName(fragments.length + 1);
    currentLines = [];
  }

  const currentSource = currentLines.join('\n');

  if (hasMarkers || currentSource.trim() !== '' || fragments.length === 0) {
    fragments.push(createWorkflowCodeFragment(currentName, currentSource));
  }

  return {
    fragments,
    useMarkers: hasMarkers,
  };
}

export function workflowSourceFromCodeFragments(fragments: WorkflowCodeFragment[], useMarkers: boolean): string {
  if (fragments.length === 0) {
    return '';
  }

  if (!useMarkers && fragments.length === 1) {
    return fragments[0]?.source ?? '';
  }

  return fragments
    .map((fragment) => {
      const marker = `//-- ${fragment.name}`;

      if (!fragment.source) {
        return marker;
      }

      return `${marker}\n${fragment.source}`;
    })
    .join('\n');
}

export function workflowCodeFragmentSourceMaps(
  fragments: WorkflowCodeFragment[],
  useMarkers: boolean,
): WorkflowCodeFragmentSourceMap[] {
  const sourceMaps: WorkflowCodeFragmentSourceMap[] = [];
  let currentOffset = 0;

  for (const [fragmentIndex, fragment] of fragments.entries()) {
    const fullStartOffset = currentOffset;
    let sourceStartOffset = currentOffset;

    if (useMarkers || fragments.length > 1) {
      const markerLength = `//-- ${fragment.name}`.length;
      sourceStartOffset += markerLength;
      currentOffset += markerLength;

      if (fragment.source.length > 0) {
        sourceStartOffset += 1;
        currentOffset += 1;
      }
    }

    const sourceEndOffset = sourceStartOffset + fragment.source.length;
    sourceMaps.push({
      fragment,
      fullStartOffset,
      fullEndOffset: sourceEndOffset,
      sourceStartOffset,
      sourceEndOffset,
    });

    currentOffset = sourceEndOffset + (fragmentIndex === fragments.length - 1 ? 0 : 1);
  }

  return sourceMaps;
}

export function sourceMapForFragment(
  fragments: WorkflowCodeFragment[],
  useMarkers: boolean,
  fragmentId: string,
): WorkflowCodeFragmentSourceMap | null {
  return workflowCodeFragmentSourceMaps(fragments, useMarkers).find((sourceMap) => sourceMap.fragment.id === fragmentId) ?? null;
}

export function sourceMapForFullOffset(
  fragments: WorkflowCodeFragment[],
  useMarkers: boolean,
  fullOffset: number,
): WorkflowCodeFragmentSourceMap | null {
  const sourceMaps = workflowCodeFragmentSourceMaps(fragments, useMarkers);

  if (sourceMaps.length === 0) {
    return null;
  }

  return (
    sourceMaps.find((sourceMap) => fullOffset >= sourceMap.sourceStartOffset && fullOffset <= sourceMap.sourceEndOffset)
    ?? sourceMaps.find((sourceMap) => fullOffset >= sourceMap.fullStartOffset && fullOffset <= sourceMap.fullEndOffset)
    ?? sourceMaps[sourceMaps.length - 1]
    ?? null
  );
}

export function sourceContainsCodeFragmentMarkers(source: string): boolean {
  return source.split('\n').some((sourceLine) => codeFragmentMarkerPattern.test(sourceLine));
}

export function uniqueCodeFragmentName(fragments: WorkflowCodeFragment[], baseName: string): string {
  const existingNames = new Set(fragments.map((fragment) => fragment.name));

  if (!existingNames.has(baseName)) {
    return baseName;
  }

  for (let fragmentIndex = 2; fragmentIndex < 1000; fragmentIndex += 1) {
    const candidateName = `${baseName} ${fragmentIndex}`;

    if (!existingNames.has(candidateName)) {
      return candidateName;
    }
  }

  return `${baseName} ${Date.now()}`;
}

export function preserveWorkflowCodeFragmentIdentities(
  nextFragments: WorkflowCodeFragment[],
  previousFragments: WorkflowCodeFragment[],
): WorkflowCodeFragment[] {
  const previousFragmentsByName = new Map<string, WorkflowCodeFragment[]>();

  for (const previousFragment of previousFragments) {
    const matchingFragments = previousFragmentsByName.get(previousFragment.name) ?? [];
    matchingFragments.push(previousFragment);
    previousFragmentsByName.set(previousFragment.name, matchingFragments);
  }

  return nextFragments.map((nextFragment, fragmentIndex) => {
    const matchingFragment = previousFragmentsByName.get(nextFragment.name)?.shift() ?? previousFragments[fragmentIndex];

    if (!matchingFragment) {
      return nextFragment;
    }

    return {
      ...nextFragment,
      id: matchingFragment.id,
    };
  });
}

function nextUntitledFragmentName(fragmentNumber: number) {
  return `Fragment ${fragmentNumber}`;
}

function uniqueWorkflowCodeFragmentId() {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }

  return `fragment-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}
