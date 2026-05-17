import { ArrowRight, Braces, Copy, FileText, Pencil, Play, Plus, RefreshCcw, Sun, Trash2, Workflow } from 'lucide-react';
import { motion } from 'motion/react';
import { useEffect, useMemo, useRef, useState } from 'react';
import frameUrl from '../../frame.webp';
import logoUrl from '../../../docs/public/logo-horizontal.svg';

const documentationUrl = 'https://docs.superwire.dev';

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

const codeLines: CodeSegment[][] = [
  [{ text: 'provider', color: 'keyword' }, { text: ' openai ', color: 'plain' }, { text: 'from', color: 'keyword' }, { text: ' openai {' }],
  [{ text: '  endpoint: ', color: 'property' }, { text: '"http://100.118.299.48:3000/v1"', color: 'string' }],
  [{ text: '  api_key: ', color: 'property' }, { text: '"sk-CLKR4I0qU4oPFyTNjACCTDrqO66EMYTx1PNFSoolZF6wFuzz"', color: 'string' }],
  [{ text: '}' }],
  [],
  [{ text: 'model', color: 'keyword' }, { text: ' openai_model ', color: 'plain' }, { text: 'from', color: 'keyword' }, { text: ' openai {' }],
  [{ text: '  id: ', color: 'property' }, { text: '"big-pickle"', color: 'string' }],
  [{ text: '}' }],
  [],
  [{ text: 'mcp', color: 'keyword' }, { text: ' local {' }],
  [{ text: '  endpoint: ', color: 'property' }, { text: '"http://localhost:8000/mcp/summarizer"', color: 'string' }],
  [{ text: '  headers {' }],
  [{ text: '    Accept: ', color: 'property' }, { text: '"application/json"', color: 'string' }],
  [{ text: '    Authorization: ', color: 'property' }, { text: '"Bearer 74N!CJXMMCJrHwFa6qApHt7X8Pg00NiLj1MKXyR81da8Sdce"', color: 'string' }],
  [{ text: '  }' }],
  [{ text: '}' }],
  [],
  [{ text: 'from', color: 'keyword' }, { text: ' mcp.local {' }],
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
  0.763615, -0.0418114, 0, -9.81e-05, -0.0668422, 0.8398, 0, -2.72e-05, 0, 0, 1, 0, 85.7695, 59.7734, 0, 1
];

const editorSourceSize = 1000;
const editorCalibrationDragMargin = 500;

