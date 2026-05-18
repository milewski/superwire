import { ArrowRight, Braces, Copy, FileText, Pencil, Play, Plus, RefreshCcw, Sun, Trash2, Workflow } from 'lucide-react';
import { motion } from 'motion/react';
import { memo, type CSSProperties, useEffect, useMemo, useRef, useState } from 'react';
import frameUrl from '../../frame.webp';
import logoUrl from '../../../docs/public/logo-horizontal.svg';

const documentationUrl = 'https://docs.superwire.dev';
const githubUrl = 'https://github.com/milewski/superwire';

type CodeSegmentColor = 'keyword' | 'number' | 'plain' | 'property' | 'reference' | 'string' | 'type';

type CodeSegment = {
  text: string;
  color?: CodeSegmentColor;
};

type EditorTransformPoint = {
  coordinateX: number;
  coordinateY: number;
};

type EditorCornerName = 'topLeft' | 'topRight' | 'bottomRight' | 'bottomLeft';

type EditorCorner = {
  name: EditorCornerName;
  label: string;
  point: EditorTransformPoint;
};

type CircuitFramePath = {
  path: string;
  duration: number;
  delay: number;
};

const codeLines: CodeSegment[][] = [
  [{ text: 'provider', color: 'keyword' }, { text: ' openai ', color: 'plain' }, { text: 'from', color: 'keyword' }, { text: ' openai {' }],
  [{ text: '  endpoint: ', color: 'property' }, { text: '"https://ollama.com/v1"', color: 'string' }],
  [{ text: '  api_key: ', color: 'property' }, { text: '"*********"', color: 'string' }],
  [{ text: '}' }],
  [],
  [{ text: 'model', color: 'keyword' }, { text: ' openai_model ', color: 'plain' }, { text: 'from', color: 'keyword' }, { text: ' openai {' }],
  [{ text: '  id: ', color: 'property' }, { text: '"big-pickle"', color: 'string' }],
  [{ text: '}' }],
  [],
  [{ text: 'mcp', color: 'keyword' }, { text: ' example {' }],
  [{ text: '  endpoint: ', color: 'property' }, { text: '"https://superwire.dev/mcp/hello-world"', color: 'string' }],
  [{ text: '  headers {' }],
  [{ text: '    Accept: ', color: 'property' }, { text: '"application/json"', color: 'string' }],
  [{ text: '  }' }],
  [{ text: '}' }],
  [],
  [{ text: 'from', color: 'keyword' }, { text: ' mcp.example {' }],
  [{ text: '  bindings {' }],
  [{ text: '    project_id: ', color: 'property' }, { text: '14', color: 'number' }],
  [{ text: '    task_id: ', color: 'property' }, { text: '109', color: 'number' }],
  [{ text: '  }' }],
  [{ text: '}' }],
  [],
  [{ text: 'prompt', color: 'keyword' }, { text: ' dynamic_summary_prompt {' }],
  [{ text: '  bindings {' }],
  [{ text: '    project_id: ', color: 'property' }, { text: '14', color: 'number' }],
  [{ text: '    type: ', color: 'property' }, { text: '"task"', color: 'string' }],
  [{ text: '    type_id: ', color: 'property' }, { text: '109', color: 'number' }],
  [{ text: '  }' }],
  [{ text: '}' }],
  [],
  [{ text: 'tool', color: 'keyword' }, { text: ' list_all_participants_who_has_answered_given_task' }],
  [{ text: 'tool', color: 'keyword' }, { text: ' fetch_participant_answer' }],
  [],
  [{ text: 'agent', color: 'keyword' }, { text: ' greeting {' }],
  [{ text: '  model: ', color: 'property' }, { text: 'model.openai_model', color: 'reference' }],
  [{ text: '  uses: [', color: 'property' }, { text: 'tool', color: 'keyword' }, { text: '.list_all_participants_who_has_answered_given_task, ', color: 'plain' }, { text: 'prompt', color: 'keyword' }, { text: '.dynamic_summary_prompt]' }],
  [],
  [{ text: '  instruction: ', color: 'property' }, { text: '"""', color: 'string' }],
  [{ text: '    call the ', color: 'string' }, { text: 'prompt', color: 'keyword' }, { text: ' to figure out extra instructions user my request', color: 'string' }],
  [{ text: '    Please analyze all tasks of the participants and provide me a summary', color: 'string' }],
  [{ text: '  """', color: 'string' }],
  [],
  [{ text: '  output', color: 'keyword' }, { text: ' {' }],
  [{ text: '    summary: ', color: 'property' }, { text: 'string', color: 'type' }],
  [{ text: '  }' }],
  [{ text: '}' }],
  [],
  [{ text: 'output', color: 'keyword' }, { text: ' {' }],
  [{ text: '  greeting: ', color: 'property' }, { text: 'agent', color: 'keyword' }, { text: '.greeting.summary' }],
  [{ text: '}' }],
];

