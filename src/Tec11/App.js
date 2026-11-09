import * as THREE from "three";
import initWasmModule, { WasmGridSession, WasmPangeaSession } from "tec11-wasm";

const _yAxis = new THREE.Vector3(0, 1, 0);

let activeView = null;

/**
 * Frontend colormap for Grid mode: maps per-tile unit sphere coordinates (x, y, z)
 * in [-1, 1] to per-vertex RGB colors (r/g/b = (x+1)/2, (y+1)/2, (z+1)/2).
 */
function computePositionVertexColors(tileCenters, vertexTileIds) {
  const numVerts = vertexTileIds.length;
  const colors = new Float32Array(numVerts * 3);
  for (let v = 0; v < numVerts; v++) {
    const tid = vertexTileIds[v];
    const base = tid * 3;
    const x = tileCenters[base];
    const y = tileCenters[base + 1];
    const z = tileCenters[base + 2];
    const out = v * 3;
    colors[out] = 0.5 * (x + 1.0);
    colors[out + 1] = 0.5 * (y + 1.0);
    colors[out + 2] = 0.5 * (z + 1.0);
  }
  return colors;
}

/**
 * Frontend colormaps for Pangea mode:
 * - "color": Water (elevation <= 0) is ocean blue rgb(0, 0, 128).
 *            Land rock (elevation > 0) is colored by silica/felsity f in [0.5, 1.0]:
 *            rgb(128, 64 + 64*f, 128*f).
 * - "elevation": Land (elevation > 0) is upper greyscale [142..255].
 *                Ocean floor bathymetry (elevation <= 0, -5.6..0 km) is lower greyscale [18..112].
 */
function computePangeaVertexColors(elevations, felsities, vertexTileIds, renderMode) {
  const numVerts = vertexTileIds.length;
  const colors = new Float32Array(numVerts * 3);
  const isElevation = renderMode === "elevation";

  for (let v = 0; v < numVerts; v++) {
    const tid = vertexTileIds[v];
    const h = elevations[tid];
    const out = v * 3;

    if (isElevation) {
      if (h > 0.0) {
        const val = Math.min(255.0, 142.0 + 96.0 * h) / 255.0;
        colors[out] = val;
        colors[out + 1] = val;
        colors[out + 2] = val;
      } else {
        const norm = Math.max(0.0, Math.min(1.0, (h + 5.6) / 5.6));
        const val = (18.0 + 94.0 * norm) / 255.0;
        colors[out] = val;
        colors[out + 1] = val;
        colors[out + 2] = val;
      }
    } else {
      if (h <= 0.0) {
        colors[out] = 0.0;
        colors[out + 1] = 0.0;
        colors[out + 2] = 128.0 / 255.0;
      } else {
        const f = felsities[tid];
        colors[out] = 128.0 / 255.0;
        colors[out + 1] = (64.0 + 64.0 * f) / 255.0;
        colors[out + 2] = (128.0 * f) / 255.0;
      }
    }
  }
  return colors;
}

/**
 * Extracts the outer polygon border segments (v1 -> v2 of each fan triangle).
 */
function extractBorderPositions(positions) {
  const numTris = positions.length / 9;
  const border = new Float32Array(numTris * 6);
  for (let t = 0; t < numTris; t++) {
    const src = t * 9;
    const dst = t * 6;
    border[dst] = positions[src + 3];
    border[dst + 1] = positions[src + 4];
    border[dst + 2] = positions[src + 5];
    border[dst + 3] = positions[src + 6];
    border[dst + 4] = positions[src + 7];
    border[dst + 5] = positions[src + 8];
  }
  return border;
}