const editorSourceCorners: EditorTransformPoint[] = [
  { coordinateX: 0, coordinateY: 0 },
  { coordinateX: editorSourceSize, coordinateY: 0 },
  { coordinateX: editorSourceSize, coordinateY: editorSourceSize },
  { coordinateX: 0, coordinateY: editorSourceSize },
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

function CircuitLines() {
  const circuitPaths = [
    { path: 'M2 126 H86 C114 126 114 158 143 158 H194', duration: 5.2, delay: 0 },
    { path: 'M0 262 H136 C164 262 164 296 193 296 H238', duration: 6.4, delay: 0.35 },
    { path: 'M102 24 V88 C102 112 82 116 63 116 H0', duration: 5.8, delay: 0.7 },
    { path: 'M682 58 H755 C783 58 786 92 814 92 H878', duration: 6.1, delay: 0.1 },
    { path: 'M710 300 H807 C835 300 835 334 864 334 H930', duration: 5.6, delay: 0.55 },
    { path: 'M686 444 H760 C790 444 790 492 820 492 H932', duration: 6.8, delay: 0.85 },
  ];

  return (
    <svg aria-hidden="true" className="circuit-board" viewBox="0 0 930 560" preserveAspectRatio="none">
      <defs>
        <filter id="circuit-glow" x="-30%" y="-30%" width="160%" height="160%">
          <feGaussianBlur stdDeviation="2.4" result="blur" />
          <feMerge>
            <feMergeNode in="blur" />
            <feMergeNode in="SourceGraphic" />
          </feMerge>
        </filter>
      </defs>

      {circuitPaths.map((circuitPath) => (
        <g key={circuitPath.path}>
          <path
            d={circuitPath.path}
            fill="none"
            stroke="#ff7900"
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth="1"
            opacity="0.34"
          />
          <motion.path
            d={circuitPath.path}
            fill="none"
            stroke="#ff8a14"
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth="1.35"
            strokeDasharray="1 17"
            filter="url(#circuit-glow)"
            initial={{ strokeDashoffset: 0, opacity: 0.32 }}
            animate={{ strokeDashoffset: -72, opacity: [0.32, 0.95, 0.32] }}
            transition={{
              strokeDashoffset: {
                delay: circuitPath.delay,
                duration: circuitPath.duration,
                ease: 'linear',
                repeat: Infinity,
              },
              opacity: {
                delay: circuitPath.delay,
                duration: circuitPath.duration * 0.5,
                ease: 'easeInOut',
                repeat: Infinity,
                repeatType: 'mirror',
              },
            }}
          />
        </g>
      ))}

      <motion.circle
        cx="2"
        cy="126"
        r="5"
        fill="#ff7900"
        animate={{ opacity: [0.35, 1, 0.35], scale: [0.92, 1.18, 0.92] }}
        transition={{ duration: 2.1, repeat: Infinity, ease: 'easeInOut' }}
      />
      <motion.circle
        cx="0"
        cy="262"
        r="4.5"
        fill="#ff7900"
        animate={{ opacity: [0.22, 0.92, 0.22], scale: [0.9, 1.2, 0.9] }}
        transition={{ delay: 0.8, duration: 2.4, repeat: Infinity, ease: 'easeInOut' }}
      />
    </svg>
  );
}

function EditorWindow() {
  const editorPerspectiveElementRef = useRef<HTMLDivElement | null>(null);
  const [editorPanelTransform, setEditorPanelTransform] = useState('none');
  const [editorCoordinateTransform, setEditorCoordinateTransform] = useState('scale(1)');
  const [isCalibrationEnabled, setIsCalibrationEnabled] = useState(false);
  const [activeCornerName, setActiveCornerName] = useState<EditorCornerName | null>(null);
  const [editorCorners, setEditorCorners] = useState(createInitialEditorCorners);
  const editorMatrixValues = useMemo(
    () => getEditorTransformMatrix(editorSourceCorners, editorCorners.map((editorCorner) => editorCorner.point)),
    [editorCorners],
  );

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
    setEditorPanelTransform(`matrix3d(${editorMatrixValues.join(',')})`);
  }, [editorMatrixValues]);

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
      className="editor-perspective"
      ref={editorPerspectiveElementRef}
      initial={{ opacity: 0, rotateX: 10, rotateY: -18, rotateZ: 2, scale: 0.88, y: 72 }}
      animate={{ opacity: 1, rotateX: 0, rotateY: 0, rotateZ: 0, scale: 1, y: 0 }}
      transition={{ duration: 1.25, ease: [0.16, 1, 0.3, 1], delay: 0.28 }}
    >
      <img className="editor-frame-image" src={frameUrl.src} alt="" aria-hidden="true" />
      <div className="editor-coordinate-space" style={{ transform: editorCoordinateTransform }}>
        <div className="editor-panel" style={{ transform: editorPanelTransform }}>
          <div className="playground-preview dark">
            <section className="playground__frame">
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

                <div className="playground__tabs">
                  <div className="tabs-list">
                    <div className="playground-tabs__item">
                      <button className="playground-tabs__trigger" type="button" data-state="inactive">
                        <span className="playground-tabs__dot" />
                        <span className="playground-tabs__title">Launch brief</span>
                        <span className="mini-status completed">completed</span>
                      </button>
                    </div>

                    <div className="playground-tabs__item playground-tabs__item--active">
                      <button className="playground-tabs__trigger" type="button" data-state="active" data-active>
                        <span className="playground-tabs__dot" />
                        <span className="playground-tabs__title">Workflow 2</span>
                        <span className="mini-status completed">completed</span>
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
                        <span className="status-pill invalid">invalid</span>
                        <button className="button button--ghost button--lg" type="button"><RefreshCcw /> Format</button>
                        <button className="button button--ghost button--lg" type="button">Validate</button>
                        <button className="button button--lg playground-actions__run" type="button"><Play /> Run workflow</button>
                      </div>
                    </div>

                    <section className="workflow-layout">
                      <div className="workflow-layout__top workflow-layout__top--single">
                        <article className="workflow-editor">
                          <div className="workflow-editor__header panel-card__header">
                            <div className="panel-card__title-block">
                              <strong>Workflow 2</strong>
                            </div>
                          </div>

                          <div className="wire-editor-shell">
                            <div className="wire-editor-preview" aria-label="Superwire workflow code preview">
                              <div className="cm-gutters" aria-hidden="true">
                                {codeLines.map((_, codeLineIndex) => <span key={`gutter-${codeLineIndex + 1}`}>{codeLineIndex + 1}</span>)}
                              </div>

                              <div className="cm-content">
                                {codeLines.map((codeLine, codeLineIndex) => (
                                  <div className="cm-line" key={`code-line-${codeLineIndex + 1}`}>
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
                            <pre className="workflow-output__json">{"{\n  \"greeting\": \"Summary is ready.\"\n}"}</pre>
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
                                <span className="events-log__item-meta"><span className="event-chip event-completed">completed</span><span className="events-log__item-summary">agent.greeting finished</span></span>
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
          <div className="editor-calibration" data-dragging={activeCornerName ? 'true' : 'false'}>
            {editorCorners.map((editorCorner) => (
              <button
                className="editor-calibration__handle"
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
    <main className="hero-shell">
      <div className="hero-noise" />
      <div className="hero-grid" />
      <div className="hero-inner">
        <motion.section
          className="hero-copy"
          initial={{ opacity: 0, x: -42, filter: 'blur(10px)' }}
          animate={{ opacity: 1, x: 0, filter: 'blur(0px)' }}
          transition={{ duration: 0.9, ease: [0.16, 1, 0.3, 1] }}
        >
          <img className="hero-logo" src={logoUrl.src} alt="Superwire" />

          <div className="hero-copy-content">
            <motion.h1
              initial={{ opacity: 0, y: 28 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.9, delay: 0.16, ease: [0.16, 1, 0.3, 1] }}
            >
              Turn AI agent behavior into a <span>controlled backend workflow.</span>
            </motion.h1>

            <motion.p
              initial={{ opacity: 0, y: 24 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.85, delay: 0.32, ease: [0.16, 1, 0.3, 1] }}
            >
              Superwire is a <strong>declarative DSL</strong> for server-side AI orchestration. Define workflows in code,
              use <strong>scoped tools</strong>, enforce <strong>typed outputs</strong> with <strong>validation</strong>, and
              stream results with built-in observability and <strong>streaming execution</strong>.
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

        <section className="hero-visual" aria-label="Superwire editor preview">
          <CircuitLines />
          <EditorWindow />
        </section>
      </div>
    </main>
  );
}