const colorClassNames = {
  keyword: 'text-[#ff7b00]',
  number: 'text-[#ffd28b]',
  plain: 'text-[#d7d7d7]',
  property: 'text-[#7bb7ff]',
  reference: 'text-[#8ce6b0]',
  string: 'text-[#94e5b6]',
  type: 'text-[#a8c7ff]',
};

const calibratedEditorMatrix = [
  0.765954, -0.0400053, 0, -9.31e-05, -0.0642622, 0.844449, 0, -2.24e-05, 0, 0, 1, 0, 87.3246, 61.0378, 0, 1
];

const editorSourceSize = 1000;
const editorCalibrationDragMargin = 500;

const editorSourceCorners: EditorTransformPoint[] = [
  { coordinateX: 0, coordinateY: 0 },
  { coordinateX: editorSourceSize, coordinateY: 0 },
  { coordinateX: editorSourceSize, coordinateY: editorSourceSize },
  { coordinateX: 0, coordinateY: editorSourceSize },
];

const circuitBoardWidth = 1280;
const circuitBoardHeight = 760;

const circuitFramePaths: CircuitFramePath[] = [
  { path: 'M0 228 H178 C212 228 212 192 246 192 H420 C456 192 456 134 492 134 V44', duration: 8.8, delay: -1.1 },
  { path: 'M0 316 H248 C286 316 286 372 324 372 H548 C584 372 584 420 620 420 H738', duration: 9.4, delay: -4.2 },
  { path: 'M0 432 H196 C232 432 232 490 268 490 H414 C450 490 450 548 486 548 H736', duration: 8.2, delay: -2.8 },
  { path: 'M578 0 V86 C578 126 536 126 536 166 V334 C536 374 580 374 580 414 V760', duration: 10.4, delay: -6.2 },
  { path: 'M688 72 H930 C968 72 968 34 1006 34 H1210 C1246 34 1246 78 1280 78', duration: 11.2, delay: -7.1 },
  { path: 'M814 694 H1004 C1042 694 1042 736 1080 736 H1198 C1236 736 1236 704 1280 704', duration: 9.8, delay: -5.6 },
  { path: 'M1280 364 H1188 C1152 364 1152 426 1116 426 H1052 C1016 426 1016 472 980 472 H872', duration: 8.6, delay: -3.4 },
  { path: 'M1280 556 H1216 C1180 556 1180 612 1144 612 H1032 C996 612 996 654 960 654 H784', duration: 9.6, delay: -0.6 },
];

const circuitNodes = [
  { coordinateX: 178, coordinateY: 228, radius: 4.8, delay: 0.1 },
  { coordinateX: 492, coordinateY: 134, radius: 5.4, delay: 0.45 },
  { coordinateX: 248, coordinateY: 316, radius: 4.4, delay: 1.05 },
  { coordinateX: 486, coordinateY: 548, radius: 4.6, delay: 0.65 },
  { coordinateX: 1006, coordinateY: 34, radius: 3.8, delay: 0.8 },
  { coordinateX: 1188, coordinateY: 364, radius: 4.2, delay: 1.3 },
  { coordinateX: 1144, coordinateY: 612, radius: 4.2, delay: 0.95 },
  { coordinateX: 1080, coordinateY: 736, radius: 4.8, delay: 1.55 },
];

function applyMatrixToPoint(matrixValues: number[], point: EditorTransformPoint) {
  const divisor = matrixValues[3] * point.coordinateX + matrixValues[7] * point.coordinateY + matrixValues[15];

  return {
    coordinateX: (matrixValues[0] * point.coordinateX + matrixValues[4] * point.coordinateY + matrixValues[12]) / divisor,
    coordinateY: (matrixValues[1] * point.coordinateX + matrixValues[5] * point.coordinateY + matrixValues[13]) / divisor,
  };
}

