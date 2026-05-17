import { useEffect, useRef } from "react";
import * as THREE from "three";

const vertexShader = `
varying vec2 fragmentUv;

void main() {
  fragmentUv = uv;
  gl_Position = vec4(position.xy, 0.0, 1.0);
}
`;

const fragmentShader = `
precision highp float;

uniform float elapsedTime;
uniform vec2 resolution;
varying vec2 fragmentUv;

float lineDistance(vec2 point, vec2 startPoint, vec2 endPoint) {
  vec2 segment = endPoint - startPoint;
  float segmentProgress = clamp(dot(point - startPoint, segment) / dot(segment, segment), 0.0, 1.0);

  return length(point - (startPoint + segment * segmentProgress));
}

float segmentProgress(vec2 point, vec2 startPoint, vec2 endPoint) {
  vec2 segment = endPoint - startPoint;

  return clamp(dot(point - startPoint, segment) / dot(segment, segment), 0.0, 1.0);
}

float circuitSegment(vec2 point, vec2 startPoint, vec2 endPoint, float offset) {
  float distanceToLine = lineDistance(point, startPoint, endPoint);
  float core = smoothstep(0.0075, 0.0014, distanceToLine);
  float glow = smoothstep(0.042, 0.0, distanceToLine) * 0.72;
  float progress = segmentProgress(point, startPoint, endPoint);
  float runner = smoothstep(0.12, 0.0, abs(fract(progress - elapsedTime * 0.22 + offset) - 0.5));

  return core * 1.25 + glow + runner * smoothstep(0.03, 0.0, distanceToLine) * 1.9;
}

float node(vec2 point, vec2 center, float radius) {
  float distanceToCenter = length(point - center);

  return smoothstep(radius, radius * 0.42, distanceToCenter) + smoothstep(radius * 4.0, 0.0, distanceToCenter) * 0.38;
}

float circuit(vec2 point) {
  float value = 0.0;
  value += circuitSegment(point, vec2(0.365, 0.71), vec2(0.43, 0.71), 0.02);
  value += circuitSegment(point, vec2(0.43, 0.71), vec2(0.44, 0.695), 0.08);
  value += circuitSegment(point, vec2(0.44, 0.695), vec2(0.44, 0.635), 0.16);
  value += circuitSegment(point, vec2(0.44, 0.635), vec2(0.455, 0.62), 0.24);
  value += circuitSegment(point, vec2(0.455, 0.62), vec2(0.61, 0.62), 0.32);
  value += circuitSegment(point, vec2(0.52, 0.62), vec2(0.52, 0.535), 0.42);
  value += circuitSegment(point, vec2(0.52, 0.535), vec2(0.66, 0.535), 0.52);
  value += circuitSegment(point, vec2(0.455, 0.54), vec2(0.475, 0.52), 0.6);
  value += circuitSegment(point, vec2(0.475, 0.52), vec2(0.63, 0.52), 0.68);
  value += circuitSegment(point, vec2(0.455, 0.72), vec2(0.455, 0.575), 0.62);
  value += circuitSegment(point, vec2(0.455, 0.575), vec2(0.482, 0.56), 0.7);
  value += circuitSegment(point, vec2(0.482, 0.56), vec2(0.65, 0.56), 0.78);
  value += circuitSegment(point, vec2(0.445, 0.81), vec2(0.445, 0.735), 0.36);
  value += circuitSegment(point, vec2(0.445, 0.735), vec2(0.47, 0.72), 0.44);
  value += circuitSegment(point, vec2(0.47, 0.72), vec2(0.64, 0.72), 0.52);
  value += circuitSegment(point, vec2(0.905, 0.42), vec2(0.985, 0.42), 0.12);
  value += circuitSegment(point, vec2(0.985, 0.42), vec2(0.985, 0.18), 0.26);
  value += circuitSegment(point, vec2(0.985, 0.18), vec2(0.925, 0.18), 0.34);
  value += circuitSegment(point, vec2(0.88, 0.085), vec2(0.98, 0.085), 0.46);
  value += circuitSegment(point, vec2(0.98, 0.085), vec2(0.98, 0.025), 0.58);
  value += node(point, vec2(0.365, 0.71), 0.007);
  value += node(point, vec2(0.445, 0.81), 0.0065);
  value += node(point, vec2(0.905, 0.42), 0.0055);

  return value;
}

void main() {
  vec2 point = fragmentUv;
  float aspect = resolution.x / max(resolution.y, 1.0);
  vec2 gridPoint = vec2(point.x * aspect, point.y);
  vec2 grid = abs(fract(gridPoint * 52.0) - 0.5);
  float dots = smoothstep(0.024, 0.0, length(grid)) * 0.38;
  float rightMask = smoothstep(0.34, 0.54, point.x);
  float centerFade = smoothstep(0.04, 0.22, point.y) * smoothstep(1.0, 0.66, point.y);
  float orangeValue = circuit(point) * centerFade;
  vec3 orange = vec3(1.0, 0.42, 0.0);
  vec3 dotColor = vec3(0.75, 0.75, 0.75) * dots * rightMask * centerFade;
  vec3 color = dotColor + orange * orangeValue;
  float alpha = clamp(dots * 0.28 * rightMask + orangeValue * 1.0, 0.0, 1.0);

  gl_FragColor = vec4(color, alpha);
}
`;

