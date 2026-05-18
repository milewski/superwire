import { ArrowRight, Braces, Copy, FileText, Pencil, Play, Plus, RefreshCcw, Sun, Trash2, Workflow } from 'lucide-react';
import { motion } from 'motion/react';
import { memo, type CSSProperties, useEffect, useMemo, useRef, useState } from 'react';
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

type CircuitFramePath = {
  path: string;
  duration: number;
  delay: number;
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

const circuitBoardWidth = 1100;
const circuitBoardHeight = 720;

const circuitTilePaths = [
  'M0 28 H34 C48 28 48 52 62 52 H120',
  'M0 96 H28 C44 96 44 74 60 74 H86 C102 74 102 52 120 52',
  'M23 0 V23 C23 37 42 37 42 52',
  'M78 120 V96 C78 82 98 82 98 68 V0',
  'M0 52 H18',
  'M102 96 H120',
];

const circuitFramePaths: CircuitFramePath[] = [
  { path: 'M42 138 H188 C230 138 230 96 272 96 H356 C392 96 392 64 430 64 H680 C718 64 718 96 754 96 H838 C880 96 880 138 922 138 H1062', duration: 7.8, delay: -1.2 },
  { path: 'M18 262 H154 C194 262 194 218 236 218 H328 C362 218 362 190 398 190', duration: 6.1, delay: -3.2 },
  { path: 'M1082 268 H948 C908 268 908 224 866 224 H778 C744 224 744 192 708 192', duration: 6.4, delay: -2.1 },
  { path: 'M0 448 H126 C170 448 170 492 214 492 H324 C360 492 360 528 398 528', duration: 6.8, delay: -4.4 },
  { path: 'M1100 464 H970 C926 464 926 510 882 510 H776 C738 510 738 542 700 542', duration: 6.6, delay: -1.8 },
  { path: 'M126 652 H294 C338 652 338 610 382 610 H718 C762 610 762 652 806 652 H974', duration: 8.5, delay: -5.4 },
  { path: 'M258 18 V62 C258 104 302 104 302 146 V198', duration: 5.4, delay: -2.8 },
  { path: 'M838 24 V72 C838 112 796 112 796 152 V206', duration: 5.7, delay: -0.7 },
];

const circuitNodes = [
  { coordinateX: 188, coordinateY: 138, radius: 4.5, delay: 0.1 },
  { coordinateX: 430, coordinateY: 64, radius: 5.5, delay: 0.45 },
  { coordinateX: 754, coordinateY: 96, radius: 4.5, delay: 0.8 },
  { coordinateX: 328, coordinateY: 218, radius: 4, delay: 1.05 },
  { coordinateX: 778, coordinateY: 224, radius: 4, delay: 1.3 },
  { coordinateX: 324, coordinateY: 492, radius: 4.5, delay: 0.65 },
  { coordinateX: 776, coordinateY: 510, radius: 4.5, delay: 0.95 },
  { coordinateX: 550, coordinateY: 610, radius: 5, delay: 1.55 },
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
        <defs>
          <pattern id="circuit-static-tile" width="120" height="120" patternUnits="userSpaceOnUse">
            {circuitTilePaths.map((circuitTilePath) => (
              <path
                d={circuitTilePath}
                fill="none"
                key={circuitTilePath}
                stroke="#ff7900"
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth="0.65"
                opacity="0.16"
              />
            ))}
            <circle cx="23" cy="23" r="2.2" fill="#ff9b32" opacity="0.18" />
            <circle cx="78" cy="96" r="2" fill="#ff9b32" opacity="0.14" />
            <circle cx="98" cy="68" r="1.7" fill="#ff9b32" opacity="0.14" />
          </pattern>
        </defs>

        <rect className="circuit-board__static-pattern" width={circuitBoardWidth} height={circuitBoardHeight} fill="url(#circuit-static-tile)" />

        <g className="circuit-board__trace-group">
          {circuitFramePaths.map((circuitFramePath) => (
            <path
              d={circuitFramePath.path}
              fill="none"
              key={circuitFramePath.path}
              stroke="#ff7900"
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth="1.2"
            />
          ))}
        </g>

        <g className="circuit-board__glint-group">
          {circuitFramePaths.map((circuitFramePath) => (
            <g key={`${circuitFramePath.path}-glint`}>
              <path
                className="circuit-board__glint circuit-board__glint--halo"
                d={circuitFramePath.path}
                pathLength="100"
              >
                <animate
                  attributeName="stroke-dashoffset"
                  begin={`${circuitFramePath.delay}s`}
                  dur={`${circuitFramePath.duration}s`}
                  from="100"
                  repeatCount="indefinite"
                  to="0"
                />
              </path>

              <path
                className="circuit-board__glint circuit-board__glint--core"
                d={circuitFramePath.path}
                pathLength="100"
              >
                <animate
                  attributeName="stroke-dashoffset"
                  begin={`${circuitFramePath.delay}s`}
                  dur={`${circuitFramePath.duration}s`}
                  from="100"
                  repeatCount="indefinite"
                  to="0"
                />
              </path>
            </g>
          ))}
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
          {/*<img className="hero-logo" src={logoUrl.src} alt="Superwire" />*/}

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