function createInitialEditorCorners(): EditorCorner[] {
  const labels: Array<{ name: EditorCornerName; label: string }> = [
    { name: 'topLeft', label: 'TL' },
    { name: 'topRight', label: 'TR' },
    { name: 'bottomRight', label: 'BR' },
    { name: 'bottomLeft', label: 'BL' },
  ];

  return labels.map((cornerLabel, cornerIndex) => ({
    ...cornerLabel,
    point: applyMatrixToPoint(calibratedEditorMatrix, editorSourceCorners[cornerIndex]),
  }));
}

function solveLinearSystem(matrixRows: number[][], vectorValues: number[]) {
  const rowCount = matrixRows.length;
  const augmentedMatrix = matrixRows.map((matrixRow, rowIndex) => [...matrixRow, vectorValues[rowIndex]]);

  for (let pivotIndex = 0; pivotIndex < rowCount; pivotIndex += 1) {
    let bestRowIndex = pivotIndex;

    for (let rowIndex = pivotIndex + 1; rowIndex < rowCount; rowIndex += 1) {
      if (Math.abs(augmentedMatrix[rowIndex][pivotIndex]) > Math.abs(augmentedMatrix[bestRowIndex][pivotIndex])) {
        bestRowIndex = rowIndex;
      }
    }

    [augmentedMatrix[pivotIndex], augmentedMatrix[bestRowIndex]] = [augmentedMatrix[bestRowIndex], augmentedMatrix[pivotIndex]];

    const pivotValue = augmentedMatrix[pivotIndex][pivotIndex];

    if (Math.abs(pivotValue) < 1e-10) {
      return calibratedEditorMatrix;
    }

    for (let columnIndex = pivotIndex; columnIndex <= rowCount; columnIndex += 1) {
      augmentedMatrix[pivotIndex][columnIndex] /= pivotValue;
    }

    for (let rowIndex = 0; rowIndex < rowCount; rowIndex += 1) {
      if (rowIndex === pivotIndex) {
        continue;
      }

      const rowFactor = augmentedMatrix[rowIndex][pivotIndex];

      for (let columnIndex = pivotIndex; columnIndex <= rowCount; columnIndex += 1) {
        augmentedMatrix[rowIndex][columnIndex] -= rowFactor * augmentedMatrix[pivotIndex][columnIndex];
      }
    }
  }

  return augmentedMatrix.map((matrixRow) => matrixRow[rowCount]);
}

function getEditorTransformMatrix(sourcePoints: EditorTransformPoint[], targetPoints: EditorTransformPoint[]) {
  const matrixRows: number[][] = [];
  const vectorValues: number[] = [];

  sourcePoints.forEach((sourcePoint, sourcePointIndex) => {
    const targetPoint = targetPoints[sourcePointIndex];

    matrixRows.push([
      sourcePoint.coordinateX,
      sourcePoint.coordinateY,
      1,
      0,
      0,
      0,
      -sourcePoint.coordinateX * targetPoint.coordinateX,
      -sourcePoint.coordinateY * targetPoint.coordinateX,
    ]);
    matrixRows.push([
      0,
      0,
      0,
      sourcePoint.coordinateX,
      sourcePoint.coordinateY,
      1,
      -sourcePoint.coordinateX * targetPoint.coordinateY,
      -sourcePoint.coordinateY * targetPoint.coordinateY,
    ]);
    vectorValues.push(targetPoint.coordinateX, targetPoint.coordinateY);
  });

  const solutionValues = solveLinearSystem(matrixRows, vectorValues);
  const homographyMatrix = [
    [solutionValues[0], solutionValues[1], 0, solutionValues[2]],
    [solutionValues[3], solutionValues[4], 0, solutionValues[5]],
    [0, 0, 1, 0],
    [solutionValues[6], solutionValues[7], 0, 1],
  ];

  return [
    homographyMatrix[0][0],
    homographyMatrix[1][0],
    homographyMatrix[2][0],
    homographyMatrix[3][0],
    homographyMatrix[0][1],
    homographyMatrix[1][1],
    homographyMatrix[2][1],
    homographyMatrix[3][1],
    homographyMatrix[0][2],
    homographyMatrix[1][2],
    homographyMatrix[2][2],
    homographyMatrix[3][2],
    homographyMatrix[0][3],
    homographyMatrix[1][3],
    homographyMatrix[2][3],
    homographyMatrix[3][3],
  ];
}

