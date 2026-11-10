import * as THREE from "three";
import initWasmModule, {
  WasmClimateSession,
  WasmGridSession,
  WasmPangeaSession,
  WasmTectonicsSession,
} from "tec11-wasm";

const _yAxis = new THREE.Vector3(0, 1, 0);

let activeView = null;
let savedSnapshotBytes = null;
let activeClimateSnapshotBytes = null;

const SAVED_PLANET_B64_KEY = "tec11_saved_planet_pb_b64";
const IDB_NAME = "tec11_db";
const IDB_STORE = "snapshots";
const IDB_LATEST_KEY = "latest_tectonics";

function uint8ToBase64(bytes) {
  let binary = "";
  const chunkSize = 0x8000;
  for (let i = 0; i < bytes.length; i += chunkSize) {
    binary += String.fromCharCode.apply(null, bytes.subarray(i, i + chunkSize));
  }
  return window.btoa(binary);
}

function base64ToUint8(b64) {
  const binary = window.atob(b64);
  const out = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    out[i] = binary.charCodeAt(i);
  }
  return out;
}

function saveToIndexedDb(bytes) {
  try {
    if (!window.indexedDB) return;
    const req = window.indexedDB.open(IDB_NAME, 1);
    req.onupgradeneeded = () => {
      const db = req.result;
      if (!db.objectStoreNames.contains(IDB_STORE)) {
        db.createObjectStore(IDB_STORE);
      }
    };
    req.onsuccess = () => {
      const db = req.result;
      const tx = db.transaction(IDB_STORE, "readwrite");
      tx.objectStore(IDB_STORE).put(bytes, IDB_LATEST_KEY);
    };
  } catch (_err) {
    // Ignore IndexedDB errors
  }
}

function getSavedSnapshotBytes() {
  if (savedSnapshotBytes && savedSnapshotBytes.length > 0) {
    return savedSnapshotBytes;
  }
  try {
    const b64 = window.localStorage.getItem(SAVED_PLANET_B64_KEY);
    if (b64) {
      const bytes = base64ToUint8(b64);
      if (bytes.length > 0) {
        savedSnapshotBytes = bytes;
        return bytes;
      }
    }
  } catch (_err) {
    // Ignore localStorage read errors
  }
  return null;
}

function storeSavedSnapshotBytes(bytes) {
  const copy = new Uint8Array(bytes);
  savedSnapshotBytes = copy;
  saveToIndexedDb(copy);
  try {
    window.localStorage.setItem(SAVED_PLANET_B64_KEY, uint8ToBase64(copy));
  } catch (_err) {
    // Ignore localStorage quota/write errors
  }
}

