import { useEffect, useMemo, useRef, useState } from 'react';
import frameUrl from '../../frame.webp';
import logoUrl from '../../../docs/public/logo-horizontal.svg';

type Point = {
  x: number;
  y: number;
};

type CornerName = 'topLeft' | 'topRight' | 'bottomRight' | 'bottomLeft';

type Corner = {
  name: CornerName;
  label: string;
  point: Point;
};

const sourceWidth = 1000;
const sourceHeight = 1000;

const sourceCorners: Point[] = [
  { x: 0, y: 0 },
  { x: sourceWidth, y: 0 },
  { x: sourceWidth, y: sourceHeight },
  { x: 0, y: sourceHeight },
];

const initialCorners: Corner[] = [
  { name: 'topLeft', label: 'TL', point: { x: 74, y: 70 } },
  { name: 'topRight', label: 'TR', point: { x: 925, y: 28 } },
  { name: 'bottomRight', label: 'BR', point: { x: 883, y: 944 } },
  { name: 'bottomLeft', label: 'BL', point: { x: 32, y: 912 } },
];

function solveLinearSystem(matrix: number[][], vector: number[]) {
  const rowCount = matrix.length;
  const augmentedMatrix = matrix.map((row, rowIndex) => [...row, vector[rowIndex]]);

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
      throw new Error('Corner positions produced a singular transform matrix.');
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

  return augmentedMatrix.map((row) => row[rowCount]);
}