const CircuitLines = memo(function CircuitLines() {
  return (
    <div aria-hidden="true" className="circuit-board">
      <div className="circuit-board__tile circuit-board__tile--back" />
      <div className="circuit-board__tile circuit-board__tile--front" />

      <svg className="circuit-board__traces" viewBox={`0 0 ${circuitBoardWidth} ${circuitBoardHeight}`} preserveAspectRatio="xMidYMid slice">
        <g className="circuit-board__trace-group">
          {circuitFramePaths.map((circuitFramePath) => (
            <path
              className="circuit-board__trace"
              d={circuitFramePath.path}
              fill="none"
              key={circuitFramePath.path}
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          ))}
        </g>

        <g className="circuit-board__glint-group">
          {circuitFramePaths.map((circuitFramePath) => {
            const circuitGlintStyle = {
              '--trace-delay': `${circuitFramePath.delay}s`,
              '--trace-duration': `${circuitFramePath.duration}s`,
            } as CSSProperties;

            return (
              <g key={`${circuitFramePath.path}-glint`} style={circuitGlintStyle}>
                <path className="circuit-board__glint circuit-board__glint--halo" d={circuitFramePath.path} pathLength="100" />
                <path className="circuit-board__glint circuit-board__glint--core" d={circuitFramePath.path} pathLength="100" />
              </g>
            );
          })}
        </g>
      </svg>

      {circuitNodes.map((circuitNode) => {
        const circuitNodeStyle = {
          '--node-delay': `${circuitNode.delay}s`,
          '--node-size': `${circuitNode.radius * 5.2}px`,
          left: `${(circuitNode.coordinateX / circuitBoardWidth) * 100}%`,
          top: `${(circuitNode.coordinateY / circuitBoardHeight) * 100}%`,
        } as CSSProperties;

        return <span className="circuit-board__node" key={`${circuitNode.coordinateX}-${circuitNode.coordinateY}`} style={circuitNodeStyle} />;
      })}
    </div>
  );
});

