import * as THREE from "three";
import initWasm, { hello } from "tec11-wasm";

let wasmInitialized = false;

function startCanvas3DFallback(canvas, width, height, geometry, planetMesh, camera) {
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext("2d");
  const posAttr = geometry.attributes.position;
  const lightDir = new THREE.Vector3(5, 4, 6).normalize();

  const vA = new THREE.Vector3();
  const vB = new THREE.Vector3();
  const vC = new THREE.Vector3();
  const pA = new THREE.Vector3();
  const pB = new THREE.Vector3();
  const pC = new THREE.Vector3();
  const edge1 = new THREE.Vector3();
  const edge2 = new THREE.Vector3();
  const normal = new THREE.Vector3();
  const toCam = new THREE.Vector3();

  let frameId = 0;
  const renderFrame = () => {
    frameId = requestAnimationFrame(renderFrame);
    planetMesh.rotation.y += 0.008;
    planetMesh.rotation.x += 0.003;
    planetMesh.updateMatrixWorld(true);
    camera.updateMatrixWorld(true);

    ctx.fillStyle = "#0b1021";
    ctx.fillRect(0, 0, width, height);

    const faces = [];
    for (let i = 0; i < posAttr.count; i += 3) {
      vA.fromBufferAttribute(posAttr, i).applyMatrix4(planetMesh.matrixWorld);
      vB.fromBufferAttribute(posAttr, i + 1).applyMatrix4(planetMesh.matrixWorld);
      vC.fromBufferAttribute(posAttr, i + 2).applyMatrix4(planetMesh.matrixWorld);

      edge1.subVectors(vB, vA);
      edge2.subVectors(vC, vA);
      normal.crossVectors(edge1, edge2).normalize();

      toCam.subVectors(camera.position, vA).normalize();
      if (normal.dot(toCam) <= 0) {
        continue;
      }

      const diffuse = Math.max(0, normal.dot(lightDir));
      const intensity = Math.min(1, 0.28 + 0.72 * diffuse);

      pA.copy(vA).project(camera);
      pB.copy(vB).project(camera);
      pC.copy(vC).project(camera);

      const depth = (pA.z + pB.z + pC.z) / 3;
      faces.push({
        ax: (pA.x * 0.5 + 0.5) * width,
        ay: (-pA.y * 0.5 + 0.5) * height,
        bx: (pB.x * 0.5 + 0.5) * width,
        by: (-pB.y * 0.5 + 0.5) * height,
        cx: (pC.x * 0.5 + 0.5) * width,
        cy: (-pC.y * 0.5 + 0.5) * height,
        depth,
        intensity,
      });
    }

    faces.sort((a, b) => b.depth - a.depth);

    for (const f of faces) {
      const r = Math.round(43 * f.intensity);
      const g = Math.round(108 * f.intensity);
      const b = Math.round(176 * f.intensity);
      ctx.beginPath();
      ctx.moveTo(f.ax, f.ay);
      ctx.lineTo(f.bx, f.by);
      ctx.lineTo(f.cx, f.cy);
      ctx.closePath();
      ctx.fillStyle = `rgb(${r}, ${g}, ${b})`;
      ctx.fill();
      ctx.strokeStyle = "rgba(144, 205, 244, 0.45)";
      ctx.lineWidth = 1;
      ctx.stroke();
    }
  };

  renderFrame();
  return () => cancelAnimationFrame(frameId);
}

export const initWasmAndScene = (onReady) => () => {
  initWasm({ module_or_path: "./tec11_wasm_bg.wasm" })
    .then(() => {
      wasmInitialized = true;
      onReady();
    })
    .catch((err) => {
      console.error("Failed to initialize tec11_wasm:", err);
    });

  const canvas = document.getElementById("three-canvas");
  if (!canvas) {
    return () => {};
  }

  const width = 520;
  const height = 420;
  const scene = new THREE.Scene();
  scene.background = new THREE.Color(0x0b1021);

  const camera = new THREE.PerspectiveCamera(45, width / height, 0.1, 100);
  camera.position.set(0, 0.6, 4.5);
  camera.lookAt(0, 0, 0);

  const geometry = new THREE.IcosahedronGeometry(1.45, 2);
  const material = new THREE.MeshStandardMaterial({
    color: 0x2b6cb0,
    roughness: 0.45,
    metalness: 0.15,
    flatShading: true,
  });
  const planetMesh = new THREE.Mesh(geometry, material);
  scene.add(planetMesh);

  let renderer = null;
  try {
    renderer = new THREE.WebGLRenderer({ canvas, antialias: true });
    renderer.setSize(width, height, false);
    renderer.setPixelRatio(window.devicePixelRatio || 1);
  } catch (err) {
    console.warn("WebGL context unavailable, using Three.js 3D projection renderer:", err);
    const stopFallback = startCanvas3DFallback(
      canvas,
      width,
      height,
      geometry,
      planetMesh,
      camera
    );
    return () => {
      stopFallback();
      geometry.dispose();
      material.dispose();
    };
  }

  const wireframeGeometry = new THREE.WireframeGeometry(geometry);
  const wireframeMaterial = new THREE.LineBasicMaterial({
    color: 0x90cdf4,
    transparent: true,
    opacity: 0.45,
  });
  const wireframe = new THREE.LineSegments(wireframeGeometry, wireframeMaterial);
  planetMesh.add(wireframe);

  const ambientLight = new THREE.AmbientLight(0xffffff, 0.55);
  scene.add(ambientLight);

  const dirLight = new THREE.DirectionalLight(0xffffff, 1.3);
  dirLight.position.set(5, 4, 6);
  scene.add(dirLight);

  let frameId = 0;
  const animate = () => {
    frameId = requestAnimationFrame(animate);
    planetMesh.rotation.y += 0.008;
    planetMesh.rotation.x += 0.003;
    renderer.render(scene, camera);
  };
  animate();

  return () => {
    cancelAnimationFrame(frameId);
    geometry.dispose();
    material.dispose();
    wireframeGeometry.dispose();
    wireframeMaterial.dispose();
    renderer.dispose();
  };
};

export const callRustHello = (name) => () => {
  if (!wasmInitialized) {
    return "Rust Wasm module is still initializing...";
  }
  return hello(name);
};