function getTransformMatrix(sourcePoints: Point[], targetPoints: Point[]) {
  const matrixRows: number[][] = [];
  const vector: number[] = [];

  sourcePoints.forEach((sourcePoint, sourcePointIndex) => {
    const targetPoint = targetPoints[sourcePointIndex];

    matrixRows.push([sourcePoint.x, sourcePoint.y, 1, 0, 0, 0, -sourcePoint.x * targetPoint.x, -sourcePoint.y * targetPoint.x]);
    matrixRows.push([0, 0, 0, sourcePoint.x, sourcePoint.y, 1, -sourcePoint.x * targetPoint.y, -sourcePoint.y * targetPoint.y]);
    vector.push(targetPoint.x, targetPoint.y);
  });

  const solution = solveLinearSystem(matrixRows, vector);
  const homographyMatrix = [
    [solution[0], solution[1], 0, solution[2]],
    [solution[3], solution[4], 0, solution[5]],
    [0, 0, 1, 0],
    [solution[6], solution[7], 0, 1],
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

function formatNumber(value: number) {
  return Number.parseFloat(value.toFixed(8)).toString();
}

function formatMatrix(matrixValues: number[]) {
  return `matrix3d(${matrixValues.map(formatNumber).join(', ')})`;
}

function pointToPercent(point: Point) {
  return {
    x: Number.parseFloat((point.x / 10).toFixed(3)),
    y: Number.parseFloat((point.y / 10).toFixed(3)),
  };
}

export default function Matrix3dCornerTool() {
  const stageElementRef = useRef<HTMLDivElement | null>(null);
  const [corners, setCorners] = useState(initialCorners);
  const [activeCornerName, setActiveCornerName] = useState<CornerName | null>(null);
  const targetPoints = useMemo(() => corners.map((corner) => corner.point), [corners]);
  const matrixValues = useMemo(() => getTransformMatrix(sourceCorners, targetPoints), [targetPoints]);
  const matrixCss = useMemo(() => formatMatrix(matrixValues), [matrixValues]);
  const cornerPercentages = useMemo(() => corners.map((corner) => ({ ...corner, point: pointToPercent(corner.point) })), [corners]);

  useEffect(() => {
    function handlePointerMove(pointerEvent: PointerEvent) {
      if (!activeCornerName || !stageElementRef.current) {
        return;
      }

      const stageRect = stageElementRef.current.getBoundingClientRect();
      const nextPoint = {
        x: Math.min(1000, Math.max(0, ((pointerEvent.clientX - stageRect.left) / stageRect.width) * 1000)),
        y: Math.min(1000, Math.max(0, ((pointerEvent.clientY - stageRect.top) / stageRect.height) * 1000)),
      };

      setCorners((currentCorners) => currentCorners.map((corner) => (corner.name === activeCornerName ? { ...corner, point: nextPoint } : corner)));
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

  function resetCorners() {
    setCorners(initialCorners);
  }

  function nudgeCorner(cornerName: CornerName, deltaX: number, deltaY: number) {
    setCorners((currentCorners) => currentCorners.map((corner) => {
      if (corner.name !== cornerName) {
        return corner;
      }

      return {
        ...corner,
        point: {
          x: Math.min(1000, Math.max(0, corner.point.x + deltaX)),
          y: Math.min(1000, Math.max(0, corner.point.y + deltaY)),
        },
      };
    }));
  }

  return (
    <main className="matrix-tool">
      <section className="matrix-tool__stage-panel">
        <div className="matrix-tool__stage" data-dragging={activeCornerName ? 'true' : 'false'} ref={stageElementRef}>
          <img className="matrix-tool__frame" src={frameUrl.src} alt="Frame calibration background" />

          <div className="matrix-tool__surface" style={{ transform: matrixCss }}>
            <div className="matrix-tool__mock-app">
              <header className="matrix-tool__mock-header">
                <img className="matrix-tool__mock-logo" src={logoUrl.src} alt="Superwire" />
                <span className="matrix-tool__mock-avatar" />
              </header>

              <div className="matrix-tool__mock-tabs">
                <span className="matrix-tool__mock-tab" />
                <span className="matrix-tool__mock-tab" />
                <span className="matrix-tool__mock-tab" />
              </div>

              <div className="matrix-tool__mock-body">
                <aside className="matrix-tool__mock-sidebar" />

                <section className="matrix-tool__mock-content">
                  <div className="matrix-tool__mock-content-row" />
                  <div className="matrix-tool__mock-content-row" />
                  <div className="matrix-tool__mock-content-row" />
                </section>
              </div>
            </div>
          </div>

          {corners.map((corner) => (
            <button
              className="matrix-tool__handle"
              data-active={activeCornerName === corner.name ? 'true' : 'false'}
              key={corner.name}
              onPointerDown={(pointerEvent) => {
                pointerEvent.currentTarget.setPointerCapture(pointerEvent.pointerId);
                setActiveCornerName(corner.name);
              }}
              style={{ left: `${corner.point.x / 10}%`, top: `${corner.point.y / 10}%` }}
              type="button"
            >
              {corner.label}
            </button>
          ))}
        </div>
      </section>

      <aside className="matrix-tool__controls">
        <div className="matrix-tool__header">
          <p>Superwire frame calibration</p>
          <h1>Drag corners to generate matrix3d</h1>
          <button type="button" onClick={resetCorners}>Reset</button>
        </div>

        <section className="matrix-tool__card">
          <h2>Matrix CSS</h2>
          <textarea readOnly value={`transform-origin: 0 0;\ntransform: ${matrixCss};`} />
        </section>

        <section className="matrix-tool__card">
          <h2>Corner percentages</h2>
          <pre>{JSON.stringify(Object.fromEntries(cornerPercentages.map((corner) => [corner.name, corner.point])), null, 2)}</pre>
        </section>

        <section className="matrix-tool__card">
          <h2>Nudge active corner</h2>
          <div className="matrix-tool__nudges">
            {corners.map((corner) => (
              <div className="matrix-tool__nudge-row" key={`${corner.name}-nudge`}>
                <strong>{corner.label}</strong>
                <button type="button" onClick={() => nudgeCorner(corner.name, 0, -1)}>↑</button>
                <button type="button" onClick={() => nudgeCorner(corner.name, -1, 0)}>←</button>
                <button type="button" onClick={() => nudgeCorner(corner.name, 1, 0)}>→</button>
                <button type="button" onClick={() => nudgeCorner(corner.name, 0, 1)}>↓</button>
              </div>
            ))}
          </div>
        </section>
      </aside>
    </main>
  );
}