function EditorWindow() {
  const editorPerspectiveElementRef = useRef<HTMLDivElement | null>(null);
  const [editorCoordinateTransform, setEditorCoordinateTransform] = useState('scale(1)');
  const [isCalibrationEnabled, setIsCalibrationEnabled] = useState(false);
  const [activeCornerName, setActiveCornerName] = useState<EditorCornerName | null>(null);
  const [editorCorners, setEditorCorners] = useState(createInitialEditorCorners);
  const editorMatrixValues = useMemo(
    () => getEditorTransformMatrix(editorSourceCorners, editorCorners.map((editorCorner) => editorCorner.point)),
    [editorCorners],
  );
  const editorPanelTransform = useMemo(() => `matrix3d(${editorMatrixValues.join(',')})`, [editorMatrixValues]);

  useEffect(() => {
    const editorPerspectiveElement = editorPerspectiveElementRef.current;

    if (!editorPerspectiveElement) {
      return undefined;
    }

    const updateEditorPanelTransform = (width = editorPerspectiveElement.clientWidth, height = editorPerspectiveElement.clientHeight) => {
      if (width === 0 || height === 0) {
        return;
      }

      setEditorCoordinateTransform(`scale(${width / editorSourceSize}, ${height / editorSourceSize})`);
    };

    updateEditorPanelTransform();

    const resizeObserver = new ResizeObserver((resizeObserverEntries) => {
      const resizeObserverEntry = resizeObserverEntries[0];
      const resizeObserverSize = Array.isArray(resizeObserverEntry.contentBoxSize)
        ? resizeObserverEntry.contentBoxSize[0]
        : resizeObserverEntry.contentBoxSize;

      updateEditorPanelTransform(
        resizeObserverSize?.inlineSize ?? resizeObserverEntry.contentRect.width,
        resizeObserverSize?.blockSize ?? resizeObserverEntry.contentRect.height,
      );
    });
    resizeObserver.observe(editorPerspectiveElement);

    return () => resizeObserver.disconnect();
  }, []);

  useEffect(() => {
    function handleKeyDown(keyboardEvent: KeyboardEvent) {
      if (!(keyboardEvent.ctrlKey || keyboardEvent.metaKey) || keyboardEvent.key.toLowerCase() !== 'k') {
        return;
      }

      keyboardEvent.preventDefault();
      setActiveCornerName(null);
      setIsCalibrationEnabled((currentValue) => !currentValue);
    }

    window.addEventListener('keydown', handleKeyDown);

    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  useEffect(() => {
    function handlePointerMove(pointerEvent: PointerEvent) {
      const editorPerspectiveElement = editorPerspectiveElementRef.current;

      if (!activeCornerName || !editorPerspectiveElement) {
        return;
      }

      const editorPerspectiveRect = editorPerspectiveElement.getBoundingClientRect();
      const minimumDragCoordinate = -editorCalibrationDragMargin;
      const maximumDragCoordinate = editorSourceSize + editorCalibrationDragMargin;
      const nextCoordinateX = ((pointerEvent.clientX - editorPerspectiveRect.left) / editorPerspectiveRect.width) * editorSourceSize;
      const nextCoordinateY = ((pointerEvent.clientY - editorPerspectiveRect.top) / editorPerspectiveRect.height) * editorSourceSize;
      const nextPoint = {
        coordinateX: Math.min(maximumDragCoordinate, Math.max(minimumDragCoordinate, nextCoordinateX)),
        coordinateY: Math.min(maximumDragCoordinate, Math.max(minimumDragCoordinate, nextCoordinateY)),
      };

      setEditorCorners((currentCorners) => currentCorners.map((editorCorner) => (
        editorCorner.name === activeCornerName ? { ...editorCorner, point: nextPoint } : editorCorner
      )));
    }

    function handlePointerUp() {
      setActiveCornerName(null);
    }

    window.addEventListener('pointermove', handlePointerMove);
    window.addEventListener('pointerup', handlePointerUp);

    return () => {
      window.removeEventListener('pointermove', handlePointerMove);
      window.removeEventListener('pointerup', handlePointerUp);
    };
  }, [activeCornerName]);

  return (
    <motion.div
      className="editor-preview"
      ref={editorPerspectiveElementRef}
      initial={{ opacity: 0, rotateX: 10, rotateY: -18, rotateZ: 2, scale: 0.88, y: 72 }}
      animate={{ opacity: 1, rotateX: 0, rotateY: 0, rotateZ: 0, scale: 1, y: 0 }}
      transition={{ duration: 1.25, ease: [0.16, 1, 0.3, 1], delay: 0.28 }}
    >
      <img className="editor-preview__frame-image" src={frameUrl.src} alt="" aria-hidden="true" />
      <div className="editor-preview__coordinate-space" style={{ transform: editorCoordinateTransform }}>
        <div className="editor-preview__panel" style={{ transform: editorPanelTransform }}>
          <div className="playground-preview playground-preview--dark">
            <section className="playground playground__frame">
              <div className="playground__main">
                <header className="playground__topbar">
                  <div className="playground__brand">
                    <img src={logoUrl.src} alt="Superwire" className="playground__logo" />
                  </div>

                  <div className="playground__topbar-actions">
                    <button className="button button--ghost button--icon-lg playground__theme-toggle" type="button" aria-label="Toggle theme">
                      <Sun />
                    </button>
                  </div>
                </header>

                <div className="playground__tabs playground-tabs">
                  <div className="playground-tabs__list">
                    <div className="playground-tabs__item">
                      <button className="playground-tabs__trigger" type="button" data-state="inactive">
                        <span className="playground-tabs__dot" />
                        <span className="playground-tabs__title">Launch brief</span>
                        <span className="mini-status mini-status--completed">completed</span>
                      </button>
                    </div>

                    <div className="playground-tabs__item playground-tabs__item--active">
                      <button className="playground-tabs__trigger" type="button" data-state="active" data-active>
                        <span className="playground-tabs__dot" />
                        <span className="playground-tabs__title">Workflow 2</span>
                        <span className="mini-status mini-status--completed">completed</span>
                      </button>

                      <div className="playground-tabs__actions">
                        <button className="button button--ghost button--icon-sm playground-tabs__action" type="button" aria-label="Rename Workflow 2"><Pencil /></button>
                        <button className="button button--ghost button--icon-sm playground-tabs__action" type="button" aria-label="Duplicate Workflow 2"><Copy /></button>
                        <button className="button button--ghost button--icon-sm playground-tabs__action" type="button" aria-label="Close Workflow 2"><Trash2 /></button>
                      </div>
                    </div>

                    <button className="button button--outline button--lg playground-tabs__new" type="button"><Plus /> Workflow</button>
                  </div>
                </div>

                <div className="playground__canvas">
                  <section className="playground__content">
                    <div className="playground__controls">
                      <nav className="playground-mode-switch" aria-label="Playground mode">
                        <button className="button button--secondary button--lg playground-mode-switch__button" type="button"><Workflow /> Workflow</button>
                        <button className="button button--ghost button--lg playground-mode-switch__button" type="button"><Braces /> Variables</button>
                      </nav>

                      <div className="playground-actions">
                        <span className="status-pill status-pill--invalid">invalid</span>
                        <button className="button button--ghost button--lg" type="button"><RefreshCcw /> Format</button>
                        <button className="button button--ghost button--lg" type="button">Validate</button>
                        <button className="button button--lg playground-actions__run" type="button"><Play /> Run workflow</button>
                      </div>
                    </div>

                    <section className="workflow-layout">
                      <div className="workflow-layout__top workflow-layout__top--single">
                        <article className="workflow-editor">
                          <div className="workflow-editor__header">
                            <div className="workflow-editor__title-block">
                              <strong>Workflow 2</strong>
                            </div>
                          </div>

                          <div className="wire-editor-shell">
                            <div className="wire-editor-preview" aria-label="Superwire workflow code preview">
                              <div className="wire-editor-preview__gutters" aria-hidden="true">
                                {codeLines.map((_, codeLineIndex) => <span key={`gutter-${codeLineIndex + 1}`}>{codeLineIndex + 1}</span>)}
                              </div>

                              <div className="wire-editor-preview__content">
                                {codeLines.map((codeLine, codeLineIndex) => (
                                  <div className="wire-editor-preview__line" key={`code-line-${codeLineIndex + 1}`}>
                                    {codeLine.map((codeSegment, codeSegmentIndex) => {
                                      const colorName = codeSegment.color;
                                      const className = colorName ? colorClassNames[colorName] : 'text-[#d6d6d6]';

                                      return <span className={className} key={`${codeSegment.text}-${codeSegmentIndex}`}>{codeSegment.text}</span>;
                                    })}
                                  </div>
                                ))}
                              </div>
                            </div>
                          </div>

                          <div className="workflow-editor__message workflow-editor__message--error">
                            <span className="workflow-editor__message-line workflow-editor__message-line--full">Unable to validate workflow: provider endpoint is not reachable.</span>
                          </div>
                        </article>
                      </div>

                      <div className="workflow-layout__bottom">
                        <article className="panel-card workflow-log-panel" data-state="open">
                          <div className="panel-card__header">
                            <div className="panel-card__title-block">
                              <strong>Output</strong>
                              <small>Final workflow output payload.</small>
                            </div>
                          </div>
                          <div className="workflow-log-panel__body">
                            <pre className="workflow-output workflow-output__json">
                              {
                                "{\n" +
                                  "  \"name\": \"Jane Doe\",\n" +
                                  "  \"age\": 28,\n" +
                                  "  \"isEmployed\": true,\n" +
                                  "  \"skills\": [\"Rust\", \"Ai\", \"React\"],\n" +
                                  "  \"address\": {\n" +
                                  "    \"city\": \"São Paulo\",\n" +
                                  "    \"country\": \"Brazil\"\n" +
                                  "  }\n" +
                                  "}"
                              }
                            </pre>
                          </div>
                        </article>

                        <article className="panel-card workflow-log-panel" data-state="open">
                          <div className="panel-card__header">
                            <div className="panel-card__title-block">
                              <strong>Server events</strong>
                              <small>3 streamed events.</small>
                            </div>
                          </div>
                          <div className="workflow-log-panel__body events-log">
                            <div className="events-log__item">
                              <div className="events-log__item-trigger">
                                <span className="events-log__item-meta"><span className="event-chip event-chip--completed">completed</span><span className="events-log__item-summary">agent.greeting finished</span></span>
                                <span className="events-log__item-time">12ms</span>
                              </div>
                            </div>
                          </div>
                        </article>
                      </div>
                    </section>
                  </section>
                </div>
              </div>
            </section>
          </div>
          </div>
        {isCalibrationEnabled ? (
          <div className="editor-preview__calibration" data-dragging={activeCornerName ? 'true' : 'false'}>
            {editorCorners.map((editorCorner) => (
              <button
                className="editor-preview__calibration-handle"
                key={editorCorner.name}
                onPointerDown={(pointerEvent) => {
                  pointerEvent.currentTarget.setPointerCapture(pointerEvent.pointerId);
                  setActiveCornerName(editorCorner.name);
                }}
                style={{ left: `${editorCorner.point.coordinateX}px`, top: `${editorCorner.point.coordinateY}px` }}
                type="button"
              >
                {editorCorner.label}
              </button>
            ))}
          </div>
        ) : null}
      </div>
    </motion.div>
  );
}

export default function LandingHero() {
  return (
    <main className="hero">
      <div className="hero__noise" />
      <div className="hero__grid" />
      <div className="hero__inner">
        <motion.section
          className="hero__copy"
          initial={{ opacity: 0, x: -42, filter: 'blur(10px)' }}
          animate={{ opacity: 1, x: 0, filter: 'blur(0px)' }}
          transition={{ duration: 0.9, ease: [0.16, 1, 0.3, 1] }}
        >
          {/*<img className="hero__logo" src={logoUrl.src} alt="Superwire" />*/}

          <div className="hero__copy-content">
            <motion.h1
              initial={{ opacity: 0, y: 28 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.9, delay: 0.16, ease: [0.16, 1, 0.3, 1] }}
            >
              Build backend agent systems with <span>clear, controllable workflows.</span>
            </motion.h1>

            <motion.p
              initial={{ opacity: 0, y: 24 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.85, delay: 0.32, ease: [0.16, 1, 0.3, 1] }}
            >
              Superwire is a <strong>declarative DSL</strong> for backend agent workflows. Define each step in code,
              keep tools and context <strong>scoped</strong>, and return <strong>structured outputs</strong> your app can
              use immediately.
            </motion.p>

            <motion.a
              className="documentation-button"
              href={documentationUrl}
              initial={{ opacity: 0, y: 22 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.78, delay: 0.48, ease: [0.16, 1, 0.3, 1] }}
            >
              <FileText aria-hidden="true" size={25} strokeWidth={2.2} />
              <span>Read the documentation</span>
              <ArrowRight className="documentation-button__arrow" aria-hidden="true" size={27} strokeWidth={2.2} />
            </motion.a>

          </div>
        </motion.section>

        <section className="hero__visual" aria-label="Superwire editor preview">
          <CircuitLines />
          <EditorWindow />
        </section>
      </div>

      <footer className="hero__footer">
        <a aria-label="View Superwire on GitHub" className="github-link" href={githubUrl} rel="noreferrer" target="_blank">
          <svg aria-hidden="true" className="github-link__icon" viewBox="0 0 24 24">
            <path
              d="M12 2C6.48 2 2 6.59 2 12.25c0 4.53 2.87 8.37 6.84 9.73.5.09.68-.22.68-.49v-1.9c-2.78.62-3.37-1.22-3.37-1.22-.45-1.18-1.11-1.5-1.11-1.5-.91-.64.07-.63.07-.63 1 .07 1.53 1.06 1.53 1.06.9 1.57 2.35 1.12 2.92.85.09-.67.35-1.12.63-1.38-2.22-.26-4.56-1.14-4.56-5.06 0-1.12.39-2.03 1.03-2.75-.1-.26-.45-1.3.1-2.71 0 0 .84-.28 2.75 1.05A9.28 9.28 0 0 1 12 6.96c.85 0 1.7.12 2.5.34 1.9-1.33 2.74-1.05 2.74-1.05.55 1.41.2 2.45.1 2.71.64.72 1.03 1.63 1.03 2.75 0 3.93-2.34 4.8-4.57 5.05.36.32.68.94.68 1.9v2.83c0 .27.18.59.69.49A10.13 10.13 0 0 0 22 12.25C22 6.59 17.52 2 12 2Z"
              fill="currentColor"
            />
          </svg>
        </a>
      </footer>
    </main>
  );
}