function triggerBinaryDownload(filename, bytes) {
  try {
    const blob = new Blob([bytes], { type: "application/octet-stream" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  } catch (_err) {
    // Ignore download errors in headless environments
  }
}

function pickBinarySnapshotFile(onBytesLoaded) {
  const input = document.createElement("input");
  input.type = "file";
  input.accept = ".tec11";
  input.style.display = "none";
  input.addEventListener("change", () => {
    const file = input.files && input.files[0];
    if (!file) {
      document.body.removeChild(input);
      return;
    }
    const reader = new FileReader();
    reader.onload = () => {
      if (reader.result instanceof ArrayBuffer) {
        onBytesLoaded(new Uint8Array(reader.result));
      }
      document.body.removeChild(input);
    };
    reader.onerror = () => {
      document.body.removeChild(input);
    };
    reader.readAsArrayBuffer(file);
  });
  document.body.appendChild(input);
  input.click();
}

/**
 * Köppen classification RGB palette matching `koeppencolor.py` (in [0..255]).
 * Index 0 is Ocean (0, 0, 0).
 */
const KOEPPEN_PALETTE_RGB = [
  [0, 0, 0], // 0: Ocean
  [0, 0, 255], // 1: Af
  [0, 63, 255], // 2: Am
  [0, 127, 255], // 3: Aw
  [255, 127, 0], // 4: BS
  [255, 0, 0], // 5: BW
  [0, 255, 0], // 6: Cf
  [255, 255, 0], // 7: Cs
  [127, 255, 0], // 8: Cw
  [0, 255, 255], // 9: Df
  [255, 0, 255], // 10: Ds
  [127, 127, 255], // 11: Dw
  [127, 127, 127], // 12: EF
  [191, 191, 191], // 13: ET
];

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
 * Frontend colormaps for Tectonics mode matching `planetdisplay.py`:
 * - "elevation": Land greyscale `min(255, 128 + 12.5 * h)` (above 140), ocean bathymetry `18..112`.
 * - "color": Ocean blue `rgb(0, 0, 128)`; land rock felsity `rgb(128, 64+64*f, 128*f)` blended 50/50
 *            with `rgb(0, 255*life, 0)` when atmosphere is active (or snow white `rgb(255, 255, 255)` for ET/EF).
 * - "climate": Köppen palette on land when atmosphere is active (white before atmosphere), black on ocean.
 * - "mountains": Elevation greyscale with red channel boosted by normalized `mountainosity`:
 *                `r = c + m * (255 - c)`.
 * - "rock": Top layer RockKind: Igneous `'I'` = `(255, val, val)`, Sedimentary `'S'` = `(val, val, 255)`,
 *           Metamorphic `'M'` = `(val, 255, val)`.
 * - "subduction": Elevation greyscale with active subduction tiles highlighted in red `(255, val, val)`.
 * - "thickness": Crustal thickness greyscale `min(255, 128 + 1.69 * thickness)` on land,
 *                and `24 + 14 * thickness` on ocean crust.
 */
function computeTectonicsVertexColors(view) {
  const {
    vertexTileIds,
    elevations,
    thicknesses,
    mountainosities,
    subductions,
    felsities,
    topRockKinds,
    koeppenIds,
    tileLife,
    hasAtmosphere,
    aspect,
    selectedTileIdx,
  } = view;
  const numVerts = vertexTileIds.length;
  const colors = new Float32Array(numVerts * 3);

  for (let v = 0; v < numVerts; v++) {
    const tid = vertexTileIds[v];
    const h = elevations[tid];
    const out = v * 3;
    let r = 0.0;
    let g = 0.0;
    let b = 0.0;

    if (aspect === "elevation") {
      if (h > 0.0) {
        const val = Math.min(255.0, 134.0 + 18.0 * h) / 255.0;
        r = val;
        g = val;
        b = val;
      } else {
        const norm = Math.max(0.0, Math.min(1.0, (h + 7.5) / 7.5));
        const val = (18.0 + 88.0 * norm) / 255.0;
        r = val;
        g = val;
        b = val;
      }
    } else if (aspect === "color") {
      if (h <= 0.0) {
        r = 0.0;
        g = 0.0;
        b = 128.0 / 255.0;
      } else {
        const f = felsities[tid];
        const rockR = 128.0;
        const rockG = 64.0 + 64.0 * f;
        const rockB = 128.0 * f;
        const kId = koeppenIds[tid];
        if (!hasAtmosphere || kId === 0) {
          r = rockR / 255.0;
          g = rockG / 255.0;
          b = rockB / 255.0;
        } else if (kId === 12 || kId === 13) {
          // Polar ice / tundra snow (`planetdisplay.py:64`)
          r = 1.0;
          g = 1.0;
          b = 1.0;
        } else {
          // 50/50 blend of rock color and vegetation green (`planetdisplay.py:66`)
          const lifeGreen = 255.0 * tileLife[tid];
          r = (0.5 * rockR) / 255.0;
          g = (0.5 * (rockG + lifeGreen)) / 255.0;
          b = (0.5 * rockB) / 255.0;
        }
      }
    } else if (aspect === "climate") {
      if (h > 0.0) {
        if (!hasAtmosphere) {
          r = 1.0;
          g = 1.0;
          b = 1.0;
        } else {
          const idx = Math.round(koeppenIds[tid]);
          const rgb = KOEPPEN_PALETTE_RGB[idx] || [255, 255, 255];
          r = rgb[0] / 255.0;
          g = rgb[1] / 255.0;
          b = rgb[2] / 255.0;
        }
      }
    } else if (aspect === "mountains") {
      if (h > 0.0) {
        const base = Math.min(255.0, 128.0 + 12.5 * h);
        const m = mountainosities[tid];
        r = (base + m * (255.0 - base)) / 255.0;
        g = base / 255.0;
        b = base / 255.0;
      } else {
        const norm = Math.max(0.0, Math.min(1.0, (h + 7.5) / 7.5));
        const val = (16.0 + 54.0 * norm) / 255.0;
        r = val;
        g = val;
        b = val;
      }
    } else if (aspect === "rock") {
      const kind = topRockKinds[tid];
      if (h > 0.0) {
        const val = Math.min(255.0, 128.0 + 12.5 * h) / 255.0;
        if (kind === 0) {
          // Igneous ('I'): red-tinted
          r = 1.0;
          g = val;
          b = val;
        } else if (kind === 1) {
          // Sedimentary ('S'): blue-tinted
          r = val;
          g = val;
          b = 1.0;
        } else {
          // Metamorphic ('M'): green-tinted
          r = val;
          g = 1.0;
          b = val;
        }
      } else {
        // Subtle ocean floor rock tint (basalt/gabbro vs marine sediment)
        const norm = Math.max(0.0, Math.min(1.0, (h + 7.5) / 7.5));
        const base = (18.0 + 42.0 * norm) / 255.0;
        if (kind === 0) {
          r = base + 0.14;
          g = base;
          b = base;
        } else if (kind === 1) {
          r = base;
          g = base;
          b = base + 0.16;
        } else {
          r = base;
          g = base + 0.14;
          b = base;
        }
      }
    } else if (aspect === "subduction") {
      const sub = subductions[tid];
      if (h > 0.0) {
        const val = Math.min(255.0, 128.0 + 12.5 * h) / 255.0;
        r = sub > 0.0 ? 1.0 : val;
        g = val;
        b = val;
      } else {
        const norm = Math.max(0.0, Math.min(1.0, (h + 7.5) / 7.5));
        const val = (18.0 + 64.0 * norm) / 255.0;
        r = sub > 0.0 ? 0.85 : val;
        g = val;
        b = val;
      }
    } else if (aspect === "thickness") {
      const th = thicknesses[tid];
      if (h > 0.0) {
        const val = Math.min(255.0, 128.0 + 1.69 * th) / 255.0;
        r = val;
        g = val;
        b = val;
      } else {
        const val = Math.min(115.0, 18.0 + 10.0 * th) / 255.0;
        r = val;
        g = val;
        b = val;
      }
    }

    if (tid === selectedTileIdx) {
      // Highlight selected tile with a warm golden-yellow tint
      r = Math.min(1.0, r * 0.55 + 0.45);
      g = Math.min(1.0, g * 0.55 + 0.42);
      b = Math.min(1.0, b * 0.4);
    }

    colors[out] = r;
    colors[out + 1] = g;
    colors[out + 2] = b;
  }

  return colors;
}

/**
 * Frontend colormaps for Climate mode matching `climatedisplay.py` + `"ocean_currents"`:
 * - "koeppen": Land tiles colored by Köppen classification (`koeppencolor.py`), ocean black (0, 0, 0).
 * - "ocean_currents": Ocean tiles colored by SST Anomaly (Warm Red/Orange = poleward western
 *   boundary currents, Cold Cyan/Blue = equatorward currents & upwelling, Deep Navy = calm),
 *   with continents in muted dark slate and 3D flow arrows overlaid on ocean tiles.
 * - "insolation": Land tiles colored yellow-white (`r=255, g=255*min(1, s/0.5), b=255*max(0, (s-0.5)/0.5)`).
 * - "precipitation": Land tiles colored greyscale (`128 + 127 * s`).
 * - "temperature": Land tiles colored blue-to-red (`r=255*min(1, s/0.5), g=0, b=255*min(1, (1-s)/0.5)`).
 */
function computeClimateVertexColors(view) {
  const { session, elevations, vertexTileIds, attribute, seasonSlider } = view;
  const numVerts = vertexTileIds.length;
  const colors = new Float32Array(numVerts * 3);
  const seasonCount = session.season_count();
  const seasonIdx = Math.min(
    seasonCount - 1,
    Math.max(0, Math.round(((seasonCount - 1) * seasonSlider) / 100.0))
  );

  if (attribute === "koeppen") {
    const koeppenIds = view.koeppenIds;
    for (let v = 0; v < numVerts; v++) {
      const tid = vertexTileIds[v];
      const out = v * 3;
      if (elevations[tid] > 0.0) {
        const idx = Math.round(koeppenIds[tid]);
        const rgb = KOEPPEN_PALETTE_RGB[idx] || KOEPPEN_PALETTE_RGB[0];
        colors[out] = rgb[0] / 255.0;
        colors[out + 1] = rgb[1] / 255.0;
        colors[out + 2] = rgb[2] / 255.0;
      } else {
        colors[out] = 0.0;
        colors[out + 1] = 0.0;
        colors[out + 2] = 0.0;
      }
    }
  } else if (attribute === "ocean_currents") {
    const sstAnomaly = view.seasonalSstAnomaly[seasonIdx];
    const oceanSpeed = view.seasonalOceanSpeed[seasonIdx];
    for (let v = 0; v < numVerts; v++) {
      const tid = vertexTileIds[v];
      const out = v * 3;
      if (elevations[tid] > 0.0) {
        // Muted dark slate for continents so ocean currents stand out clearly
        colors[out] = 36.0 / 255.0;
        colors[out + 1] = 42.0 / 255.0;
        colors[out + 2] = 52.0 / 255.0;
      } else {
        const anom = sstAnomaly[tid];
        const spd = oceanSpeed[tid];
        const baseR = 14.0;
        const baseG = 30.0 + 18.0 * spd;
        const baseB = 72.0 + 25.0 * spd;

        if (anom >= 0.0) {
          const t = Math.min(1.0, Math.pow(anom, 0.85));
          // Deep navy -> vibrant crimson/red -> bright warm orange-gold (suppressing blue quickly so it never muddies)
          const r = baseR + (255.0 - baseR) * Math.min(1.0, t * 1.35);
          const g = t < 0.45 ? baseG * (1.0 - t) + 45.0 * t : 35.0 + 145.0 * ((t - 0.45) / 0.55);
          const b = baseB * Math.pow(1.0 - t, 1.8);
          colors[out] = r / 255.0;
          colors[out + 1] = g / 255.0;
          colors[out + 2] = b / 255.0;
        } else {
          const t = Math.min(1.0, -anom * 1.25);
          // Deep navy -> bright marine cyan -> icy turquoise
          const targetR = t > 0.5 ? 110.0 * ((t - 0.5) / 0.5) : 0.0;
          const targetG = 210.0 + 35.0 * t;
          const targetB = 255.0;
          colors[out] = (baseR + (targetR - baseR) * t) / 255.0;
          colors[out + 1] = (baseG + (targetG - baseG) * t) / 255.0;
          colors[out + 2] = (baseB + (targetB - baseB) * t) / 255.0;
        }
      }
    }
  } else if (attribute === "insolation") {
    const sun = session.tile_insolation(seasonIdx);
    for (let v = 0; v < numVerts; v++) {
      const tid = vertexTileIds[v];
      const out = v * 3;
      if (elevations[tid] > 0.0) {
        const s = sun[tid];
        const r = 1.0;
        const g = Math.max(0.0, Math.min(1.0, s >= 0.5 ? 1.0 : s / 0.5));
        const b = Math.max(0.0, Math.min(1.0, s < 0.5 ? 0.0 : (s - 0.5) / 0.5));
        colors[out] = r;
        colors[out + 1] = g;
        colors[out + 2] = b;
      }
    }
  } else if (attribute === "precipitation") {
    const precip = session.tile_precipitation(seasonIdx);
    for (let v = 0; v < numVerts; v++) {
      const tid = vertexTileIds[v];
      const out = v * 3;
      if (elevations[tid] > 0.0) {
        const s = precip[tid];
        const val = Math.max(0.0, Math.min(255.0, 128.0 + 127.0 * s)) / 255.0;
        colors[out] = val;
        colors[out + 1] = val;
        colors[out + 2] = val;
      }
    }
  } else if (attribute === "temperature") {
    const temp = session.tile_temperature(seasonIdx);
    for (let v = 0; v < numVerts; v++) {
      const tid = vertexTileIds[v];
      const out = v * 3;
      if (elevations[tid] > 0.0) {
        const s = temp[tid];
        const r = Math.max(0.0, Math.min(1.0, s >= 0.5 ? 1.0 : s / 0.5));
        const b = Math.max(0.0, Math.min(1.0, s < 0.5 ? 1.0 : (1.0 - s) / 0.5));
        colors[out] = r;
        colors[out + 1] = 0.0;
        colors[out + 2] = b;
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
  const { canvas, width, height, geometry, mesh, camera, showBorders, arrowGeometry, arrowLines } =
    view;
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

  // Draw 3D ocean current arrows if visible
  if (arrowLines && arrowLines.visible && arrowGeometry) {
    const arrowPos = arrowGeometry.attributes.position;
    if (arrowPos) {
      ctx.strokeStyle = "rgba(248, 250, 252, 0.78)";
      ctx.lineWidth = 1.1;
      ctx.beginPath();
      for (let i = 0; i < arrowPos.count; i += 2) {
        vA.fromBufferAttribute(arrowPos, i).applyMatrix4(mesh.matrixWorld);
        vB.fromBufferAttribute(arrowPos, i + 1).applyMatrix4(mesh.matrixWorld);
        if (vA.z <= 0.02 && vB.z <= 0.02) {
          continue;
        }
        pA.copy(vA).project(camera);
        pB.copy(vB).project(camera);
        const x0 = (pA.x * 0.5 + 0.5) * width;
        const y0 = (-pA.y * 0.5 + 0.5) * height;
        const x1 = (pB.x * 0.5 + 0.5) * width;
        const y1 = (-pB.y * 0.5 + 0.5) * height;
        ctx.moveTo(x0, y0);
        ctx.lineTo(x1, y1);
      }
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

  const arrowGeometry = new THREE.BufferGeometry();
  const arrowMaterial = new THREE.LineBasicMaterial({
    color: 0xf8fafc,
    transparent: true,
    opacity: 0.78,
  });
  const arrowLines = new THREE.LineSegments(arrowGeometry, arrowMaterial);
  arrowLines.visible = false;
  mesh.add(arrowLines);

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
    arrowGeometry,
    arrowMaterial,
    arrowLines,
    mesh,
    renderer,
    rotationAngleRad: 0.0,
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
      arrowGeometry.dispose();
      arrowMaterial.dispose();
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

function updateClimateDataBuffers(view) {
  view.koeppenIds = view.session.tile_koeppen_ids();
  const seasonCount = view.session.season_count();
  view.seasonalSstAnomaly = [];
  view.seasonalOceanSpeed = [];
  view.seasonalArrowPos = [];
  for (let s = 0; s < seasonCount; s++) {
    view.seasonalSstAnomaly.push(view.session.tile_ocean_sst_anomaly(s));
    view.seasonalOceanSpeed.push(view.session.tile_ocean_speed(s));
    view.seasonalArrowPos.push(view.session.ocean_arrow_positions(s));
  }
}

function updateClimateViewColors(view) {
  const seasonCount = view.session.season_count();
  const seasonIdx = Math.min(
    seasonCount - 1,
    Math.max(0, Math.round(((seasonCount - 1) * view.seasonSlider) / 100.0))
  );
  view.arrowLines.visible = view.attribute === "ocean_currents";
  if (view.attribute === "ocean_currents" && view.seasonalArrowPos && view.seasonalArrowPos[seasonIdx]) {
    view.arrowGeometry.setAttribute(
      "position",
      new THREE.Float32BufferAttribute(view.seasonalArrowPos[seasonIdx], 3)
    );
  }
  const colors = computeClimateVertexColors(view);
  view.geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
  view.paint();
}

function updateTectonicsViewBuffers(view, rebuildGeometry) {
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
  view.thicknesses = session.tile_thicknesses();
  view.mountainosities = session.tile_mountainosities();
  view.subductions = session.tile_subductions();
  view.felsities = session.tile_felsities();
  view.topRockKinds = session.tile_top_rock_kinds();
  view.koeppenIds = session.tile_koeppen_ids();
  view.tileLife = session.tile_life();
  view.hasAtmosphere = session.has_atmosphere();
  view.hasLife = session.has_life();

  const colors = computeTectonicsVertexColors(view);
  geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
  view.paint();
}

function buildTectonicsInfo(view) {
  const { session, selectedTileIdx, timerId } = view;
  const selectedTileReport =
    selectedTileIdx >= 0
      ? session.inspect_tile(selectedTileIdx)
      : "Click any tile on the globe to inspect its rock layers.";
  return {
    gridSize: session.grid_size(),
    tileCount: session.tile_count(),
    geoticks: session.geoticks(),
    continentCount: session.continent_count(),
    actualLandPercent: session.land_percent_actual(),
    hasAtmosphere: session.has_atmosphere(),
    hasLife: session.has_life(),
    isRunning: timerId !== null,
    selectedTileReport,
  };
}

function stopTectonicsLoop(view) {
  if (view && view.timerId !== null) {
    clearInterval(view.timerId);
    view.timerId = null;
  }
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

function rebuildClimateViewGeometryAndData(view) {
  const positions = view.session.mesh_positions();
  const normals = view.session.mesh_normals();
  view.vertexTileIds = view.session.vertex_tile_ids();
  view.elevations = view.session.tile_elevations();
  updateClimateDataBuffers(view);

  view.geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
  view.geometry.setAttribute("normal", new THREE.Float32BufferAttribute(normals, 3));
  view.geometry.computeBoundingSphere();
  updateClimateViewColors(view);
}

function buildClimateInfo(view) {
  const s = view.session;
  const planetName = s.is_custom_planet()
    ? `Tectonics (${s.loaded_geoticks() * 5} Myr, ${s.loaded_land_percent()}% land)`
    : "Earth (ETOPO2)";
  return {
    planetName,
    tileCount: s.tile_count(),
    hasSavedPlanet: getSavedSnapshotBytes() !== null,
  };
}

export const mountClimateView =
  (cells) => (glaciationPct) => (attribute) => (seasonSlider) => (setClimateInfo) => () => {
    const canvas = document.getElementById("sphere-canvas");
    if (!canvas) {
      return () => {};
    }

    const view = createBaseSphereView(canvas, false);
    view.session = new WasmClimateSession(cells, glaciationPct / 100.0);
    view.attribute = attribute;
    view.seasonSlider = seasonSlider;
    view.setClimateInfo = setClimateInfo;

    if (activeClimateSnapshotBytes) {
      const ok = view.session.load_snapshot_bytes(activeClimateSnapshotBytes);
      if (!ok) {
        activeClimateSnapshotBytes = null;
      }
    }

    activeView = view;
    rebuildClimateViewGeometryAndData(view);
    setClimateInfo(buildClimateInfo(view))();

    return () => {
      if (activeView === view) {
        activeView = null;
      }
      view.dispose();
      view.session.free();
    };
  };

export const setClimateAttribute = (attribute) => () => {
  if (!activeView || !activeView.session) return;
  activeView.attribute = attribute;
  updateClimateViewColors(activeView);
};

export const setClimateSeason = (seasonSlider) => () => {
  if (!activeView || !activeView.session) return;
  activeView.seasonSlider = seasonSlider;
  updateClimateViewColors(activeView);
};

export const setClimateParams = (cells) => (glaciationPct) => () => {
  if (!activeView || !activeView.session) return;
  activeView.session.set_params(cells, glaciationPct / 100.0);
  updateClimateDataBuffers(activeView);
  updateClimateViewColors(activeView);
};

export const loadEarthIntoClimate = (setClimateInfo) => () => {
  if (!activeView || !activeView.session) return;
  activeClimateSnapshotBytes = null;
  activeView.session.load_earth();
  rebuildClimateViewGeometryAndData(activeView);
  setClimateInfo(buildClimateInfo(activeView))();
};

export const loadSavedPlanetIntoClimate = (setClimateInfo) => () => {
  if (!activeView || !activeView.session) return;
  const saved = getSavedSnapshotBytes();
  if (!saved) return;
  const ok = activeView.session.load_snapshot_bytes(saved);
  if (ok) {
    activeClimateSnapshotBytes = saved;
    rebuildClimateViewGeometryAndData(activeView);
    setClimateInfo(buildClimateInfo(activeView))();
  }
};

export const loadFilePlanetIntoClimate = (setClimateInfo) => () => {
  if (!activeView || !activeView.session) return;
  pickBinarySnapshotFile((bytes) => {
    if (!activeView || !activeView.session) return;
    const ok = activeView.session.load_snapshot_bytes(bytes);
    if (ok) {
      storeSavedSnapshotBytes(bytes);
      activeClimateSnapshotBytes = bytes;
      rebuildClimateViewGeometryAndData(activeView);
      setClimateInfo(buildClimateInfo(activeView))();
    }
  });
};

export const mountTectonicsView = (gridSize) => (landPct) => (aspect) => (setTectonicsInfo) => () => {
  const canvas = document.getElementById("sphere-canvas");
  if (!canvas) {
    return () => {};
  }

  const view = createBaseSphereView(canvas, false);
  const seed = BigInt(Date.now());
  view.session = new WasmTectonicsSession(gridSize, landPct, seed);
  view.currentGridSize = gridSize;
  view.aspect = aspect;
  view.selectedTileIdx = -1;
  view.timerId = null;
  view.setTectonicsInfo = setTectonicsInfo;

  const raycaster = new THREE.Raycaster();
  const pointerNdc = new THREE.Vector2();

  const handleCanvasClick = (ev) => {
    if (!view.session) return;
    const rect = canvas.getBoundingClientRect();
    const u = (ev.clientX - rect.left) / rect.width;
    const v = (ev.clientY - rect.top) / rect.height;

    // 1. Exact 3D mesh raycast against the rendered tile triangles:
    pointerNdc.set(2.0 * u - 1.0, 1.0 - 2.0 * v);
    view.mesh.updateMatrixWorld(true);
    view.camera.updateMatrixWorld(true);
    raycaster.setFromCamera(pointerNdc, view.camera);
    const hits = raycaster.intersectObject(view.mesh, false);
    if (hits.length > 0 && hits[0].face && view.vertexTileIds) {
      view.selectedTileIdx = view.vertexTileIds[hits[0].face.a];
    } else {
      // 2. Analytic fallback: unproject orthographic screen coordinates to mesh space,
      //    then invert `rotate_axes(v) = [-v1, v2, v0]` (`v0 = gz, v1 = -gx, v2 = gy`).
      const sx = pointerNdc.x * 1.1;
      const sy = pointerNdc.y * 1.1;
      const r2 = sx * sx + sy * sy;
      if (r2 > 1.0) {
        return;
      }
      const sz = Math.sqrt(Math.max(0.0, 1.0 - r2));
      const cosA = Math.cos(-view.rotationAngleRad);
      const sinA = Math.sin(-view.rotationAngleRad);
      const gx = sx * cosA + sz * sinA;
      const gy = sy;
      const gz = -sx * sinA + sz * cosA;
      view.selectedTileIdx = view.session.nearest_tile(gz, -gx, gy);
    }

    const colors = computeTectonicsVertexColors(view);
    view.geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
    view.paint();
    view.setTectonicsInfo(buildTectonicsInfo(view))();
  };

  canvas.addEventListener("click", handleCanvasClick);

  activeView = view;
  updateTectonicsViewBuffers(view, true);
  setTectonicsInfo(buildTectonicsInfo(view))();

  return () => {
    stopTectonicsLoop(view);
    canvas.removeEventListener("click", handleCanvasClick);
    if (activeView === view) {
      activeView = null;
    }
    view.dispose();
    view.session.free();
  };
};

export const stepTectonics = (setTectonicsInfo) => () => {
  if (!activeView || !activeView.session) return;
  activeView.session.step();
  updateTectonicsViewBuffers(activeView, false);
  setTectonicsInfo(buildTectonicsInfo(activeView))();
};

export const toggleTectonicsRunning = (setTectonicsInfo) => () => {
  if (!activeView || !activeView.session) return;
  const view = activeView;
  view.setTectonicsInfo = setTectonicsInfo;
  if (view.timerId !== null) {
    stopTectonicsLoop(view);
    setTectonicsInfo(buildTectonicsInfo(view))();
  } else {
    view.timerId = setInterval(() => {
      if (!activeView || activeView !== view || !view.session) {
        stopTectonicsLoop(view);
        return;
      }
      view.session.step();
      updateTectonicsViewBuffers(view, false);
      view.setTectonicsInfo(buildTectonicsInfo(view))();
    }, 120);
    setTectonicsInfo(buildTectonicsInfo(view))();
  }
};

export const resetTectonics = (gridSize) => (landPct) => (setTectonicsInfo) => () => {
  if (!activeView || !activeView.session) return;
  stopTectonicsLoop(activeView);
  const sizeChanged = activeView.currentGridSize !== gridSize;
  activeView.session.reset(gridSize, landPct);
  activeView.currentGridSize = gridSize;
  activeView.selectedTileIdx = -1;
  updateTectonicsViewBuffers(activeView, sizeChanged);
  setTectonicsInfo(buildTectonicsInfo(activeView))();
};

export const setTectonicsAspect = (aspect) => () => {
  if (!activeView || !activeView.session) return;
  activeView.aspect = aspect;
  const colors = computeTectonicsVertexColors(activeView);
  activeView.geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
  activeView.paint();
};

export const saveTectonicsPlanet = (setTectonicsInfo) => () => {
  if (!activeView || !activeView.session) return;
  const s = activeView.session;
  const geoticks = s.geoticks();
  const landPercent = s.land_percent_actual();
  const bytes = s.export_snapshot_bytes();
  storeSavedSnapshotBytes(bytes);
  const filename = `planet-${geoticks * 5}Myr.tec11`;
  triggerBinaryDownload(filename, bytes);
  const info = buildTectonicsInfo(activeView);
  info.selectedTileReport = `Saved serialized PlanetSnapshot (${bytes.length} bytes, ${geoticks * 5} Myr, ${landPercent}% land) to ${filename}.\nOpen Climate and click "Load Saved" or "Load File" to explore seasons.`;
  setTectonicsInfo(info)();
};

export const loadTectonicsPlanetFromFile = (setTectonicsSize) => (setTectonicsInfo) => () => {
  if (!activeView || !activeView.session) return;
  stopTectonicsLoop(activeView);
  pickBinarySnapshotFile((bytes) => {
    if (!activeView || !activeView.session) return;
    const ok = activeView.session.load_snapshot_bytes(bytes);
    if (ok) {
      storeSavedSnapshotBytes(bytes);
      const newSize = activeView.session.grid_size();
      const sizeChanged = activeView.currentGridSize !== newSize;
      activeView.currentGridSize = newSize;
      activeView.selectedTileIdx = -1;
      updateTectonicsViewBuffers(activeView, sizeChanged);
      setTectonicsSize(newSize)();
      setTectonicsInfo(buildTectonicsInfo(activeView))();
    }
  });
};

export const rotateGrid = (deg) => () => {
  if (!activeView) return;
  const angleRad = (deg * Math.PI) / 180.0;
  activeView.rotationAngleRad = angleRad;
  activeView.mesh.setRotationFromAxisAngle(_yAxis, angleRad);
  activeView.paint();
};