function createRoundedRectangleShape(width, height, radius) {
  const shape = new THREE.Shape();
  const halfWidth = width / 2;
  const halfHeight = height / 2;

  shape.moveTo(-halfWidth + radius, -halfHeight);
  shape.lineTo(halfWidth - radius, -halfHeight);
  shape.quadraticCurveTo(halfWidth, -halfHeight, halfWidth, -halfHeight + radius);
  shape.lineTo(halfWidth, halfHeight - radius);
  shape.quadraticCurveTo(halfWidth, halfHeight, halfWidth - radius, halfHeight);
  shape.lineTo(-halfWidth + radius, halfHeight);
  shape.quadraticCurveTo(-halfWidth, halfHeight, -halfWidth, halfHeight - radius);
  shape.lineTo(-halfWidth, -halfHeight + radius);
  shape.quadraticCurveTo(-halfWidth, -halfHeight, -halfWidth + radius, -halfHeight);

  return shape;
}

function createCircuitCurve(points) {
  return new THREE.CatmullRomCurve3(points.map(([pointX, pointY, pointZ = 0]) => new THREE.Vector3(pointX, pointY, pointZ)), false, "catmullrom", 0.02);
}

function createTubeLine(points, material) {
  const curve = createCircuitCurve(points);
  const geometry = new THREE.TubeGeometry(curve, 96, 0.006, 8, false);
  const mesh = new THREE.Mesh(geometry, material);
  mesh.userData.curve = curve;

  return mesh;
}

function createGlowDot(material) {
  const geometry = new THREE.SphereGeometry(0.035, 24, 24);

  return new THREE.Mesh(geometry, material);
}