function paintCanvasFallback(view) {
  const { canvas, width, height, geometry, mesh, camera, showBorders } = view;
  const ctx = canvas.getContext("2d");
  if (!ctx) return;

  mesh.updateMatrixWorld(true);
  camera.updateMatrixWorld(true);

  ctx.fillStyle = "#050505";
  ctx.fillRect(0, 0, width, height);

  const posAttr = geometry.attributes.position;
  const normAttr = geometry.attributes.normal;
  const colAttr = geometry.attributes.color;
  if (!posAttr || !colAttr) return;

  const vA = new THREE.Vector3();
  const vB = new THREE.Vector3();
  const vC = new THREE.Vector3();
  const nVec = new THREE.Vector3();
  const pA = new THREE.Vector3();
  const pB = new THREE.Vector3();
  const pC = new THREE.Vector3();
  const lightDir = new THREE.Vector3(0.25, 0.75, 1.0).normalize();
  const rotMatrix = new THREE.Matrix3().getNormalMatrix(mesh.matrixWorld);

  const tris = [];
  for (let i = 0; i < posAttr.count; i += 3) {
    nVec.fromBufferAttribute(normAttr, i).applyMatrix3(rotMatrix).normalize();
    if (nVec.z <= -0.05) {
      continue;
    }

    vA.fromBufferAttribute(posAttr, i).applyMatrix4(mesh.matrixWorld);
    vB.fromBufferAttribute(posAttr, i + 1).applyMatrix4(mesh.matrixWorld);
    vC.fromBufferAttribute(posAttr, i + 2).applyMatrix4(mesh.matrixWorld);

    pA.copy(vA).project(camera);
    pB.copy(vB).project(camera);
    pC.copy(vC).project(camera);

    const shade = 0.55 + 0.45 * Math.max(0, nVec.dot(lightDir));
    const r = Math.min(255, Math.round(colAttr.getX(i) * 255 * shade));
    const g = Math.min(255, Math.round(colAttr.getY(i) * 255 * shade));
    const b = Math.min(255, Math.round(colAttr.getZ(i) * 255 * shade));

    tris.push({
      ax: (pA.x * 0.5 + 0.5) * width,
      ay: (-pA.y * 0.5 + 0.5) * height,
      bx: (pB.x * 0.5 + 0.5) * width,
      by: (-pB.y * 0.5 + 0.5) * height,
      cx: (pC.x * 0.5 + 0.5) * width,
      cy: (-pC.y * 0.5 + 0.5) * height,
      depth: (pA.z + pB.z + pC.z) / 3,
      fill: `rgb(${r}, ${g}, ${b})`,
    });
  }

  tris.sort((a, b) => b.depth - a.depth);

  for (const t of tris) {
    ctx.beginPath();
    ctx.moveTo(t.ax, t.ay);
    ctx.lineTo(t.bx, t.by);
    ctx.lineTo(t.cx, t.cy);
    ctx.closePath();
    ctx.fillStyle = t.fill;
    ctx.fill();
    ctx.strokeStyle = t.fill;
    ctx.lineWidth = 0.75;
    ctx.stroke();

    if (showBorders) {
      ctx.beginPath();
      ctx.moveTo(t.bx, t.by);
      ctx.lineTo(t.cx, t.cy);
      ctx.strokeStyle = "rgba(10, 14, 24, 0.45)";
      ctx.lineWidth = 1.0;
      ctx.stroke();
    }
  }
}

function createBaseSphereView(canvas, showBorders) {
  const width = 460;
  const height = 460;

  const camera = new THREE.OrthographicCamera(-1.1, 1.1, 1.1, -1.1, 0, 11);
  camera.position.z = 11;

  const scene = new THREE.Scene();
  scene.background = new THREE.Color(0x050505);

  const light = new THREE.HemisphereLight(0xffffff, 0x444444, 2.8);
  scene.add(light);

  const geometry = new THREE.BufferGeometry();
  const material = new THREE.MeshPhongMaterial({
    side: THREE.FrontSide,
    vertexColors: true,
    flatShading: true,
  });
  const mesh = new THREE.Mesh(geometry, material);
  scene.add(mesh);

  let borderGeometry = null;
  let borderMaterial = null;
  if (showBorders) {
    borderGeometry = new THREE.BufferGeometry();
    borderMaterial = new THREE.LineBasicMaterial({
      color: 0x0a0e18,
      transparent: true,
      opacity: 0.45,
    });
    const borderLines = new THREE.LineSegments(borderGeometry, borderMaterial);
    mesh.add(borderLines);
  }

  let renderer = null;
  try {
    renderer = new THREE.WebGLRenderer({ canvas, antialias: true });
    renderer.setPixelRatio(window.devicePixelRatio || 1);
    renderer.setSize(width, height, false);
  } catch (err) {
    canvas.width = width;
    canvas.height = height;
  }

  return {
    canvas,
    width,
    height,
    showBorders,
    camera,
    scene,
    geometry,
    material,
    borderGeometry,
    borderMaterial,
    mesh,
    renderer,
    paint() {
      if (this.renderer) {
        this.renderer.render(this.scene, this.camera);
      } else {
        paintCanvasFallback(this);
      }
    },
    dispose() {
      geometry.dispose();
      material.dispose();
      if (borderGeometry) borderGeometry.dispose();
      if (borderMaterial) borderMaterial.dispose();
      if (renderer) renderer.dispose();
    },
  };
}