export default function WebGLWorkflowScene() {
  const containerReference = useRef(null);

  useEffect(() => {
    const container = containerReference.current;

    if (!container) {
      return undefined;
    }

    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(38, 1, 0.1, 100);
    camera.position.set(0, 0, 7.4);

    const renderer = new THREE.WebGLRenderer({ alpha: true, antialias: true, powerPreference: "high-performance" });
    renderer.setClearColor(0x000000, 0);
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    container.appendChild(renderer.domElement);

    const shaderUniforms = {
      elapsedTime: { value: 0 },
      resolution: { value: new THREE.Vector2(1, 1) },
    };
    const shaderPlane = new THREE.Mesh(
      new THREE.PlaneGeometry(2, 2),
      new THREE.ShaderMaterial({
        blending: THREE.NormalBlending,
        depthWrite: false,
        depthTest: false,
        fragmentShader,
        transparent: true,
        uniforms: shaderUniforms,
        vertexShader,
      }),
    );
    shaderPlane.renderOrder = -10;
    scene.add(shaderPlane);

    const orangeMaterial = new THREE.MeshBasicMaterial({ color: 0xff7f00, transparent: true, opacity: 0.34 });
    const orangeBrightMaterial = new THREE.MeshBasicMaterial({ color: 0xff7f00, transparent: true, opacity: 0.86 });
    const darkPlateMaterial = new THREE.MeshStandardMaterial({
      color: 0x161616,
      emissive: 0x120600,
      emissiveIntensity: 0.32,
      metalness: 0.18,
      opacity: 0.72,
      roughness: 0.74,
      transparent: true,
    });
    const orangeEdgeMaterial = new THREE.MeshStandardMaterial({
      color: 0xff7f00,
      emissive: 0xff7f00,
      emissiveIntensity: 0.52,
      metalness: 0.08,
      opacity: 0.24,
      roughness: 0.58,
      transparent: true,
    });

    const ambientLight = new THREE.AmbientLight(0xffffff, 0.72);
    const orangeLight = new THREE.PointLight(0xff7f00, 5.2, 7.6);
    orangeLight.position.set(2.4, -2.1, 2.8);
    scene.add(ambientLight, orangeLight);

    const plateShape = createRoundedRectangleShape(4.76, 3.58, 0.18);
    const plateGeometry = new THREE.ExtrudeGeometry(plateShape, {
      bevelEnabled: true,
      bevelSegments: 10,
      bevelSize: 0.055,
      bevelThickness: 0.075,
      depth: 0.24,
      steps: 1,
    });
    plateGeometry.center();

    const backPlate = new THREE.Mesh(plateGeometry, darkPlateMaterial);
    backPlate.position.set(2.82, 0.18, -0.62);
    backPlate.rotation.set(THREE.MathUtils.degToRad(0.8), THREE.MathUtils.degToRad(-7), THREE.MathUtils.degToRad(-4.2));
    backPlate.scale.set(1.05, 1.05, 1);

    const orangePlate = new THREE.Mesh(plateGeometry, orangeEdgeMaterial);
    orangePlate.position.set(3.06, -0.02, -0.94);
    orangePlate.rotation.copy(backPlate.rotation);
    orangePlate.scale.set(1.08, 1.08, 1);

    const rearPlate = new THREE.Mesh(plateGeometry, darkPlateMaterial.clone());
    rearPlate.material.opacity = 0.34;
    rearPlate.position.set(3.28, -0.2, -1.28);
    rearPlate.rotation.copy(backPlate.rotation);
    rearPlate.scale.set(1.1, 1.1, 1);
    scene.add(rearPlate, orangePlate, backPlate);

    const circuitGroup = new THREE.Group();
    const firstCircuit = createTubeLine([
      [-0.74, 0.74, 0.08],
      [-0.36, 0.74, 0.08],
      [-0.24, 0.58, 0.08],
      [-0.24, 0.22, 0.08],
      [0.02, 0.08, 0.08],
      [0.58, 0.08, 0.08],
    ], orangeMaterial);
    const secondCircuit = createTubeLine([
      [0.08, 1.38, 0.05],
      [0.08, 1.02, 0.05],
      [-0.02, 0.88, 0.05],
      [-0.45, 0.88, 0.05],
    ], orangeMaterial);
    const thirdCircuit = createTubeLine([
      [4.18, -0.34, -0.08],
      [4.78, -0.34, -0.08],
      [4.94, -0.5, -0.08],
      [4.94, -1.42, -0.08],
      [4.72, -1.64, -0.08],
      [4.08, -1.64, -0.08],
    ], orangeMaterial);
    circuitGroup.add(firstCircuit, secondCircuit, thirdCircuit);

    const firstDot = createGlowDot(orangeBrightMaterial);
    firstDot.position.set(-0.74, 0.74, 0.08);
    const secondDot = createGlowDot(orangeBrightMaterial);
    secondDot.position.set(0.08, 1.38, 0.05);
    circuitGroup.add(firstDot, secondDot);
    scene.add(circuitGroup);

    const traceGeometry = new THREE.SphereGeometry(0.024, 16, 16);
    const traceMaterial = new THREE.MeshBasicMaterial({ color: 0xffa94f, transparent: true, opacity: 0.95 });
    const traceDots = [firstCircuit, secondCircuit, thirdCircuit].map((lineMesh) => {
      const traceDot = new THREE.Mesh(traceGeometry, traceMaterial);
      traceDot.userData.curve = lineMesh.userData.curve;
      scene.add(traceDot);

      return traceDot;
    });

    function resizeRenderer() {
      const { width, height } = container.getBoundingClientRect();
      renderer.setSize(width, height, false);
      shaderUniforms.resolution.value.set(width, height);
      camera.aspect = width / height;
      camera.updateProjectionMatrix();
    }

    let animationFrameIdentifier = 0;
    const clock = new THREE.Clock();

    function animateScene() {
      const elapsedTime = clock.getElapsedTime();
      shaderUniforms.elapsedTime.value = elapsedTime;
      const pulse = 1 + Math.sin(elapsedTime * 1.7) * 0.018;
      backPlate.scale.set(1.05 * pulse, 1.05 * pulse, 1);
      orangePlate.material.opacity = 0.18 + Math.sin(elapsedTime * 1.4) * 0.045;
      orangeLight.intensity = 4.8 + Math.sin(elapsedTime * 1.2) * 0.8;

      traceDots.forEach((traceDot, traceIndex) => {
        const progress = (elapsedTime * 0.12 + traceIndex * 0.28) % 1;
        traceDot.position.copy(traceDot.userData.curve.getPointAt(progress));
        traceDot.material.opacity = progress > 0.04 && progress < 0.94 ? 0.92 : 0;
      });

      renderer.render(scene, camera);
      animationFrameIdentifier = window.requestAnimationFrame(animateScene);
    }

    resizeRenderer();
    animateScene();
    window.addEventListener("resize", resizeRenderer);

    return () => {
      window.cancelAnimationFrame(animationFrameIdentifier);
      window.removeEventListener("resize", resizeRenderer);
      renderer.dispose();
      scene.traverse((sceneObject) => {
        if (sceneObject.geometry) {
          sceneObject.geometry.dispose();
        }

        if (sceneObject.material) {
          if (Array.isArray(sceneObject.material)) {
            sceneObject.material.forEach((material) => material.dispose());
          } else {
            sceneObject.material.dispose();
          }
        }
      });
      container.removeChild(renderer.domElement);
    };
  }, []);

  return <div className="webgl-workflow-scene" ref={containerReference} aria-hidden="true" />;
}