function updateGridViewBuffers(view) {
  const { session, geometry, borderGeometry } = view;
  const positions = session.mesh_positions();
  const normals = session.mesh_normals();
  const vertexTileIds = session.vertex_tile_ids();
  const tileCenters = session.tile_centers();

  const colors = computePositionVertexColors(tileCenters, vertexTileIds);

  geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
  geometry.setAttribute("normal", new THREE.Float32BufferAttribute(normals, 3));
  geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
  geometry.computeBoundingSphere();

  if (borderGeometry) {
    const borderPos = extractBorderPositions(positions);
    borderGeometry.setAttribute("position", new THREE.Float32BufferAttribute(borderPos, 3));
  }

  view.paint();
}

function updatePangeaViewBuffers(view, rebuildGeometry) {
  const { session, geometry } = view;
  if (rebuildGeometry) {
    const positions = session.mesh_positions();
    const normals = session.mesh_normals();
    view.vertexTileIds = session.vertex_tile_ids();
    geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
    geometry.setAttribute("normal", new THREE.Float32BufferAttribute(normals, 3));
    geometry.computeBoundingSphere();
  }

  view.elevations = session.tile_elevations();
  view.felsities = session.tile_felsities();

  const colors = computePangeaVertexColors(
    view.elevations,
    view.felsities,
    view.vertexTileIds,
    view.renderMode
  );
  geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
  view.paint();
}

export const initWasm = (onReady) => () => {
  initWasmModule({ module_or_path: "./tec11_wasm_bg.wasm" })
    .then(() => {
      onReady();
    })
    .catch((err) => {
      console.error("Failed to initialize tec11_wasm:", err);
    });
};

export const mountGridView = (setGridInfo) => () => {
  const canvas = document.getElementById("sphere-canvas");
  if (!canvas) {
    return () => {};
  }

  const view = createBaseSphereView(canvas, true);
  view.session = new WasmGridSession();

  activeView = view;
  updateGridViewBuffers(view);
  setGridInfo({
    size: view.session.size(),
    tileCount: view.session.tile_count(),
  })();

  return () => {
    if (activeView === view) {
      activeView = null;
    }
    view.dispose();
    view.session.free();
  };
};

export const subdivideGrid = (setGridInfo) => () => {
  if (!activeView || !activeView.session) return;
  activeView.session.subdivide();
  updateGridViewBuffers(activeView);
  setGridInfo({
    size: activeView.session.size(),
    tileCount: activeView.session.tile_count(),
  })();
};

export const mountPangeaView = (gridSize) => (landPct) => (renderMode) => (setPangeaInfo) => () => {
  const canvas = document.getElementById("sphere-canvas");
  if (!canvas) {
    return () => {};
  }

  const view = createBaseSphereView(canvas, false);
  const seed = BigInt(Date.now());
  view.session = new WasmPangeaSession(gridSize, landPct, seed);
  view.currentGridSize = gridSize;
  view.renderMode = renderMode;

  activeView = view;
  updatePangeaViewBuffers(view, true);
  setPangeaInfo({
    gridSize: view.session.grid_size(),
    tileCount: view.session.tile_count(),
    actualLandPercent: view.session.land_percent_actual(),
  })();

  return () => {
    if (activeView === view) {
      activeView = null;
    }
    view.dispose();
    view.session.free();
  };
};

export const regeneratePangea = (gridSize) => (landPct) => (setPangeaInfo) => () => {
  if (!activeView || !activeView.session) return;
  const sizeChanged = activeView.currentGridSize !== gridSize;
  activeView.session.regenerate(gridSize, landPct);
  activeView.currentGridSize = gridSize;
  updatePangeaViewBuffers(activeView, sizeChanged);
  setPangeaInfo({
    gridSize: activeView.session.grid_size(),
    tileCount: activeView.session.tile_count(),
    actualLandPercent: activeView.session.land_percent_actual(),
  })();
};

export const setPangeaRenderMode = (renderMode) => () => {
  if (!activeView || !activeView.elevations) return;
  activeView.renderMode = renderMode;
  const colors = computePangeaVertexColors(
    activeView.elevations,
    activeView.felsities,
    activeView.vertexTileIds,
    activeView.renderMode
  );
  activeView.geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
  activeView.paint();
};

export const rotateGrid = (deg) => () => {
  if (!activeView) return;
  const angleRad = (deg * Math.PI) / 180.0;
  activeView.mesh.setRotationFromAxisAngle(_yAxis, angleRad);
  activeView.paint();
};
