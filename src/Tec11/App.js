import * as THREE from "three";
import initWasmModule, {
  WasmClimateSession,
  WasmErosionSession,
  WasmGridSession,
  WasmHistorySession,
  WasmLanguageSession,
  WasmLifeformsSession,
  WasmMeteorSession,
  WasmMoonSession,
  WasmMoveSession,
  WasmPangeaSession,
  WasmPopulationSession,
  WasmPrehistorySession,
  WasmRacinationSession,
  WasmRiversSession,
  WasmSplitSession,
  WasmSunSession,
  WasmTectonicsSession,
  WasmTerrainSession,
  WasmWorldSession,
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
      if (input.parentNode) {
        input.parentNode.removeChild(input);
      }
      return;
    }
    const reader = new FileReader();
    reader.onload = () => {
      if (reader.result instanceof ArrayBuffer) {
        onBytesLoaded(new Uint8Array(reader.result), file.name);
      }
      if (input.parentNode) {
        input.parentNode.removeChild(input);
      }
    };
    reader.onerror = () => {
      if (input.parentNode) {
        input.parentNode.removeChild(input);
      }
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
    impactScars,
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
    } else if (aspect === "meteors") {
      // Base muted natural terrain + bright crimson-orange crater basins & amber-gold ejecta rims:
      if (h <= 0.0) {
        r = 0.0;
        g = 0.02;
        b = 0.32;
      } else {
        const f = felsities[tid];
        r = (0.35 * 128.0) / 255.0;
        g = (0.35 * (64.0 + 64.0 * f + 200.0 * tileLife[tid])) / 255.0;
        b = (0.35 * (128.0 * f)) / 255.0;
      }
      const scar = impactScars ? impactScars[tid] : 0.0;
      if (scar >= 0.60) {
        r = 0.98;
        g = 0.24;
        b = 0.10;
      } else if (scar >= 0.32) {
        r = 0.96;
        g = 0.76;
        b = 0.10;
      } else if (scar > 0.05) {
        const w = Math.min(0.75, scar * 2.5);
        r = r * (1.0 - w) + 0.85 * w;
        g = g * (1.0 - w) + 0.45 * w;
        b = b * (1.0 - w) + 0.20 * w;
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

  if (view.quad2dMesh && view.quad2dMesh.visible && view.pic2dCanvas) {
    ctx.drawImage(view.pic2dCanvas, 0, 0, width, height);
    return;
  }

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
  if (!view.hideSurfaceMesh) {
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

      const shade = view.unlit ? 1.0 : 0.55 + 0.45 * Math.max(0, nVec.dot(lightDir));
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

  // Draw 3D ocean current arrows / travel route lines / constellation & orbit lines if visible
  if (arrowLines && arrowLines.visible && arrowGeometry) {
    const arrowPos = arrowGeometry.attributes.position;
    const arrowCol =
      view.arrowMaterial && view.arrowMaterial.vertexColors
        ? arrowGeometry.attributes.color
        : null;
    if (arrowPos) {
      if (arrowCol) {
        ctx.lineWidth = view.arrowLineWidth || 1.5;
        for (let i = 0; i < arrowPos.count; i += 2) {
          vA.fromBufferAttribute(arrowPos, i).applyMatrix4(mesh.matrixWorld);
          vB.fromBufferAttribute(arrowPos, i + 1).applyMatrix4(mesh.matrixWorld);
          if (!view.disableArrowBackfaceCull && vA.z <= 0.02 && vB.z <= 0.02) {
            continue;
          }
          pA.copy(vA).project(camera);
          pB.copy(vB).project(camera);
          const x0 = (pA.x * 0.5 + 0.5) * width;
          const y0 = (-pA.y * 0.5 + 0.5) * height;
          const x1 = (pB.x * 0.5 + 0.5) * width;
          const y1 = (-pB.y * 0.5 + 0.5) * height;
          const cr = Math.min(255, Math.round(arrowCol.getX(i) * 255));
          const cg = Math.min(255, Math.round(arrowCol.getY(i) * 255));
          const cb = Math.min(255, Math.round(arrowCol.getZ(i) * 255));
          ctx.strokeStyle = `rgba(${cr}, ${cg}, ${cb}, 0.92)`;
          ctx.beginPath();
          ctx.moveTo(x0, y0);
          ctx.lineTo(x1, y1);
          ctx.stroke();
        }
      } else {
        ctx.strokeStyle = view.arrowStrokeStyle || "rgba(248, 250, 252, 0.78)";
        ctx.lineWidth = 1.1;
        ctx.beginPath();
        for (let i = 0; i < arrowPos.count; i += 2) {
          vA.fromBufferAttribute(arrowPos, i).applyMatrix4(mesh.matrixWorld);
          vB.fromBufferAttribute(arrowPos, i + 1).applyMatrix4(mesh.matrixWorld);
          if (!view.disableArrowBackfaceCull && vA.z <= 0.02 && vB.z <= 0.02) {
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

  // Draw 3D star points if visible (`SunScreen`)
  if (view.starPoints && view.starPoints.visible && view.starPointsRaw) {
    const raw = view.starPointsRaw;
    const cullBack = view.cullStarBackface !== false;
    for (let i = 0; i + 6 < raw.length; i += 7) {
      vA.set(raw[i], raw[i + 1], raw[i + 2]).applyMatrix4(mesh.matrixWorld);
      if (cullBack && vA.z < -0.02) {
        continue;
      }
      pA.copy(vA).project(camera);
      const sx = (pA.x * 0.5 + 0.5) * width;
      const sy = (-pA.y * 0.5 + 0.5) * height;
      const sr = Math.min(255, Math.round(raw[i + 3] * 255));
      const sg = Math.min(255, Math.round(raw[i + 4] * 255));
      const sb = Math.min(255, Math.round(raw[i + 5] * 255));
      const rad = Math.max(0.75, raw[i + 6] * 0.52);
      ctx.fillStyle = `rgb(${sr}, ${sg}, ${sb})`;
      ctx.beginPath();
      ctx.arc(sx, sy, rad, 0, Math.PI * 2);
      ctx.fill();
    }
  }
}

function createBaseSphereView(canvas, showBorders) {
  const width = 460;
  const height = 460;

  const camera = new THREE.OrthographicCamera(-1.1, 1.1, 1.1, -1.1, 0.1, 25);
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

  const starGeometry = new THREE.BufferGeometry();
  const starMaterial = new THREE.ShaderMaterial({
    vertexShader: `
      attribute float size;
      attribute vec3 customColor;
      varying vec3 vColor;
      varying float vWorldZ;
      void main() {
        vColor = customColor;
        vec4 worldPos = modelMatrix * vec4(position, 1.0);
        vWorldZ = worldPos.z;
        gl_Position = projectionMatrix * viewMatrix * worldPos;
        gl_PointSize = size;
      }
    `,
    fragmentShader: `
      uniform float cullBackface;
      varying vec3 vColor;
      varying float vWorldZ;
      void main() {
        if (cullBackface > 0.5 && vWorldZ < -0.02) discard;
        vec2 cxy = 2.0 * gl_PointCoord - 1.0;
        float r2 = dot(cxy, cxy);
        if (r2 > 1.0) discard;
        float alpha = 1.0 - r2 * 0.45;
        gl_FragColor = vec4(vColor, alpha);
      }
    `,
    uniforms: {
      cullBackface: { value: 1.0 },
    },
    transparent: true,
    depthTest: true,
    depthWrite: false,
  });
  const starPoints = new THREE.Points(starGeometry, starMaterial);
  starPoints.visible = false;
  mesh.add(starPoints);

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
    starGeometry,
    starMaterial,
    starPoints,
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
      starGeometry.dispose();
      starMaterial.dispose();
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
  view.impactScars = session.tile_impact_scars();
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
  const bundle = buildHistoryInspectorBundle(view);
  if (!bundle.tileView.hasTile) {
    scheduleStratigraphySidePaint(view, selectedTileIdx);
  }
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
    tileView: bundle.tileView,
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
  const labelPrefix = view.loadedFileName ? view.loadedFileName : "Tectonics";
  const planetName = s.is_custom_planet()
    ? `${labelPrefix} (${s.loaded_geoticks() * 5} Myr, ${s.loaded_land_percent()}% land)`
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
    view.loadedFileName = null;

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
  activeView.loadedFileName = null;
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
    activeView.loadedFileName = null;
    rebuildClimateViewGeometryAndData(activeView);
    setClimateInfo(buildClimateInfo(activeView))();
  }
};

export const loadFilePlanetIntoClimate = (setClimateInfo) => () => {
  if (!activeView || !activeView.session) return;
  pickBinarySnapshotFile((bytes, filename) => {
    if (!activeView || !activeView.session) return;
    const ok = activeView.session.load_snapshot_bytes(bytes);
    if (ok) {
      storeSavedSnapshotBytes(bytes);
      activeClimateSnapshotBytes = bytes;
      activeView.loadedFileName = filename || null;
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
  view.inspectorMode = "tectonics";
  view.timerId = null;
  view.setTectonicsInfo = setTectonicsInfo;

  if (activeClimateSnapshotBytes) {
    if (view.session.load_snapshot_bytes(activeClimateSnapshotBytes)) {
      view.currentGridSize = view.session.grid_size();
    }
  }

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
  activeClimateSnapshotBytes = null;
  activeView.session.reset(gridSize, landPct);
  activeView.currentGridSize = gridSize;
  activeView.selectedTileIdx = -1;
  updateTectonicsViewBuffers(activeView, true);
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
  activeClimateSnapshotBytes = bytes;
  const filename = `planet-${geoticks * 5}Myr.tec11`;
  triggerBinaryDownload(filename, bytes);
  const info = buildTectonicsInfo(activeView);
  info.selectedTileReport = `Saved serialized PlanetSnapshot (${bytes.length} bytes, ${geoticks * 5} Myr, ${landPercent}% land) to ${filename}.\nOpen Climate and click "Load Saved" or "Load File" to explore seasons.`;
  setTectonicsInfo(info)();
};

export const loadTectonicsPlanetFromFile = (setTectonicsSize) => (setTectonicsInfo) => () => {
  if (!activeView || !activeView.session) return;
  stopTectonicsLoop(activeView);
  pickBinarySnapshotFile((bytes, filename) => {
    if (!activeView || !activeView.session) return;
    const ok = activeView.session.load_snapshot_bytes(bytes);
    if (ok) {
      storeSavedSnapshotBytes(bytes);
      activeClimateSnapshotBytes = bytes;
      const newSize = activeView.session.grid_size();
      activeView.currentGridSize = newSize;
      activeView.selectedTileIdx = -1;
      updateTectonicsViewBuffers(activeView, true);
      setTectonicsSize(newSize)();
      const info = buildTectonicsInfo(activeView);
      info.selectedTileReport = `Loaded ${filename || "snapshot"} (${info.geoticks * 5} Myr, ${info.actualLandPercent}% land, ${info.tileCount} tiles).\nClick any tile on the globe to inspect its rock layers.`;
      setTectonicsInfo(info)();
    }
  });
};

// ============================================================================
// Split Sandbox Mode (`splitdisplay.py` / `splitpresenter.py`)
// ============================================================================

function computeSplitVertexColors(tileStates, vertexTileIds, timeSlider) {
  const numVerts = vertexTileIds.length;
  const colors = new Float32Array(numVerts * 3);
  for (let v = 0; v < numVerts; v++) {
    const tid = vertexTileIds[v];
    const st = tileStates[tid];
    const out = v * 3;
    if (st === 1) {
      // 'R' (Red half)
      colors[out] = timeSlider >= 1 ? 1.0 : 128.0 / 255.0;
      colors[out + 1] = 128.0 / 255.0;
      colors[out + 2] = 128.0 / 255.0;
    } else if (st === 2) {
      // 'B' (Blue half)
      colors[out] = 128.0 / 255.0;
      colors[out + 1] = 128.0 / 255.0;
      colors[out + 2] = timeSlider >= 2 ? 1.0 : 128.0 / 255.0;
    } else {
      // 'F' (Ocean / outside)
      colors[out] = 1.0;
      colors[out + 1] = 1.0;
      colors[out + 2] = 1.0;
    }
  }
  return colors;
}

function updateSplitViewBuffers(view, rebuildGeometry) {
  const { session, geometry } = view;
  if (rebuildGeometry) {
    const positions = session.mesh_positions();
    const normals = session.mesh_normals();
    view.vertexTileIds = session.vertex_tile_ids();
    geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
    geometry.setAttribute("normal", new THREE.Float32BufferAttribute(normals, 3));
    geometry.computeBoundingSphere();
  }
  view.tileStates = session.tile_states();
  const colors = computeSplitVertexColors(view.tileStates, view.vertexTileIds, view.timeSlider);
  geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
  view.paint();
}

function buildSplitInfo(view) {
  const s = view.session;
  return {
    gridSize: s.grid_size(),
    tileCount: s.tile_count(),
    iterations: s.iterations(),
    redCount: s.red_count(),
    blueCount: s.blue_count(),
  };
}

export const mountSplitView = (gridSize) => (timeSlider) => (setSplitInfo) => () => {
  const canvas = document.getElementById("sphere-canvas");
  if (!canvas) {
    return () => {};
  }

  const view = createBaseSphereView(canvas, false);
  const seed = BigInt(Date.now());
  view.session = new WasmSplitSession(gridSize, seed);
  view.currentGridSize = gridSize;
  view.timeSlider = timeSlider;

  activeView = view;
  updateSplitViewBuffers(view, true);
  setSplitInfo(buildSplitInfo(view))();

  return () => {
    if (activeView === view) {
      activeView = null;
    }
    view.dispose();
    view.session.free();
  };
};

export const iterateSplit = (choice) => (setSplitInfo) => () => {
  if (!activeView || !activeView.session) return;
  activeView.session.iterate(choice);
  updateSplitViewBuffers(activeView, false);
  setSplitInfo(buildSplitInfo(activeView))();
};

export const resetSplit = (gridSize) => (setSplitInfo) => () => {
  if (!activeView || !activeView.session) return;
  const sizeChanged = activeView.currentGridSize !== gridSize;
  activeView.session.reset(gridSize);
  activeView.currentGridSize = gridSize;
  updateSplitViewBuffers(activeView, sizeChanged);
  setSplitInfo(buildSplitInfo(activeView))();
};

export const setSplitTime = (timeSlider) => () => {
  if (!activeView || !activeView.tileStates) return;
  activeView.timeSlider = timeSlider;
  const colors = computeSplitVertexColors(
    activeView.tileStates,
    activeView.vertexTileIds,
    activeView.timeSlider
  );
  activeView.geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
  activeView.paint();
};

// ============================================================================
// Move Sandbox Mode (`movedisplay.py` / `movepresenter.py`)
// ============================================================================

function computeMoveVertexColors(tileStates, vertexTileIds, showTrail) {
  const numVerts = vertexTileIds.length;
  const colors = new Float32Array(numVerts * 3);
  for (let v = 0; v < numVerts; v++) {
    const tid = vertexTileIds[v];
    const st = tileStates[tid];
    const out = v * 3;
    if (st === 1) {
      // 'T' (Active moving continent)
      colors[out] = 128.0 / 255.0;
      colors[out + 1] = 128.0 / 255.0;
      colors[out + 2] = 128.0 / 255.0;
    } else if (st === 2 && showTrail) {
      // 'M' (Previous step trail)
      colors[out] = 192.0 / 255.0;
      colors[out + 1] = 192.0 / 255.0;
      colors[out + 2] = 192.0 / 255.0;
    } else {
      // 'F' (Ocean / outside)
      colors[out] = 1.0;
      colors[out + 1] = 1.0;
      colors[out + 2] = 1.0;
    }
  }
  return colors;
}

function updateMoveViewBuffers(view, rebuildGeometry) {
  const { session, geometry } = view;
  if (rebuildGeometry) {
    const positions = session.mesh_positions();
    const normals = session.mesh_normals();
    view.vertexTileIds = session.vertex_tile_ids();
    geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
    geometry.setAttribute("normal", new THREE.Float32BufferAttribute(normals, 3));
    geometry.computeBoundingSphere();
  }
  view.tileStates = session.tile_states();
  const colors = computeMoveVertexColors(view.tileStates, view.vertexTileIds, view.showTrail);
  geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
  view.paint();
}

function buildMoveInfo(view) {
  const s = view.session;
  return {
    gridSize: s.grid_size(),
    tileCount: s.tile_count(),
    groupTileCount: s.group_tile_count(),
    steps: s.steps(),
    isRunning: view.timerId !== null,
  };
}

export const mountMoveView =
  (gridSize) => (directionDeg) => (speedDeg) => (showTrail) => (setMoveInfo) => () => {
    const canvas = document.getElementById("sphere-canvas");
    if (!canvas) {
      return () => {};
    }

    const view = createBaseSphereView(canvas, false);
    const seed = BigInt(Date.now());
    view.session = new WasmMoveSession(gridSize, directionDeg, speedDeg, seed);
    view.currentGridSize = gridSize;
    view.showTrail = showTrail;
    view.timerId = null;
    view.setMoveInfo = setMoveInfo;

    activeView = view;
    updateMoveViewBuffers(view, true);
    setMoveInfo(buildMoveInfo(view))();

    return () => {
      stopTectonicsLoop(view);
      if (activeView === view) {
        activeView = null;
      }
      view.dispose();
      view.session.free();
    };
  };

export const stepMove = (setMoveInfo) => () => {
  if (!activeView || !activeView.session) return;
  activeView.session.step();
  updateMoveViewBuffers(activeView, false);
  setMoveInfo(buildMoveInfo(activeView))();
};

export const toggleMoveRunning = (setMoveInfo) => () => {
  if (!activeView || !activeView.session) return;
  const view = activeView;
  view.setMoveInfo = setMoveInfo;
  if (view.timerId !== null) {
    stopTectonicsLoop(view);
    setMoveInfo(buildMoveInfo(view))();
  } else {
    view.timerId = setInterval(() => {
      if (!activeView || activeView !== view || !view.session) {
        stopTectonicsLoop(view);
        return;
      }
      view.session.step();
      updateMoveViewBuffers(view, false);
      view.setMoveInfo(buildMoveInfo(view))();
    }, 100);
    setMoveInfo(buildMoveInfo(view))();
  }
};

export const resetMove = (gridSize) => (directionDeg) => (speedDeg) => (setMoveInfo) => () => {
  if (!activeView || !activeView.session) return;
  stopTectonicsLoop(activeView);
  const sizeChanged = activeView.currentGridSize !== gridSize;
  activeView.session.reset(gridSize, directionDeg, speedDeg);
  activeView.currentGridSize = gridSize;
  updateMoveViewBuffers(activeView, sizeChanged);
  setMoveInfo(buildMoveInfo(activeView))();
};

export const setMoveDirection = (directionDeg) => () => {
  if (!activeView || !activeView.session) return;
  activeView.session.set_direction_deg(directionDeg);
};

export const setMoveSpeed = (speedDeg) => () => {
  if (!activeView || !activeView.session) return;
  activeView.session.set_speed_deg(speedDeg);
};

export const setMoveTrail = (showTrail) => () => {
  if (!activeView || !activeView.tileStates) return;
  activeView.showTrail = showTrail;
  const colors = computeMoveVertexColors(
    activeView.tileStates,
    activeView.vertexTileIds,
    activeView.showTrail
  );
  activeView.geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
  activeView.paint();
};

// ============================================================================
// Erosion Sandbox Mode (`erosiondisplay.py` / `erosionpresenter.py`)
// ============================================================================

function computeErosionVertexColors(view) {
  const { elevations, lost, gained, vertexTileIds, showLost, showGained, selectedTileIdx } = view;
  const numVerts = vertexTileIds.length;
  const colors = new Float32Array(numVerts * 3);

  for (let v = 0; v < numVerts; v++) {
    const tid = vertexTileIds[v];
    const h = elevations[tid];
    const out = v * 3;
    let r = 0.0;
    let g = 0.0;
    let b = 0.0;
    let baseV = 0.0;

    if (h > 0.0) {
      baseV = 128.0;
      const grey = Math.min(255.0, 128.0 + 12.5 * h);
      r = grey;
      g = grey;
      b = grey;
    }

    if (showLost) {
      const l = Math.min(5.0, Math.max(0.0, lost[tid]));
      r = Math.min(255.0, baseV + l * 25.5);
      g = baseV;
      b = baseV;
    }

    if (showGained) {
      const gn = Math.min(5.0, Math.max(0.0, gained[tid]));
      g = Math.min(255.0, baseV + gn * 25.5);
      b = baseV;
      if (!showLost) {
        r = baseV;
      }
    }

    r /= 255.0;
    g /= 255.0;
    b /= 255.0;

    if (tid === selectedTileIdx) {
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

function updateErosionViewBuffers(view, rebuildGeometry) {
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
  view.lost = session.tile_lost();
  view.gained = session.tile_gained();

  const colors = computeErosionVertexColors(view);
  geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
  view.paint();
}

function buildErosionInfo(view) {
  const s = view.session;
  const labelPrefix = view.loadedFileName ? view.loadedFileName : "Tectonics";
  const planetName = s.is_custom_planet()
    ? `${labelPrefix} (${s.loaded_geoticks() * 5} Myr, ${s.loaded_land_percent()}% land)`
    : "Earth (ETOPO2)";
  const selectedTileReport =
    view.selectedTileIdx >= 0
      ? s.inspect_tile(view.selectedTileIdx)
      : "Click any tile on the globe to inspect elevation, erosion, and deposition.";
  scheduleStratigraphySidePaint(view, view.selectedTileIdx);
  return {
    planetName,
    tileCount: s.tile_count(),
    steps: s.steps(),
    hasSavedPlanet: getSavedSnapshotBytes() !== null,
    selectedTileReport,
  };
}

export const mountErosionView = (showLost) => (showGained) => (setErosionInfo) => () => {
  const canvas = document.getElementById("sphere-canvas");
  if (!canvas) {
    return () => {};
  }

  const view = createBaseSphereView(canvas, false);
  view.session = new WasmErosionSession();
  view.showLost = showLost;
  view.showGained = showGained;
  view.selectedTileIdx = -1;
  view.setErosionInfo = setErosionInfo;
  view.loadedFileName = null;

  if (activeClimateSnapshotBytes) {
    view.session.load_snapshot_bytes(activeClimateSnapshotBytes);
  }

  const raycaster = new THREE.Raycaster();
  const pointerNdc = new THREE.Vector2();

  const handleCanvasClick = (ev) => {
    if (!view.session) return;
    const rect = canvas.getBoundingClientRect();
    const u = (ev.clientX - rect.left) / rect.width;
    const v = (ev.clientY - rect.top) / rect.height;

    pointerNdc.set(2.0 * u - 1.0, 1.0 - 2.0 * v);
    view.mesh.updateMatrixWorld(true);
    view.camera.updateMatrixWorld(true);
    raycaster.setFromCamera(pointerNdc, view.camera);
    const hits = raycaster.intersectObject(view.mesh, false);
    if (hits.length > 0 && hits[0].face && view.vertexTileIds) {
      view.selectedTileIdx = view.vertexTileIds[hits[0].face.a];
      const colors = computeErosionVertexColors(view);
      view.geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
      view.paint();
      view.setErosionInfo(buildErosionInfo(view))();
    }
  };

  canvas.addEventListener("click", handleCanvasClick);

  activeView = view;
  updateErosionViewBuffers(view, true);
  setErosionInfo(buildErosionInfo(view))();

  return () => {
    canvas.removeEventListener("click", handleCanvasClick);
    if (activeView === view) {
      activeView = null;
    }
    view.dispose();
    view.session.free();
  };
};

export const stepErosion = (setErosionInfo) => () => {
  if (!activeView || !activeView.session) return;
  activeView.session.step_erode();
  updateErosionViewBuffers(activeView, false);
  setErosionInfo(buildErosionInfo(activeView))();
};

export const setErosionOverlays = (showLost) => (showGained) => () => {
  if (!activeView || !activeView.elevations) return;
  activeView.showLost = showLost;
  activeView.showGained = showGained;
  const colors = computeErosionVertexColors(activeView);
  activeView.geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
  activeView.paint();
};

export const loadEarthIntoErosion = (setErosionInfo) => () => {
  if (!activeView || !activeView.session) return;
  activeClimateSnapshotBytes = null;
  activeView.loadedFileName = null;
  activeView.session.load_earth();
  activeView.selectedTileIdx = -1;
  updateErosionViewBuffers(activeView, true);
  setErosionInfo(buildErosionInfo(activeView))();
};

export const loadSavedPlanetIntoErosion = (setErosionInfo) => () => {
  if (!activeView || !activeView.session) return;
  const saved = getSavedSnapshotBytes();
  if (!saved) return;
  if (activeView.session.load_snapshot_bytes(saved)) {
    activeClimateSnapshotBytes = saved;
    activeView.loadedFileName = null;
    activeView.selectedTileIdx = -1;
    updateErosionViewBuffers(activeView, true);
    setErosionInfo(buildErosionInfo(activeView))();
  }
};

export const loadFilePlanetIntoErosion = (setErosionInfo) => () => {
  if (!activeView || !activeView.session) return;
  pickBinarySnapshotFile((bytes, filename) => {
    if (!activeView || !activeView.session) return;
    if (activeView.session.load_snapshot_bytes(bytes)) {
      storeSavedSnapshotBytes(bytes);
      activeClimateSnapshotBytes = bytes;
      activeView.loadedFileName = filename || null;
      activeView.selectedTileIdx = -1;
      updateErosionViewBuffers(activeView, true);
      const info = buildErosionInfo(activeView);
      info.selectedTileReport = `Loaded ${filename || "snapshot"} (${activeView.session.loaded_geoticks() * 5} Myr, ${activeView.session.loaded_land_percent()}% land).\nClick any tile on the globe to inspect elevation, erosion, and deposition.`;
      setErosionInfo(info)();
    }
  });
};

// ============================================================================
// Rivers Sandbox Mode (`riversdisplay.py` / `riverspresenter.py`)
// ============================================================================

function computeRiversVertexColors(view) {
  const { elevations, felsities, koeppenIds, tileLife, riverProgress, vertexTileIds } = view;
  const numVerts = vertexTileIds.length;
  const colors = new Float32Array(numVerts * 3);

  for (let v = 0; v < numVerts; v++) {
    const tid = vertexTileIds[v];
    const out = v * 3;
    const prog = riverProgress[tid];

    if (prog > 0.0) {
      // `riversdisplay.py:6-9`:
      // `s = 1 - rts[t]`, `return (127*s, 255*s, 255*s)`
      const s = Math.max(0.15, 1.0 - prog);
      colors[out] = (127.0 * s) / 255.0;
      colors[out + 1] = s;
      colors[out + 2] = s;
      continue;
    }

    const h = elevations[tid];
    if (h <= 0.0) {
      colors[out] = 0.0;
      colors[out + 1] = 0.0;
      colors[out + 2] = 128.0 / 255.0;
    } else {
      const kId = koeppenIds[tid];
      if (kId === 12 || kId === 13) {
        colors[out] = 1.0;
        colors[out + 1] = 1.0;
        colors[out + 2] = 1.0;
      } else {
        const f = felsities[tid];
        const rockR = 128.0;
        const rockG = 64.0 + 64.0 * f;
        const rockB = 128.0 * f;
        const lifeGreen = 255.0 * tileLife[tid];
        colors[out] = (0.5 * rockR) / 255.0;
        colors[out + 1] = (0.5 * (rockG + lifeGreen)) / 255.0;
        colors[out + 2] = (0.5 * rockB) / 255.0;
      }
    }
  }

  return colors;
}

function updateRiversViewBuffers(view, rebuildGeometry) {
  const { session, geometry } = view;
  if (rebuildGeometry) {
    const positions = session.mesh_positions();
    const normals = session.mesh_normals();
    view.vertexTileIds = session.vertex_tile_ids();
    view.elevations = session.tile_elevations();
    view.felsities = session.tile_felsities();
    view.koeppenIds = session.tile_koeppen_ids();
    view.tileLife = session.tile_life();
    geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
    geometry.setAttribute("normal", new THREE.Float32BufferAttribute(normals, 3));
    geometry.computeBoundingSphere();
  }
  view.riverProgress = session.tile_river_progress();
  const riverLines = session.river_line_positions();
  view.arrowMaterial.color.setHex(0x67e8f9);
  view.arrowMaterial.opacity = 0.92;
  view.arrowLines.visible = riverLines.length > 0;
  view.arrowGeometry.setAttribute("position", new THREE.Float32BufferAttribute(riverLines, 3));

  const colors = computeRiversVertexColors(view);
  geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
  view.paint();
}

function buildRiversInfo(view) {
  const s = view.session;
  const labelPrefix = view.loadedFileName ? view.loadedFileName : "Tectonics";
  const planetName = s.is_custom_planet()
    ? `${labelPrefix} (${s.loaded_geoticks() * 5} Myr, ${s.loaded_land_percent()}% land)`
    : "Earth (ETOPO2)";
  return {
    planetName,
    tileCount: s.tile_count(),
    riverCount: s.river_count(),
    hasSavedPlanet: getSavedSnapshotBytes() !== null,
  };
}

export const mountRiversView = (hminSlider) => (pminSlider) => (setRiversInfo) => () => {
  const canvas = document.getElementById("sphere-canvas");
  if (!canvas) {
    return () => {};
  }

  const view = createBaseSphereView(canvas, false);
  const hmin = hminSlider / 10.0;
  const pmin = (3.0 * pminSlider) / 100.0;
  view.session = new WasmRiversSession(hmin, pmin);
  view.loadedFileName = null;

  if (activeClimateSnapshotBytes) {
    view.session.load_snapshot_bytes(activeClimateSnapshotBytes);
  }

  activeView = view;
  updateRiversViewBuffers(view, true);
  setRiversInfo(buildRiversInfo(view))();

  return () => {
    if (activeView === view) {
      activeView = null;
    }
    view.dispose();
    view.session.free();
  };
};

export const setRiversParams = (hminSlider) => (pminSlider) => (setRiversInfo) => () => {
  if (!activeView || !activeView.session) return;
  const hmin = hminSlider / 10.0;
  const pmin = (3.0 * pminSlider) / 100.0;
  activeView.session.set_params(hmin, pmin);
  updateRiversViewBuffers(activeView, false);
  setRiversInfo(buildRiversInfo(activeView))();
};

export const loadEarthIntoRivers = (setRiversInfo) => () => {
  if (!activeView || !activeView.session) return;
  activeClimateSnapshotBytes = null;
  activeView.loadedFileName = null;
  activeView.session.load_earth();
  updateRiversViewBuffers(activeView, true);
  setRiversInfo(buildRiversInfo(activeView))();
};

export const loadSavedPlanetIntoRivers = (setRiversInfo) => () => {
  if (!activeView || !activeView.session) return;
  const saved = getSavedSnapshotBytes();
  if (!saved) return;
  if (activeView.session.load_snapshot_bytes(saved)) {
    activeClimateSnapshotBytes = saved;
    activeView.loadedFileName = null;
    updateRiversViewBuffers(activeView, true);
    setRiversInfo(buildRiversInfo(activeView))();
  }
};

export const loadFilePlanetIntoRivers = (setRiversInfo) => () => {
  if (!activeView || !activeView.session) return;
  pickBinarySnapshotFile((bytes, filename) => {
    if (!activeView || !activeView.session) return;
    if (activeView.session.load_snapshot_bytes(bytes)) {
      storeSavedSnapshotBytes(bytes);
      activeClimateSnapshotBytes = bytes;
      activeView.loadedFileName = filename || null;
      updateRiversViewBuffers(activeView, true);
      setRiversInfo(buildRiversInfo(activeView))();
    }
  });
};

// ============================================================================
// Lifeforms Sandbox Mode (`lifeformsdisplay.py` / `lifeformspresenter.py`)
// ============================================================================

function lifeformsColorscale(vClamped) {
  const v = Math.max(0.0, Math.min(1.0, vClamped));
  const m = 1275.0;
  const r = v < 0.2 ? 255.0 - m * v : v < 0.6 ? 0.0 : v < 0.8 ? m * (v - 0.6) : 255.0;
  const g = v < 0.2 ? 0.0 : v < 0.4 ? m * (v - 0.2) : v < 0.8 ? 255.0 : 255.0 - m * (v - 0.8);
  const b = v < 0.4 ? 255.0 : v < 0.6 ? 255.0 - m * (v - 0.4) : 0.0;
  return [
    Math.max(0.0, Math.min(255.0, r)) / 255.0,
    Math.max(0.0, Math.min(255.0, g)) / 255.0,
    Math.max(0.0, Math.min(255.0, b)) / 255.0,
  ];
}

function getLifeformsSeasonIndex(view) {
  const seasonCount = view.session.season_count();
  return Math.min(
    seasonCount - 1,
    Math.max(0, Math.round(((seasonCount - 1) * view.seasonSlider) / 100.0))
  );
}

function computeLifeformsVertexColors(view) {
  const {
    elevations,
    felsities,
    koeppenIds,
    vertexTileIds,
    attribute,
    selectedTileIdx,
    selectedSpeciesId,
    speciesPresenceMask,
  } = view;
  const seasonIdx = getLifeformsSeasonIndex(view);
  const animalCounts = view.seasonalAnimalCounts[seasonIdx];
  const plantCounts = view.seasonalPlantCounts[seasonIdx];
  const numVerts = vertexTileIds.length;
  const colors = new Float32Array(numVerts * 3);

  for (let v = 0; v < numVerts; v++) {
    const tid = vertexTileIds[v];
    const out = v * 3;

    if (tid === selectedTileIdx) {
      // `lifeformsdisplay.py:77`: `(255, 0, 0)` for selected tile
      colors[out] = 1.0;
      colors[out + 1] = 0.0;
      colors[out + 2] = 0.0;
      continue;
    }

    const h = elevations[tid];
    let baseR = 0.0;
    let baseG = 0.0;
    let baseB = 0.0;
    if (h > 0.0) {
      const kId = koeppenIds[tid];
      if (kId === 12 || kId === 13) {
        baseR = 1.0;
        baseG = 1.0;
        baseB = 1.0;
      } else {
        const f = felsities[tid];
        baseR = 128.0 / 255.0;
        baseG = (64.0 + 64.0 * f) / 255.0;
        baseB = (128.0 * f) / 255.0;
      }
    }

    if (selectedSpeciesId >= 0 && speciesPresenceMask) {
      if (speciesPresenceMask[tid] === 1) {
        colors[out] = 0.0;
        colors[out + 1] = 1.0;
        colors[out + 2] = 0.0;
      } else {
        colors[out] = baseR;
        colors[out + 1] = baseG;
        colors[out + 2] = baseB;
      }
    } else {
      let pop = 0;
      let denom = 100.0;
      if (attribute === "animals") {
        pop = animalCounts[tid];
        denom = 100.0;
      } else if (attribute === "plants") {
        pop = plantCounts[tid];
        denom = 100.0;
      } else {
        pop = animalCounts[tid] + plantCounts[tid];
        denom = 200.0;
      }
      if (pop > 0) {
        const [cr, cg, cb] = lifeformsColorscale(pop / denom);
        colors[out] = cr;
        colors[out + 1] = cg;
        colors[out + 2] = cb;
      } else {
        colors[out] = baseR;
        colors[out + 1] = baseG;
        colors[out + 2] = baseB;
      }
    }
  }

  return colors;
}

function updateLifeformsDataBuffers(view, rebuildGeometry) {
  const { session, geometry } = view;
  if (rebuildGeometry) {
    const positions = session.mesh_positions();
    const normals = session.mesh_normals();
    view.vertexTileIds = session.vertex_tile_ids();
    view.elevations = session.tile_elevations();
    view.felsities = session.tile_felsities();
    geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
    geometry.setAttribute("normal", new THREE.Float32BufferAttribute(normals, 3));
    geometry.computeBoundingSphere();
  }
  view.koeppenIds = session.tile_koeppen_ids();
  const seasonCount = session.season_count();
  view.seasonalAnimalCounts = [];
  view.seasonalPlantCounts = [];
  for (let s = 0; s < seasonCount; s++) {
    view.seasonalAnimalCounts.push(session.tile_animal_counts(s));
    view.seasonalPlantCounts.push(session.tile_plant_counts(s));
  }
  if (view.selectedSpeciesId >= 0) {
    view.speciesPresenceMask = session.species_presence_mask(
      view.selectedSpeciesId,
      getLifeformsSeasonIndex(view)
    );
  } else {
    view.speciesPresenceMask = null;
  }
  const colors = computeLifeformsVertexColors(view);
  geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
  view.paint();
}

function buildLifeformsInfo(view) {
  const s = view.session;
  const planetName = s.is_custom_planet()
    ? `Tectonics (${s.loaded_geoticks() * 5} Myr, ${s.loaded_land_percent()}% land)`
    : "Earth (ETOPO2)";
  const seasonIdx = getLifeformsSeasonIndex(view);
  const tileSpecies = [];
  let selectedTileRegion = "";
  if (view.selectedTileIdx >= 0) {
    selectedTileRegion = s.tile_region_name(view.selectedTileIdx) || "";
    const tsv = s.species_on_tile_tsv(view.selectedTileIdx, seasonIdx, view.attribute);
    const lines = tsv.split("\n");
    for (const line of lines) {
      if (!line) continue;
      const parts = line.split("\t");
      if (parts.length >= 3) {
        tileSpecies.push({
          id: parseInt(parts[0], 10),
          kind: parts[1],
          name: parts[2],
        });
      }
    }
  }
  return {
    planetName,
    tileCount: s.tile_count(),
    settledAnimals: s.settled_animal_count(),
    settledPlants: s.settled_plant_count(),
    hasSavedPlanet: getSavedSnapshotBytes() !== null,
    selectedTileIdx: view.selectedTileIdx,
    selectedTileRegion,
    selectedSpeciesId: view.selectedSpeciesId,
    tileSpecies,
  };
}

export const mountLifeformsView =
  (glaciationSlider) => (attribute) => (seasonSlider) => (setLifeformsInfo) => () => {
    const canvas = document.getElementById("sphere-canvas");
    if (!canvas) {
      return () => {};
    }

    const view = createBaseSphereView(canvas, false);
    const glaciation = 1.0 - glaciationSlider / 100.0;
    view.session = new WasmLifeformsSession(glaciation);
    view.attribute = attribute;
    view.seasonSlider = seasonSlider;
    view.selectedTileIdx = -1;
    view.selectedSpeciesId = -1;
    view.speciesPresenceMask = null;
    view.setLifeformsInfo = setLifeformsInfo;

    if (activeClimateSnapshotBytes) {
      view.session.load_snapshot_bytes(activeClimateSnapshotBytes);
    }

    const raycaster = new THREE.Raycaster();
    const pointerNdc = new THREE.Vector2();

    const handleCanvasClick = (ev) => {
      if (!view.session) return;
      const rect = canvas.getBoundingClientRect();
      const u = (ev.clientX - rect.left) / rect.width;
      const v = (ev.clientY - rect.top) / rect.height;

      pointerNdc.set(2.0 * u - 1.0, 1.0 - 2.0 * v);
      view.mesh.updateMatrixWorld(true);
      view.camera.updateMatrixWorld(true);
      raycaster.setFromCamera(pointerNdc, view.camera);
      const hits = raycaster.intersectObject(view.mesh, false);
      if (hits.length > 0 && hits[0].face && view.vertexTileIds) {
        view.selectedTileIdx = view.vertexTileIds[hits[0].face.a];
      } else {
        view.selectedTileIdx = -1;
      }
      const info = buildLifeformsInfo(view);
      if (
        view.selectedSpeciesId >= 0 &&
        !info.tileSpecies.some((sp) => sp.id === view.selectedSpeciesId)
      ) {
        view.selectedSpeciesId = -1;
        view.speciesPresenceMask = null;
        info.selectedSpeciesId = -1;
      }
      const colors = computeLifeformsVertexColors(view);
      view.geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
      view.paint();
      view.setLifeformsInfo(info)();
    };

    canvas.addEventListener("click", handleCanvasClick);

    activeView = view;
    updateLifeformsDataBuffers(view, true);
    setLifeformsInfo(buildLifeformsInfo(view))();

    return () => {
      canvas.removeEventListener("click", handleCanvasClick);
      if (activeView === view) {
        activeView = null;
      }
      view.dispose();
      view.session.free();
    };
  };

export const setLifeformsAttribute = (attribute) => (setLifeformsInfo) => () => {
  if (!activeView || !activeView.session) return;
  activeView.attribute = attribute;
  activeView.selectedSpeciesId = -1;
  activeView.speciesPresenceMask = null;
  const colors = computeLifeformsVertexColors(activeView);
  activeView.geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
  activeView.paint();
  setLifeformsInfo(buildLifeformsInfo(activeView))();
};

export const setLifeformsSeason = (seasonSlider) => (setLifeformsInfo) => () => {
  if (!activeView || !activeView.session) return;
  activeView.seasonSlider = seasonSlider;
  if (activeView.selectedSpeciesId >= 0) {
    activeView.speciesPresenceMask = activeView.session.species_presence_mask(
      activeView.selectedSpeciesId,
      getLifeformsSeasonIndex(activeView)
    );
  }
  const colors = computeLifeformsVertexColors(activeView);
  activeView.geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
  activeView.paint();
  setLifeformsInfo(buildLifeformsInfo(activeView))();
};

export const setLifeformsGlaciation = (glaciationSlider) => (setLifeformsInfo) => () => {
  if (!activeView || !activeView.session) return;
  const glaciation = 1.0 - glaciationSlider / 100.0;
  activeView.session.set_glaciation(glaciation);
  activeView.selectedSpeciesId = -1;
  activeView.speciesPresenceMask = null;
  updateLifeformsDataBuffers(activeView, false);
  setLifeformsInfo(buildLifeformsInfo(activeView))();
};

export const selectLifeformsSpecies = (speciesId) => (setLifeformsInfo) => () => {
  if (!activeView || !activeView.session) return;
  if (activeView.selectedSpeciesId === speciesId || speciesId < 0) {
    activeView.selectedSpeciesId = -1;
    activeView.speciesPresenceMask = null;
  } else {
    activeView.selectedSpeciesId = speciesId;
    activeView.speciesPresenceMask = activeView.session.species_presence_mask(
      speciesId,
      getLifeformsSeasonIndex(activeView)
    );
  }
  const colors = computeLifeformsVertexColors(activeView);
  activeView.geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
  activeView.paint();
  setLifeformsInfo(buildLifeformsInfo(activeView))();
};

export const loadEarthIntoLifeforms = (setLifeformsInfo) => () => {
  if (!activeView || !activeView.session) return;
  activeClimateSnapshotBytes = null;
  activeView.session.load_earth();
  activeView.selectedTileIdx = -1;
  activeView.selectedSpeciesId = -1;
  activeView.speciesPresenceMask = null;
  updateLifeformsDataBuffers(activeView, true);
  setLifeformsInfo(buildLifeformsInfo(activeView))();
};

export const loadSavedPlanetIntoLifeforms = (setLifeformsInfo) => () => {
  if (!activeView || !activeView.session) return;
  const saved = getSavedSnapshotBytes();
  if (!saved) return;
  if (activeView.session.load_snapshot_bytes(saved)) {
    activeClimateSnapshotBytes = saved;
    activeView.selectedTileIdx = -1;
    activeView.selectedSpeciesId = -1;
    activeView.speciesPresenceMask = null;
    updateLifeformsDataBuffers(activeView, true);
    setLifeformsInfo(buildLifeformsInfo(activeView))();
  }
};

export const loadFilePlanetIntoLifeforms = (setLifeformsInfo) => () => {
  if (!activeView || !activeView.session) return;
  pickBinarySnapshotFile((bytes) => {
    if (!activeView || !activeView.session) return;
    if (activeView.session.load_snapshot_bytes(bytes)) {
      storeSavedSnapshotBytes(bytes);
      activeClimateSnapshotBytes = bytes;
      activeView.selectedTileIdx = -1;
      activeView.selectedSpeciesId = -1;
      activeView.speciesPresenceMask = null;
      updateLifeformsDataBuffers(activeView, true);
      setLifeformsInfo(buildLifeformsInfo(activeView))();
    }
  });
};

// ============================================================================
// Mode 9: Population Simulation (`populationmethod.py`, `populationdisplay.py`, `populationpresenter.py`)
// ============================================================================

function naturalTileColorRgb(h, f, hasAtmosphere, kId, life) {
  if (h <= 0.0) {
    return [0.0, 0.0, 128.0 / 255.0];
  }
  const fVal = f !== undefined && f !== null ? f : 0.7;
  const rockR = 128.0;
  const rockG = 64.0 + 64.0 * fVal;
  const rockB = 128.0 * fVal;
  if (!hasAtmosphere || !kId) {
    return [rockR / 255.0, rockG / 255.0, rockB / 255.0];
  }
  if (kId === 12 || kId === 13) {
    return [1.0, 1.0, 1.0];
  }
  const lifeGreen = 255.0 * (life || 0.0);
  return [
    (0.5 * rockR) / 255.0,
    (0.5 * (rockG + lifeGreen)) / 255.0,
    (0.5 * rockB) / 255.0,
  ];
}

function computeNaturalTileColor(h, f, kId, life) {
  return naturalTileColorRgb(h, f, true, kId, life);
}

function computePopulationVertexColors(view) {
  const { vertexTileIds, elevations, felsities, koeppenIds, lifeValues, populatedMask } = view;
  const vertexCount = vertexTileIds.length;
  const colors = new Float32Array(vertexCount * 3);
  for (let v = 0; v < vertexCount; v++) {
    const t = vertexTileIds[v];
    if (populatedMask && populatedMask[t]) {
      // `populationdisplay.py:12-13`: populated tiles are highlighted in bright yellow `(255, 255, 0)`
      colors[v * 3] = 1.0;
      colors[v * 3 + 1] = 1.0;
      colors[v * 3 + 2] = 0.0;
    } else {
      const [r, g, b] = naturalTileColorRgb(
        elevations[t],
        felsities[t],
        true,
        koeppenIds ? koeppenIds[t] : 0,
        lifeValues ? lifeValues[t] : 0.0
      );
      colors[v * 3] = r;
      colors[v * 3 + 1] = g;
      colors[v * 3 + 2] = b;
    }
  }
  return colors;
}

function updatePopulationDataBuffers(view, rebuildGeometry = false) {
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
  view.koeppenIds = session.tile_koeppen_ids();
  view.lifeValues = session.tile_life();
  view.populatedMask = session.populated_mask();
  const colors = computePopulationVertexColors(view);
  geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
  view.paint();
}

function buildPopulationInfo(view) {
  const s = view.session;
  const planetName = s.is_custom_planet()
    ? `Saved Planet (${s.loaded_geoticks() * 5} Myr, ${s.loaded_land_percent()}% land)`
    : "Earth (ETOPO2)";
  return {
    planetName,
    tileCount: s.tile_count(),
    populatedCount: s.populated_count(),
    ticks: s.ticks(),
    isRunning: Boolean(view.isRunning),
    hasSavedPlanet: getSavedSnapshotBytes() !== null,
  };
}

function stopPopulationLoop(view) {
  view.isRunning = false;
  if (view.timerId !== null && view.timerId !== undefined) {
    clearTimeout(view.timerId);
    view.timerId = null;
  }
}

function schedulePopulationTick(view) {
  if (!view.isRunning || activeView !== view) return;
  view.timerId = setTimeout(() => {
    if (!view.isRunning || activeView !== view) return;
    const expanded = view.session.step();
    view.populatedMask = view.session.populated_mask();
    const colors = computePopulationVertexColors(view);
    view.geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
    view.paint();
    if (!expanded) {
      stopPopulationLoop(view);
    }
    if (view.setPopulationInfo) {
      view.setPopulationInfo(buildPopulationInfo(view))();
    }
    if (view.isRunning) {
      schedulePopulationTick(view);
    }
  }, 100);
}

export const mountPopulationView =
  (coastProx) => (travelRange) => (setPopulationInfo) => () => {
    const canvas = document.getElementById("sphere-canvas");
    if (!canvas) {
      return () => {};
    }

    const view = createBaseSphereView(canvas, false);
    view.session = new WasmPopulationSession(coastProx, travelRange);
    view.isRunning = false;
    view.timerId = null;
    view.setPopulationInfo = setPopulationInfo;

    if (activeClimateSnapshotBytes) {
      view.session.load_snapshot_bytes(activeClimateSnapshotBytes);
    }

    activeView = view;
    updatePopulationDataBuffers(view, true);
    setPopulationInfo(buildPopulationInfo(view))();

    return () => {
      stopPopulationLoop(view);
      if (activeView === view) {
        activeView = null;
      }
      view.dispose();
      view.session.free();
    };
  };

export const stepPopulation = (setPopulationInfo) => () => {
  if (!activeView || !activeView.session || activeView.isRunning) return;
  activeView.session.step();
  activeView.populatedMask = activeView.session.populated_mask();
  const colors = computePopulationVertexColors(activeView);
  activeView.geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
  activeView.paint();
  setPopulationInfo(buildPopulationInfo(activeView))();
};

export const togglePopulationRunning = (setPopulationInfo) => () => {
  if (!activeView || !activeView.session) return;
  if (activeView.isRunning) {
    stopPopulationLoop(activeView);
    setPopulationInfo(buildPopulationInfo(activeView))();
  } else {
    activeView.isRunning = true;
    setPopulationInfo(buildPopulationInfo(activeView))();
    schedulePopulationTick(activeView);
  }
};

export const resetPopulation = (setPopulationInfo) => () => {
  if (!activeView || !activeView.session) return;
  stopPopulationLoop(activeView);
  activeView.session.reset();
  activeView.populatedMask = activeView.session.populated_mask();
  const colors = computePopulationVertexColors(activeView);
  activeView.geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
  activeView.paint();
  setPopulationInfo(buildPopulationInfo(activeView))();
};

export const setPopulationCoastProx = (coastProx) => () => {
  if (!activeView || !activeView.session) return;
  activeView.session.set_coast_prox(coastProx);
};

export const setPopulationTravelRange = (travelRange) => () => {
  if (!activeView || !activeView.session) return;
  activeView.session.set_travel_range(travelRange);
};

export const loadEarthIntoPopulation = (setPopulationInfo) => () => {
  if (!activeView || !activeView.session) return;
  stopPopulationLoop(activeView);
  activeClimateSnapshotBytes = null;
  activeView.session.load_earth();
  updatePopulationDataBuffers(activeView, true);
  setPopulationInfo(buildPopulationInfo(activeView))();
};

export const loadSavedPlanetIntoPopulation = (setPopulationInfo) => () => {
  if (!activeView || !activeView.session) return;
  const saved = getSavedSnapshotBytes();
  if (!saved) return;
  stopPopulationLoop(activeView);
  if (activeView.session.load_snapshot_bytes(saved)) {
    activeClimateSnapshotBytes = saved;
    updatePopulationDataBuffers(activeView, true);
    setPopulationInfo(buildPopulationInfo(activeView))();
  }
};

export const loadFilePlanetIntoPopulation = (setPopulationInfo) => () => {
  if (!activeView || !activeView.session) return;
  pickBinarySnapshotFile((bytes) => {
    if (!activeView || !activeView.session) return;
    stopPopulationLoop(activeView);
    if (activeView.session.load_snapshot_bytes(bytes)) {
      storeSavedSnapshotBytes(bytes);
      activeClimateSnapshotBytes = bytes;
      updatePopulationDataBuffers(activeView, true);
      setPopulationInfo(buildPopulationInfo(activeView))();
    }
  });
};

// ============================================================================
// Mode 10: Racination Simulation (`racinatemethod.py`, `racinationdisplay.py`, `racinationpresenter.py`)
// ============================================================================

function computeRacinationVertexColors(view) {
  const { vertexTileIds, tileColors } = view;
  const vertexCount = vertexTileIds.length;
  const colors = new Float32Array(vertexCount * 3);
  for (let v = 0; v < vertexCount; v++) {
    const t = vertexTileIds[v];
    colors[v * 3] = tileColors[t * 3];
    colors[v * 3 + 1] = tileColors[t * 3 + 1];
    colors[v * 3 + 2] = tileColors[t * 3 + 2];
  }
  return colors;
}

function updateRacinationDataBuffers(view, rebuildGeometry = false) {
  const { session, geometry } = view;
  if (rebuildGeometry) {
    const positions = session.mesh_positions();
    const normals = session.mesh_normals();
    view.vertexTileIds = session.vertex_tile_ids();
    geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
    geometry.setAttribute("normal", new THREE.Float32BufferAttribute(normals, 3));
    geometry.computeBoundingSphere();
  }
  view.tileColors = session.tile_colors_rgb();
  const colors = computeRacinationVertexColors(view);
  geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
  view.paint();
}

function buildRacinationInfo(view) {
  const s = view.session;
  return {
    tileCount: s.tile_count(),
    peoples: s.peoples(),
    populatedCount: s.populated_count(),
    isolations: s.isolations(),
  };
}

export const mountRacinationView = (travelRange) => (setRacinationInfo) => () => {
  const canvas = document.getElementById("sphere-canvas");
  if (!canvas) {
    return () => {};
  }

  const view = createBaseSphereView(canvas, false);
  view.session = new WasmRacinationSession(travelRange);
  view.setRacinationInfo = setRacinationInfo;

  activeView = view;
  updateRacinationDataBuffers(view, true);
  setRacinationInfo(buildRacinationInfo(view))();

  return () => {
    if (activeView === view) {
      activeView = null;
    }
    view.dispose();
    view.session.free();
  };
};

export const isolateRacination = (setRacinationInfo) => () => {
  if (!activeView || !activeView.session) return;
  activeView.session.isolate();
  updateRacinationDataBuffers(activeView, false);
  setRacinationInfo(buildRacinationInfo(activeView))();
};

export const resetRacination = (setRacinationInfo) => () => {
  if (!activeView || !activeView.session) return;
  activeView.session.reset();
  updateRacinationDataBuffers(activeView, false);
  setRacinationInfo(buildRacinationInfo(activeView))();
};

export const setRacinationTravelRange = (travelRange) => () => {
  if (!activeView || !activeView.session) return;
  activeView.session.set_travel_range(travelRange);
};

// ============================================================================
// Mode 11: Prehistory Simulation (`prehistorysimulation.py`, `prehistorydisplay.py`, `prehistorypresenter.py`)
// ============================================================================

function formatPopulationThousands(thousands) {
  if (!thousands || thousands <= 0.0) {
    return "0";
  }
  if (thousands < 0.95) {
    return String(Math.round(thousands * 1000.0));
  }
  if (thousands < 999.5) {
    return `${Math.round(thousands)}K`;
  }
  return `${Math.round(thousands / 1000.0)}M`;
}

function computePrehistoryVertexColors(view) {
  const {
    vertexTileIds,
    elevations,
    felsities,
    koeppenIds,
    lifeValues,
    overlayColors,
    selectedTileIdx,
  } = view;
  const vertexCount = vertexTileIds.length;
  const colors = new Float32Array(vertexCount * 3);
  for (let v = 0; v < vertexCount; v++) {
    const t = vertexTileIds[v];
    if (selectedTileIdx >= 0 && t === selectedTileIdx) {
      colors[v * 3] = 1.0;
      colors[v * 3 + 1] = 0.92;
      colors[v * 3 + 2] = 0.15;
    } else {
      const ovR = overlayColors[t * 3];
      if (ovR >= 0.0) {
        colors[v * 3] = ovR;
        colors[v * 3 + 1] = overlayColors[t * 3 + 1];
        colors[v * 3 + 2] = overlayColors[t * 3 + 2];
      } else if (elevations[t] <= 0.0) {
        colors[v * 3] = 0.0;
        colors[v * 3 + 1] = 0.0;
        colors[v * 3 + 2] = 1.0;
      } else {
        const rgb = computeNaturalTileColor(
          elevations[t],
          felsities[t],
          koeppenIds[t],
          lifeValues[t]
        );
        colors[v * 3] = rgb[0];
        colors[v * 3 + 1] = rgb[1];
        colors[v * 3 + 2] = rgb[2];
      }
    }
  }
  return colors;
}

function updatePrehistoryDataBuffers(view, rebuildGeometry = false) {
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
  view.koeppenIds = session.tile_koeppen_ids();
  view.lifeValues = session.tile_life();
  view.overlayColors = session.population_overlay_colors();
  const colors = computePrehistoryVertexColors(view);
  geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
  view.paint();
}

function buildPrehistoryInfo(view) {
  const s = view.session;
  const planetName = s.is_custom_planet()
    ? `Saved Planet (${s.tectonics_ticks() * 5} Myr, ${s.land_percent()}% land)`
    : "Earth (ETOPO2)";
  const ticks = s.prehistory_ticks();
  const selectedReport =
    view.selectedTileIdx >= 0
      ? s.inspect_tile(view.selectedTileIdx)
      : "Click any coarse tile on the globe to inspect its stratigraphy, climate, population, and local sky & vantage.";
  const bundle = buildHistoryInspectorBundle(view);
  if (!bundle.tileView.hasTile) {
    scheduleStratigraphySidePaint(view, view.selectedTileIdx);
  }
  return {
    planetName,
    tileCount: s.tile_count(),
    ticks,
    kyr: ticks * 4,
    glaciationPct: Math.round(s.glaciation() * 100.0),
    races: s.active_race_count(),
    agRaces: s.active_agricultural_race_count(),
    populatedCount: s.populated_count(),
    populationText: formatPopulationThousands(s.total_population_thousands()),
    isRunning: Boolean(view.isRunning),
    hasSavedPlanet: getSavedSnapshotBytes() !== null,
    selectedReport,
    tileView: bundle.tileView,
  };
}

function stopPrehistoryLoop(view) {
  view.isRunning = false;
  if (view.timerId !== null && view.timerId !== undefined) {
    clearTimeout(view.timerId);
    view.timerId = null;
  }
}

function schedulePrehistoryTick(view) {
  if (!view.isRunning || activeView !== view) return;
  view.timerId = setTimeout(() => {
    if (!view.isRunning || activeView !== view) return;
    view.session.step();
    updatePrehistoryDataBuffers(view, false);
    if (view.setPrehistoryInfo) {
      view.setPrehistoryInfo(buildPrehistoryInfo(view))();
    }
    if (view.isRunning) {
      schedulePrehistoryTick(view);
    }
  }, 120);
}

export const mountPrehistoryView = (setPrehistoryInfo) => () => {
  const canvas = document.getElementById("sphere-canvas");
  if (!canvas) {
    return () => {};
  }

  const view = createBaseSphereView(canvas, false);
  view.session = new WasmPrehistorySession();
  view.selectedTileIdx = -1;
  view.inspectorMode = "prehistory";
  view.isRunning = false;
  view.timerId = null;
  view.setPrehistoryInfo = setPrehistoryInfo;

  if (activeClimateSnapshotBytes) {
    view.session.load_snapshot_bytes(activeClimateSnapshotBytes);
  }

  const raycaster = new THREE.Raycaster();
  const pointerNdc = new THREE.Vector2();
  const handleCanvasClick = (ev) => {
    if (!view.session) return;
    const rect = canvas.getBoundingClientRect();
    const u = (ev.clientX - rect.left) / rect.width;
    const v = (ev.clientY - rect.top) / rect.height;
    pointerNdc.set(2.0 * u - 1.0, 1.0 - 2.0 * v);
    view.mesh.updateMatrixWorld(true);
    view.camera.updateMatrixWorld(true);
    raycaster.setFromCamera(pointerNdc, view.camera);
    const hits = raycaster.intersectObject(view.mesh, false);
    if (hits.length > 0 && hits[0].face && view.vertexTileIds) {
      view.selectedTileIdx = view.vertexTileIds[hits[0].face.a];
      updatePrehistoryDataBuffers(view, false);
      view.setPrehistoryInfo(buildPrehistoryInfo(view))();
    }
  };
  canvas.addEventListener("click", handleCanvasClick);

  activeView = view;
  updatePrehistoryDataBuffers(view, true);
  setPrehistoryInfo(buildPrehistoryInfo(view))();

  return () => {
    canvas.removeEventListener("click", handleCanvasClick);
    stopPrehistoryLoop(view);
    if (activeView === view) {
      activeView = null;
    }
    view.dispose();
    view.session.free();
  };
};

export const stepPrehistory = (setPrehistoryInfo) => () => {
  if (!activeView || !activeView.session || activeView.isRunning) return;
  activeView.session.step();
  updatePrehistoryDataBuffers(activeView, false);
  setPrehistoryInfo(buildPrehistoryInfo(activeView))();
};

export const togglePrehistoryRunning = (setPrehistoryInfo) => () => {
  if (!activeView || !activeView.session) return;
  if (activeView.isRunning) {
    stopPrehistoryLoop(activeView);
    setPrehistoryInfo(buildPrehistoryInfo(activeView))();
  } else {
    activeView.isRunning = true;
    setPrehistoryInfo(buildPrehistoryInfo(activeView))();
    schedulePrehistoryTick(activeView);
  }
};

export const resetPrehistory = (setPrehistoryInfo) => () => {
  if (!activeView || !activeView.session) return;
  stopPrehistoryLoop(activeView);
  activeView.session.reset();
  updatePrehistoryDataBuffers(activeView, false);
  setPrehistoryInfo(buildPrehistoryInfo(activeView))();
};

export const savePrehistoryPlanet = (setPrehistoryInfo) => () => {
  if (!activeView || !activeView.session) return;
  const bytes = activeView.session.encode_snapshot();
  if (!bytes || bytes.length === 0) return;
  storeSavedSnapshotBytes(bytes);
  activeClimateSnapshotBytes = bytes;
  const kyr = activeView.session.prehistory_ticks() * 4;
  triggerBinaryDownload(`tec11-prehistory-${kyr}kyr.tec11`, bytes);
  setPrehistoryInfo(buildPrehistoryInfo(activeView))();
};

export const loadEarthIntoPrehistory = (setPrehistoryInfo) => () => {
  if (!activeView || !activeView.session) return;
  stopPrehistoryLoop(activeView);
  activeClimateSnapshotBytes = null;
  activeView.selectedTileIdx = -1;
  activeView.session.load_earth();
  updatePrehistoryDataBuffers(activeView, true);
  setPrehistoryInfo(buildPrehistoryInfo(activeView))();
};

export const loadSavedPlanetIntoPrehistory = (setPrehistoryInfo) => () => {
  if (!activeView || !activeView.session) return;
  const saved = getSavedSnapshotBytes();
  if (!saved) return;
  stopPrehistoryLoop(activeView);
  if (activeView.session.load_snapshot_bytes(saved)) {
    activeView.selectedTileIdx = -1;
    activeClimateSnapshotBytes = saved;
    updatePrehistoryDataBuffers(activeView, true);
    setPrehistoryInfo(buildPrehistoryInfo(activeView))();
  }
};

export const loadFilePlanetIntoPrehistory = (setPrehistoryInfo) => () => {
  if (!activeView || !activeView.session) return;
  pickBinarySnapshotFile((bytes) => {
    if (!activeView || !activeView.session) return;
    stopPrehistoryLoop(activeView);
    if (activeView.session.load_snapshot_bytes(bytes)) {
      activeView.selectedTileIdx = -1;
      storeSavedSnapshotBytes(bytes);
      activeClimateSnapshotBytes = bytes;
      updatePrehistoryDataBuffers(activeView, true);
      setPrehistoryInfo(buildPrehistoryInfo(activeView))();
    }
  });
};

// ============================================================================
// Mode 12: Language Simulation (`languagesimulation.py`, `languagepresenter.py`, `languageui.py`)
// ============================================================================

let activeLanguageSession = null;

function buildLanguageInfo(session) {
  const count = session.word_count();
  const words = [];
  for (let i = 0; i < count; i++) {
    words.push({
      idx: i,
      concept: session.concept_label(i),
      sourceOrth: session.source_orth(i),
      sourceIpa: session.source_ipa(i),
      destOrth: session.dest_orth(i),
      destIpa: session.dest_ipa(i),
      etymology: session.dest_etymology(i),
    });
  }
  return {
    sourcePhonemes: session.source_phonemes_text(),
    destPhonemes: session.dest_phonemes_text(),
    words,
  };
}

export const mountLanguageView = (setLanguageInfo) => () => {
  const session = new WasmLanguageSession();
  activeLanguageSession = session;
  setLanguageInfo(buildLanguageInfo(session))();

  return () => {
    if (activeLanguageSession === session) {
      activeLanguageSession = null;
    }
    session.free();
  };
};

export const generateLanguage = (setLanguageInfo) => () => {
  if (!activeLanguageSession) return;
  activeLanguageSession.generate();
  setLanguageInfo(buildLanguageInfo(activeLanguageSession))();
};

export const borrowLanguage = (setLanguageInfo) => () => {
  if (!activeLanguageSession) return;
  activeLanguageSession.borrow();
  setLanguageInfo(buildLanguageInfo(activeLanguageSession))();
};

export const applyLanguageSoundChange = (kindId) => (setLanguageInfo) => () => {
  if (!activeLanguageSession) return;
  activeLanguageSession.apply_sound_change(kindId);
  setLanguageInfo(buildLanguageInfo(activeLanguageSession))();
};

// ============================================================================
// Mode 13: History Simulation (`historysimulation.py`, `historydisplay.py`, `historypresenter.py`)
// ============================================================================

function computeHistoryVertexColors(view) {
  const { vertexTileIds, tileColors } = view;
  const vertexCount = vertexTileIds.length;
  const colors = new Float32Array(vertexCount * 3);
  for (let v = 0; v < vertexCount; v++) {
    const t = vertexTileIds[v];
    colors[v * 3] = tileColors[t * 3];
    colors[v * 3 + 1] = tileColors[t * 3 + 1];
    colors[v * 3 + 2] = tileColors[t * 3 + 2];
  }
  return colors;
}

function setColoredArrowSegments(view, raw9Buffers) {
  let totalSegs = 0;
  for (const buf of raw9Buffers) {
    if (buf && buf.length >= 9) {
      totalSegs += Math.floor(buf.length / 9);
    }
  }
  if (totalSegs === 0) {
    view.arrowLines.visible = false;
    return;
  }
  const positions = new Float32Array(totalSegs * 6);
  const colors = new Float32Array(totalSegs * 6);
  let outIdx = 0;
  for (const buf of raw9Buffers) {
    if (!buf || buf.length < 9) continue;
    const n = Math.floor(buf.length / 9) * 9;
    for (let i = 0; i < n; i += 9) {
      positions[outIdx] = buf[i];
      positions[outIdx + 1] = buf[i + 1];
      positions[outIdx + 2] = buf[i + 2];
      positions[outIdx + 3] = buf[i + 3];
      positions[outIdx + 4] = buf[i + 4];
      positions[outIdx + 5] = buf[i + 5];
      const r = buf[i + 6];
      const g = buf[i + 7];
      const b = buf[i + 8];
      colors[outIdx] = r;
      colors[outIdx + 1] = g;
      colors[outIdx + 2] = b;
      colors[outIdx + 3] = r;
      colors[outIdx + 4] = g;
      colors[outIdx + 5] = b;
      outIdx += 6;
    }
  }
  view.arrowGeometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
  view.arrowGeometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
  view.arrowMaterial.vertexColors = true;
  view.arrowMaterial.color.setHex(0xffffff);
  view.arrowMaterial.opacity = 0.94;
  view.arrowMaterial.needsUpdate = true;
  view.arrowLineWidth = 1.65;
  view.arrowLines.visible = true;
}

function updateHistoryDataBuffers(view, rebuildGeometry = false) {
  const { session, geometry } = view;
  if (rebuildGeometry) {
    const positions = session.mesh_positions();
    const normals = session.mesh_normals();
    view.vertexTileIds = session.vertex_tile_ids();
    geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
    geometry.setAttribute("normal", new THREE.Float32BufferAttribute(normals, 3));
    geometry.computeBoundingSphere();
  }
  view.tileColors = session.tile_colors_rgb(
    view.aspect,
    Boolean(view.showRivers),
    view.selectedTileIdx
  );
  const colors = computeHistoryVertexColors(view);
  geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));

  if (view.aspect === "travel") {
    const routeSegs = session.travel_route_segments_colored(false);
    setColoredArrowSegments(view, [routeSegs]);
  } else if (view.aspect === "winds_currents") {
    const vecSegs = session.wind_current_vector_segments();
    const routeSegs = session.travel_route_segments_colored(false);
    setColoredArrowSegments(view, [vecSegs, routeSegs]);
  } else {
    view.arrowLines.visible = false;
  }

  view.paint();
}

function emptyStateView() {
  return {
    hasState: false,
    stateId: -1,
    stateName: "",
    colorCss: "rgb(148, 163, 184)",
    continents: "",
    technology: "",
    populationText: "",
    capacityText: "",
    tileCount: 0,
    asabiyaText: "",
    nationalities: [],
    languages: [],
    exports: [],
    imports: [],
    tradePartners: [],
    conflicts: [],
    waterRoutes: [],
  };
}

function emptyTileView() {
  return {
    hasTile: false,
    displayIdx: -1,
    stateId: -1,
    title: "",
    locationSummary: "",
    populationSummary: "",
    technology: "",
    nationalities: [],
    languages: [],
    flora: [],
    fauna: [],
    skyYear: 0,
    skyDay: 0,
    skyDaysPerYear: 365,
    skyLatLonSummary: "",
    skyAnimated: true,
    vantageBearingSummary: "",
    vantageNeighborSummary: "",
  };
}

function emptyLanguageDetail() {
  return {
    hasLanguage: false,
    langId: -1,
    name: "",
    totalSpeakersText: "",
    phonemeSummary: "",
    speakersByLocation: [],
    speakersByNation: [],
    lexicon: [],
  };
}

function emptyNationDetail() {
  return {
    hasNation: false,
    natId: -1,
    name: "",
    totalPopText: "",
    originSummary: "",
    heritageSummary: "",
    currentLocations: [],
    spokenLanguages: [],
  };
}

function pointInPolygon2D(u, v, pts) {
  if (!pts || pts.length < 3) return false;
  let inside = false;
  for (let i = 0, j = pts.length - 1; i < pts.length; j = i++) {
    const xi = pts[i][0];
    const yi = pts[i][1];
    const xj = pts[j][0];
    const yj = pts[j][1];
    const intersect =
      yi > v !== yj > v && u < ((xj - xi) * (v - yi)) / (yj - yi + 1e-12) + xi;
    if (intersect) inside = !inside;
  }
  return inside;
}

function refreshActiveInspectorView(view) {
  if (!view || !view.session) return;
  if (view.inspectorMode === "history" && view.setHistoryInfo) {
    updateHistoryDataBuffers(view, false);
    view.setHistoryInfo(buildHistoryInfo(view))();
  } else if (view.inspectorMode === "travel" && view.setTravelInfo) {
    updateTravelDataBuffers(view, false);
    view.setTravelInfo(buildTravelInfo(view))();
  } else if (view.inspectorMode === "world" && view.setWorldInfo) {
    updateWorldDataBuffers(view, false);
    view.setWorldInfo(buildWorldInfo(view))();
  } else if (view.inspectorMode === "tectonics" && view.setTectonicsInfo) {
    view.setTectonicsInfo(buildTectonicsInfo(view))();
  } else if (view.inspectorMode === "prehistory" && view.setPrehistoryInfo) {
    view.setPrehistoryInfo(buildPrehistoryInfo(view))();
  }
}

function eclipticToEquatorial(lamRad, betaRad, epsRad) {
  const cosB = Math.cos(betaRad);
  const xEcl = cosB * Math.cos(lamRad);
  const yEcl = cosB * Math.sin(lamRad);
  const zEcl = Math.sin(betaRad);
  const cosE = Math.cos(epsRad);
  const sinE = Math.sin(epsRad);
  return [xEcl, yEcl * cosE - zEcl * sinE, yEcl * sinE + zEcl * cosE];
}

function raDecDegToEquatorial(raDeg, decDeg) {
  const ra = (raDeg * Math.PI) / 180.0;
  const dec = (decDeg * Math.PI) / 180.0;
  const cosD = Math.cos(dec);
  return [cosD * Math.cos(ra), cosD * Math.sin(ra), Math.sin(dec)];
}

function equatorialToHorizontal(eqVec, siderealRad, latRad) {
  const cosT = Math.cos(siderealRad);
  const sinT = Math.sin(siderealRad);
  const xm = eqVec[0] * cosT + eqVec[1] * sinT;
  const ym = -eqVec[0] * sinT + eqVec[1] * cosT;
  const zm = eqVec[2];
  const sinPhi = Math.sin(latRad);
  const cosPhi = Math.cos(latRad);
  const east = ym;
  const north = -sinPhi * xm + cosPhi * zm;
  const up = cosPhi * xm + sinPhi * zm;
  return [east, north, up];
}

function projectHorizontalToDome(horiz, cx, cy, radius) {
  const east = horiz[0];
  const north = horiz[1];
  const up = Math.max(-1.0, Math.min(1.0, horiz[2]));
  const alt = Math.asin(up);
  const rFrac = (Math.PI * 0.5 - alt) / (Math.PI * 0.5);
  const hLen = Math.hypot(east, north);
  const ux = hLen > 1e-6 ? east / hLen : 0.0;
  const uy = hLen > 1e-6 ? north / hLen : 0.0;
  return {
    x: cx + radius * rFrac * ux,
    y: cy - radius * rFrac * uy,
    up,
    alt,
    rFrac,
  };
}

function sunEquatorialAtDays(sky, totalDays, epsRad) {
  const daysPerYear = Math.max(1.0, sky.homePeriodDays || sky.daysPerYear || 365.0);
  const lamSun = (sky.homeInitAngleRad || 0.0) + (2.0 * Math.PI * totalDays) / daysPerYear;
  return eclipticToEquatorial(lamSun, 0.0, epsRad);
}

function moonEquatorialAtDays(moon, totalDays, epsRad) {
  const period = Math.max(1.0, moon.periodDays || 27.32);
  const incRad = ((moon.inclinationDeg || 5.1) * Math.PI) / 180.0;
  const lam = (moon.initAngleRad || 0.0) + (2.0 * Math.PI * totalDays) / period;
  const node = (moon.nodeAscRad || 0.0) - (2.0 * Math.PI * totalDays) / (period * 248.0);
  const beta = Math.asin(Math.sin(incRad) * Math.sin(lam - node));
  return eclipticToEquatorial(lam, beta, epsRad);
}

function planetEquatorialAtDays(planet, sky, totalDays, epsRad) {
  const homeP = Math.max(1.0, sky.homePeriodDays || sky.daysPerYear || 365.0);
  const homeR = Math.max(0.02, sky.homeOrbitAu || 1.0);
  const thHome = (sky.homeInitAngleRad || 0.0) + (2.0 * Math.PI * totalDays) / homeP;
  const hx = homeR * Math.cos(thHome);
  const hy = homeR * Math.sin(thHome);

  const pPeriod = Math.max(1.0, planet.periodDays || 365.0);
  const pR = Math.max(0.02, planet.orbitAu || 1.5);
  const thP = (planet.initAngleRad || 0.0) + (2.0 * Math.PI * totalDays) / pPeriod;
  const incRad = ((planet.inclinationDeg || 1.8) * Math.PI) / 180.0;
  const px = pR * Math.cos(thP);
  const py = pR * Math.sin(thP) * Math.cos(incRad);
  const pz = pR * Math.sin(thP) * Math.sin(incRad);

  const dx = px - hx;
  const dy = py - hy;
  const dz = pz;
  const dist = Math.hypot(dx, dy, dz) || 1.0;
  const lam = Math.atan2(dy, dx);
  const beta = Math.asin(Math.max(-1.0, Math.min(1.0, dz / dist)));
  return eclipticToEquatorial(lam, beta, epsRad);
}

function drawDiurnalArc(ctx, sampleEqFn, siderealMidday, latRad, cx, cy, radius, strokeStyle, lineWidth, dashed) {
  const steps = 48;
  ctx.save();
  ctx.strokeStyle = strokeStyle;
  ctx.lineWidth = lineWidth;
  if (dashed) {
    ctx.setLineDash([4, 3]);
  }
  ctx.beginPath();
  let penDown = false;
  for (let i = 0; i <= steps; i++) {
    const tau = i / steps;
    const sid = siderealMidday + 2.0 * Math.PI * (tau - 0.5);
    const eq = sampleEqFn(tau);
    const horiz = equatorialToHorizontal(eq, sid, latRad);
    if (horiz[2] >= -0.015) {
      const pt = projectHorizontalToDome(horiz, cx, cy, radius);
      if (!penDown) {
        ctx.moveTo(pt.x, pt.y);
        penDown = true;
      } else {
        ctx.lineTo(pt.x, pt.y);
      }
    } else {
      penDown = false;
    }
  }
  ctx.stroke();
  ctx.restore();
}

function paintTileSkyCanvas(view) {
  const canvas = document.getElementById("tile-sky-canvas");
  if (!canvas || !view || !view.tileSky) return;
  const sky = view.tileSky;

  const rect = canvas.getBoundingClientRect();
  const cssW = Math.max(170, Math.round(rect.width || 210));
  const cssH = Math.max(190, Math.round(rect.height || 224));
  const dpr = window.devicePixelRatio || 1;
  const targetW = Math.round(cssW * dpr);
  const targetH = Math.round(cssH * dpr);
  if (canvas.width !== targetW || canvas.height !== targetH) {
    canvas.width = targetW;
    canvas.height = targetH;
  }

  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  ctx.save();
  ctx.scale(dpr, dpr);

  ctx.fillStyle = "#040814";
  ctx.fillRect(0, 0, cssW, cssH);

  const cx = cssW * 0.5;
  const cy = cssH * 0.51;
  const radius = Math.min(cssW, cssH) * 0.425;

  const latRad = ((sky.latDeg || 0.0) * Math.PI) / 180.0;
  const epsRad = ((sky.tiltDeg || 23.5) * Math.PI) / 180.0;
  const daysPerYear = Math.max(1, sky.daysPerYear || 365);
  const year = Math.max(0, view.tileSkyYear || 0);
  const day = Math.max(0, Math.min(daysPerYear - 1, view.tileSkyDay || 0));
  const tau = typeof view.tileSkyHourFrac === "number" ? view.tileSkyHourFrac : 0.42;

  const baseDays = year * daysPerYear + day;
  const totalDays = baseDays + tau;

  // Determine solar noon RA on this day so tau=0.5 is local solar noon
  const sunNoonEq = sunEquatorialAtDays(sky, baseDays + 0.5, epsRad);
  const raNoon = Math.atan2(sunNoonEq[1], sunNoonEq[0]);
  const siderealNow = raNoon + 2.0 * Math.PI * (tau - 0.5);

  const sunEqNow = sunEquatorialAtDays(sky, totalDays, epsRad);
  const sunHorizNow = equatorialToHorizontal(sunEqNow, siderealNow, latRad);
  const sunAltDeg = (Math.asin(Math.max(-1.0, Math.min(1.0, sunHorizNow[2]))) * 180.0) / Math.PI;
  const sunDecDeg = (Math.asin(Math.max(-1.0, Math.min(1.0, sunEqNow[2]))) * 180.0) / Math.PI;

  // Update live time & solar declination readout
  const timeEl = document.getElementById("tile-sky-time-label");
  if (timeEl) {
    const totalHours = tau * 24.0;
    const hh = String(Math.floor(totalHours) % 24).padStart(2, "0");
    const mm = String(Math.floor((totalHours % 1.0) * 60.0)).padStart(2, "0");
    const signDec = sunDecDeg >= 0 ? "+" : "";
    timeEl.textContent = `${hh}:${mm} | Sun Dec ${signDec}${sunDecDeg.toFixed(1)}°`;
  }

  // Dynamic sky dome color based on sun altitude and PlanetaryOptics (or airless vacuum black)
  const hasAtm = sky.hasAtmosphere !== false;
  const dayFactor = Math.max(0.0, Math.min(1.0, (sunAltDeg + 6.0) / 24.0));
  const twiFactor = Math.max(0.0, Math.min(1.0, 1.0 - Math.abs(sunAltDeg + 3.0) / 11.0));
  const zRgb = sky.skyZenithRgb || [0.16, 0.38, 0.82];
  const hRgb = sky.skyHorizonRgb || [0.48, 0.72, 0.96];
  const sRgb = sky.sunsetRgb || [0.95, 0.45, 0.22];
  const tRgb = sky.twilightRgb || [0.18, 0.22, 0.45];

  const mixChannel = (nightV, dayV, twiV) =>
    Math.round(
      Math.max(
        0,
        Math.min(
          255,
          (nightV * (1.0 - dayFactor) + dayV * dayFactor * 0.85 + twiV * twiFactor * 0.45) * 255.0
        )
      )
    );

  const zenColor = hasAtm
    ? `rgb(${mixChannel(0.02, zRgb[0], tRgb[0])}, ${mixChannel(0.04, zRgb[1], tRgb[1])}, ${mixChannel(0.11, zRgb[2], tRgb[2])})`
    : "rgb(4, 7, 15)";
  const horColor = hasAtm
    ? `rgb(${mixChannel(0.05, hRgb[0], sRgb[0])}, ${mixChannel(0.08, hRgb[1], sRgb[1])}, ${mixChannel(0.17, hRgb[2], sRgb[2])})`
    : "rgb(9, 13, 24)";

  // Clip inside the local sky dome
  ctx.save();
  ctx.beginPath();
  ctx.arc(cx, cy, radius, 0, Math.PI * 2);
  ctx.clip();

  const grad = ctx.createRadialGradient(cx, cy, radius * 0.05, cx, cy, radius);
  grad.addColorStop(0.0, zenColor);
  grad.addColorStop(1.0, horColor);
  ctx.fillStyle = grad;
  ctx.fillRect(cx - radius, cy - radius, radius * 2.0, radius * 2.0);

  // Altitude rings (30° and 60°) and N-S / E-W crosshairs
  ctx.strokeStyle = "rgba(148, 163, 184, 0.22)";
  ctx.lineWidth = 0.8;
  for (const frac of [1.0 / 3.0, 2.0 / 3.0]) {
    ctx.beginPath();
    ctx.arc(cx, cy, radius * frac, 0, Math.PI * 2);
    ctx.stroke();
  }
  ctx.beginPath();
  ctx.moveTo(cx - radius, cy);
  ctx.lineTo(cx + radius, cy);
  ctx.moveTo(cx, cy - radius);
  ctx.lineTo(cx, cy + radius);
  ctx.stroke();

  // 1. Background stars & Constellation asterisms + diurnal star paths
  const starAlpha = hasAtm ? Math.max(0.28, 1.0 - dayFactor * 0.65) : 0.94;
  if (sky.constellations) {
    for (const c of sky.constellations) {
      // Subtle diurnal path of constellation center
      const cCenterEq = raDecDegToEquatorial(c.raDeg, c.decDeg);
      drawDiurnalArc(
        ctx,
        () => cCenterEq,
        raNoon,
        latRad,
        cx,
        cy,
        radius,
        `rgba(56, 189, 248, ${(0.16 * starAlpha).toFixed(3)})`,
        0.75,
        true
      );

      // Project constellation stars at current sidereal angle
      const projStars = (c.stars || []).map((st) => {
        const eq = raDecDegToEquatorial(st[0], st[1]);
        const horiz = equatorialToHorizontal(eq, siderealNow, latRad);
        return { ...projectHorizontalToDome(horiz, cx, cy, radius), mag: st[2] || 1.8 };
      });

      // Constellation stick-figure edges
      ctx.strokeStyle = `rgba(56, 189, 248, ${(0.62 * starAlpha).toFixed(3)})`;
      ctx.lineWidth = 1.1;
      for (const edge of c.edges || []) {
        const a = projStars[edge[0]];
        const b = projStars[edge[1]];
        if (a && b && (a.up >= -0.02 || b.up >= -0.02)) {
          ctx.beginPath();
          ctx.moveTo(a.x, a.y);
          ctx.lineTo(b.x, b.y);
          ctx.stroke();
        }
      }

      // Constellation stars
      for (const ps of projStars) {
        if (ps.up < -0.01) continue;
        const rStar = Math.max(1.2, 2.7 - 0.45 * ps.mag);
        ctx.fillStyle = `rgba(241, 245, 249, ${(0.95 * starAlpha).toFixed(3)})`;
        ctx.beginPath();
        ctx.arc(ps.x, ps.y, rStar, 0, Math.PI * 2);
        ctx.fill();
      }

      // Constellation label at its brightest star if above horizon
      if (c.name && projStars.length > 0 && projStars[0].up > 0.08) {
        ctx.fillStyle = `rgba(125, 211, 252, ${(0.82 * starAlpha).toFixed(3)})`;
        ctx.font = "9px sans-serif";
        ctx.fillText(c.name, projStars[0].x + 4, projStars[0].y - 3);
      }
    }
  }

  // 2. Planets: diurnal paths + animated positions
  if (sky.planets) {
    for (const p of sky.planets) {
      const pCss = rgbFloatToCss(p.rgb, 240, 190, 110);
      drawDiurnalArc(
        ctx,
        (tFrac) => planetEquatorialAtDays(p, sky, baseDays + tFrac, epsRad),
        raNoon,
        latRad,
        cx,
        cy,
        radius,
        pCss,
        1.1,
        true
      );

      const pEq = planetEquatorialAtDays(p, sky, totalDays, epsRad);
      const pHoriz = equatorialToHorizontal(pEq, siderealNow, latRad);
      if (pHoriz[2] >= 0.0) {
        const pt = projectHorizontalToDome(pHoriz, cx, cy, radius);
        if (p.hasRings) {
          ctx.strokeStyle = "rgba(253, 230, 138, 0.85)";
          ctx.lineWidth = 1.1;
          ctx.beginPath();
          ctx.ellipse(pt.x, pt.y, 6.2, 2.4, -0.35, 0, Math.PI * 2);
          ctx.stroke();
        }
        ctx.fillStyle = pCss;
        ctx.strokeStyle = "#0f172a";
        ctx.lineWidth = 1.0;
        ctx.beginPath();
        ctx.arc(pt.x, pt.y, 3.3, 0, Math.PI * 2);
        ctx.fill();
        ctx.stroke();

        ctx.fillStyle = "#fde68a";
        ctx.font = "9.5px sans-serif";
        ctx.fillText(p.name, pt.x + 5, pt.y + 3);
      }
    }
  }

  // 3. Moons: diurnal paths + animated phase discs
  if (sky.moons) {
    for (const m of sky.moons) {
      drawDiurnalArc(
        ctx,
        (tFrac) => moonEquatorialAtDays(m, baseDays + tFrac, epsRad),
        raNoon,
        latRad,
        cx,
        cy,
        radius,
        "rgba(203, 213, 225, 0.72)",
        1.25,
        true
      );

      const mEq = moonEquatorialAtDays(m, totalDays, epsRad);
      const mHoriz = equatorialToHorizontal(mEq, siderealNow, latRad);
      if (mHoriz[2] >= 0.0) {
        const pt = projectHorizontalToDome(mHoriz, cx, cy, radius);
        const rMoon = Math.max(3.8, 5.4 - m.index * 0.5);
        // Illumination fraction from 1 - dot(mEq, sunEqNow)
        const cosElong =
          mEq[0] * sunEqNow[0] + mEq[1] * sunEqNow[1] + mEq[2] * sunEqNow[2];
        const illum = 0.5 * (1.0 - cosElong);

        // Dark limb
        ctx.fillStyle = "#1e293b";
        ctx.strokeStyle = "#94a3b8";
        ctx.lineWidth = 1.0;
        ctx.beginPath();
        ctx.arc(pt.x, pt.y, rMoon, 0, Math.PI * 2);
        ctx.fill();
        ctx.stroke();

        // Lit crescent/gibbous portion
        ctx.fillStyle = "#f1f5f9";
        ctx.beginPath();
        ctx.arc(pt.x, pt.y, rMoon * Math.max(0.2, illum), 0, Math.PI * 2);
        ctx.fill();

        ctx.fillStyle = "#e2e8f0";
        ctx.font = "9.5px sans-serif";
        ctx.fillText(m.name, pt.x + rMoon + 3, pt.y - 2);
      }
    }
  }

  // 4. Sun: diurnal path arc + animated glowing Sun disc
  const sunCss = rgbFloatToCss(sky.sunRgb, 255, 228, 132);
  drawDiurnalArc(
    ctx,
    (tFrac) => sunEquatorialAtDays(sky, baseDays + tFrac, epsRad),
    raNoon,
    latRad,
    cx,
    cy,
    radius,
    "rgba(250, 204, 21, 0.92)",
    1.85,
    false
  );

  if (sunHorizNow[2] >= -0.02) {
    const ptSun = projectHorizontalToDome(sunHorizNow, cx, cy, radius);
    const sunGlow = ctx.createRadialGradient(ptSun.x, ptSun.y, 1.5, ptSun.x, ptSun.y, 14.0);
    sunGlow.addColorStop(0.0, "#ffffff");
    sunGlow.addColorStop(0.35, sunCss);
    sunGlow.addColorStop(1.0, "rgba(250, 204, 21, 0.0)");
    ctx.fillStyle = sunGlow;
    ctx.beginPath();
    ctx.arc(ptSun.x, ptSun.y, 14.0, 0, Math.PI * 2);
    ctx.fill();

    ctx.fillStyle = sunCss;
    ctx.strokeStyle = "#fef08a";
    ctx.lineWidth = 1.2;
    ctx.beginPath();
    ctx.arc(ptSun.x, ptSun.y, 5.5, 0, Math.PI * 2);
    ctx.fill();
    ctx.stroke();

    ctx.fillStyle = "#fef08a";
    ctx.font = "bold 10px sans-serif";
    ctx.fillText(sky.sunName || "Sun", ptSun.x + 8, ptSun.y - 4);
  }

  ctx.restore(); // End sky dome clip

  // Highlight active 100° vantage horizontal FOV sector on the sky dome horizon ring
  const activeNb = vantageCurrentNeighbor(view);
  if (activeNb) {
    const cAng = ((activeNb.azDeg - 90.0) * Math.PI) / 180.0;
    const halfFovRad = (50.0 * Math.PI) / 180.0;
    ctx.save();
    ctx.fillStyle = "rgba(56, 189, 248, 0.11)";
    ctx.beginPath();
    ctx.moveTo(cx, cy);
    ctx.arc(cx, cy, radius, cAng - halfFovRad, cAng + halfFovRad);
    ctx.closePath();
    ctx.fill();

    ctx.strokeStyle = "rgba(125, 211, 252, 0.88)";
    ctx.lineWidth = 2.2;
    ctx.beginPath();
    ctx.arc(cx, cy, radius, cAng - halfFovRad, cAng + halfFovRad);
    ctx.stroke();

    const tx = cx + Math.cos(cAng) * (radius + 3.5);
    const ty = cy + Math.sin(cAng) * (radius + 3.5);
    ctx.fillStyle = "#38bdf8";
    ctx.beginPath();
    ctx.arc(tx, ty, 2.4, 0, Math.PI * 2);
    ctx.fill();
    ctx.restore();
  }

  // Outer horizon ring & N / E / S / W cardinal labels
  ctx.strokeStyle = "#64748b";
  ctx.lineWidth = 1.6;
  ctx.beginPath();
  ctx.arc(cx, cy, radius, 0, Math.PI * 2);
  ctx.stroke();

  ctx.fillStyle = "#cbd5e1";
  ctx.font = "bold 10px sans-serif";
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillText("N", cx, cy - radius - 8);
  ctx.fillText("S", cx, cy + radius + 9);
  ctx.fillText("E", cx + radius + 9, cy);
  ctx.fillText("W", cx - radius - 10, cy);

  ctx.restore();
}

function wrapDeltaAzDeg(azDeg, centerAzDeg) {
  let d = ((azDeg - centerAzDeg) % 360.0 + 540.0) % 360.0 - 180.0;
  if (d === -180.0) d = 180.0;
  return d;
}

function vantageCurrentNeighbor(view) {
  if (!view || !view.tileVantage || !Array.isArray(view.tileVantage.neighbors)) {
    return null;
  }
  const nbs = view.tileVantage.neighbors;
  if (nbs.length === 0) return null;
  const idx = ((view.tileVantageNeighborIdx | 0) % nbs.length + nbs.length) % nbs.length;
  return {
    index: idx,
    count: nbs.length,
    azDeg: nbs[idx].azDeg || 0.0,
    compass: nbs[idx].compass || "N",
    label: nbs[idx].label || "",
    neighbor: nbs[idx],
  };
}

function vantageSummaryStrings(view) {
  const cur = vantageCurrentNeighbor(view);
  if (!cur) {
    return {
      bearingSummary: "Horizon (100°)",
      neighborSummary: "",
    };
  }
  const azRound = Math.round(((cur.azDeg % 360.0) + 360.0) % 360.0);
  if (view.tileVantage && view.tileVantage.obsIsCoarse) {
    const distKm = Math.max(20, Math.round(view.tileVantage.ring1DistKm || 300));
    return {
      bearingSummary: `${cur.compass} ${azRound}° (${cur.index + 1}/${cur.count})`,
      neighborSummary: `Facing ${cur.compass} (${azRound}°) — Local tile horizon only (coarse neighbors ~${distKm} km away, beyond horizon)`,
    };
  }
  return {
    bearingSummary: `${cur.compass} ${azRound}° (${cur.index + 1}/${cur.count})`,
    neighborSummary: `Facing ${cur.compass} (${azRound}°): ${cur.label}`,
  };
}

function vantageRockBaseRgb(rockName, rockKind, felsity) {
  const name = (rockName || "").toLowerCase();
  const kind = rockKind || "I";
  const f = typeof felsity === "number" ? Math.max(0.0, Math.min(1.0, felsity)) : 0.5;
  switch (name) {
    case "basalt":
      return f <= 0.35 ? [34, 62, 66] : [48, 56, 70];
    case "gabbro":
      return [38, 45, 58];
    case "andesite":
      return [82, 86, 108];
    case "diorite":
      return [74, 80, 98];
    case "dacite":
      return [116, 110, 132];
    case "granodiorite":
      return [148, 126, 138];
    case "rhyolite":
      return [184, 126, 122];
    case "granite":
      return [198, 140, 132];
    case "claystone":
      return [92, 96, 84];
    case "siltstone":
      return [118, 106, 86];
    case "sandstone":
      return [194, 146, 74];
    case "conglomerate":
      return [162, 106, 58];
    case "limestone":
      return [186, 184, 168];
    case "chalk":
      return [218, 215, 202];
    case "pelite":
      return [80, 88, 104];
    case "psammite":
      return [142, 116, 84];
    case "psephite":
      return [126, 90, 62];
    case "greenschist":
      return [58, 94, 72];
    case "blueschist":
      return [46, 76, 118];
    case "amphibolite":
      return [42, 66, 70];
    case "eclogite":
      return [40, 78, 60];
    case "hornfels":
      return [84, 62, 76];
    case "granulite":
      return [102, 72, 58];
    case "marble":
      return [228, 226, 216];
    default:
      if (kind === "S") return [144, 124, 86];
      if (kind === "M") return [82, 94, 112];
      return [
        Math.round(52 + 142 * f),
        Math.round(56 + 84 * f),
        Math.round(68 + 62 * f),
      ];
  }
}

// Continuous periodic world-azimuth ridge noise (periodic over 360° so rotating never shifts crags)
function vantageRidgeNoise(worldAzDeg, seed) {
  const rad = (worldAzDeg * Math.PI) / 180.0;
  const s = (seed || 0) * 0.713 + 1.37;
  const n1 = Math.sin(rad * 6.0 + s * 1.9) * 0.46;
  const n2 = Math.sin(rad * 13.0 - s * 3.1) * 0.28;
  const n3 = Math.cos(rad * 25.0 + s * 5.3) * 0.16;
  const n4 = (1.0 - Math.abs(Math.sin(rad * 19.0 + s * 2.7))) * 0.34 - 0.14;
  return n1 + n2 + n3 + n4;
}

function paintTileVantageCanvas(view) {
  const canvas = document.getElementById("tile-vantage-canvas");
  if (!canvas || !view || !view.tileSky || !view.tileVantage) return;
  const sky = view.tileSky;
  const vantage = view.tileVantage;

  const rect = canvas.getBoundingClientRect();
  const cssW = Math.max(160, Math.round(rect.width || 200));
  const cssH = Math.max(180, Math.round(rect.height || 224));
  const dpr = window.devicePixelRatio || 1;
  const targetW = Math.round(cssW * dpr);
  const targetH = Math.round(cssH * dpr);
  if (canvas.width !== targetW || canvas.height !== targetH) {
    canvas.width = targetW;
    canvas.height = targetH;
  }

  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  ctx.save();
  ctx.scale(dpr, dpr);

  // Proportional angular projection:
  // Horizontal FOV = 100° (-50°..+50° around active adjacent neighbor bearing)
  // Top edge = +90° (zenith straight overhead), bottom edge = -22° (foreground below horizon)
  const FOV_H_DEG = 100.0;
  const ALT_TOP_DEG = 90.0;
  const ALT_BOT_DEG = -22.0;
  const FOV_V_DEG = ALT_TOP_DEG - ALT_BOT_DEG;

  const curNb = vantageCurrentNeighbor(view);
  const viewAzDeg = curNb ? curNb.azDeg : 0.0;
  const altToY = (altDeg) => ((ALT_TOP_DEG - altDeg) / FOV_V_DEG) * cssH;
  const dAzToX = (dAzDeg) => (0.5 + dAzDeg / FOV_H_DEG) * cssW;
  const horizVecToAzAlt = (horiz) => {
    const east = horiz[0];
    const north = horiz[1];
    const up = Math.max(-1.0, Math.min(1.0, horiz[2]));
    const altDeg = (Math.asin(up) * 180.0) / Math.PI;
    const azDeg = (((Math.atan2(east, north) * 180.0) / Math.PI) % 360.0 + 360.0) % 360.0;
    return { azDeg, altDeg, up };
  };

  // Shared ephemeris state with paintTileSkyCanvas
  const latRad = ((sky.latDeg || 0.0) * Math.PI) / 180.0;
  const epsRad = ((sky.tiltDeg || 23.5) * Math.PI) / 180.0;
  const daysPerYear = Math.max(1, sky.daysPerYear || 365);
  const year = Math.max(0, view.tileSkyYear || 0);
  const day = Math.max(0, Math.min(daysPerYear - 1, view.tileSkyDay || 0));
  const tau = typeof view.tileSkyHourFrac === "number" ? view.tileSkyHourFrac : 0.42;
  const baseDays = year * daysPerYear + day;
  const totalDays = baseDays + tau;

  const sunNoonEq = sunEquatorialAtDays(sky, baseDays + 0.5, epsRad);
  const raNoon = Math.atan2(sunNoonEq[1], sunNoonEq[0]);
  const siderealNow = raNoon + 2.0 * Math.PI * (tau - 0.5);

  const sunEqNow = sunEquatorialAtDays(sky, totalDays, epsRad);
  const sunHorizNow = equatorialToHorizontal(sunEqNow, siderealNow, latRad);
  const sunAA = horizVecToAzAlt(sunHorizNow);
  const sunAltDeg = sunAA.altDeg;
  const sunAzDeg = sunAA.azDeg;

  const hasAtm = sky.hasAtmosphere !== false;
  const isCoarse = !!vantage.obsIsCoarse;
  const dayFactor = Math.max(0.0, Math.min(1.0, (sunAltDeg + 6.0) / 24.0));
  const twiFactor = hasAtm
    ? Math.max(0.0, Math.min(1.0, 1.0 - Math.abs(sunAltDeg + 2.0) / 12.0))
    : 0.0;
  const zRgb = sky.skyZenithRgb || [0.16, 0.38, 0.82];
  const hRgb = sky.skyHorizonRgb || [0.48, 0.72, 0.96];
  const sRgb = sky.sunsetRgb || [0.95, 0.45, 0.22];
  const tRgb = sky.twilightRgb || [0.18, 0.22, 0.45];

  const mixCh = (nightV, dayV, twiV) =>
    Math.round(
      Math.max(
        0,
        Math.min(
          255,
          (nightV * (1.0 - dayFactor) + dayV * dayFactor * 0.86 + twiV * twiFactor * 0.46) * 255.0
        )
      )
    );
  const zenRgb = hasAtm
    ? [mixCh(0.02, zRgb[0], tRgb[0]), mixCh(0.04, zRgb[1], tRgb[1]), mixCh(0.11, zRgb[2], tRgb[2])]
    : [4, 7, 15];
  const horRgb = hasAtm
    ? [mixCh(0.05, hRgb[0], sRgb[0]), mixCh(0.08, hRgb[1], sRgb[1]), mixCh(0.17, hRgb[2], sRgb[2])]
    : [9, 13, 24];

  const yHor0 = altToY(0.0);

  // 1. Sky backdrop from +90° zenith (y = 0) down through horizon
  const skyGrad = ctx.createLinearGradient(0, 0, 0, Math.min(cssH, yHor0 + 18));
  skyGrad.addColorStop(0.0, `rgb(${zenRgb[0]},${zenRgb[1]},${zenRgb[2]})`);
  skyGrad.addColorStop(0.62, `rgb(${Math.round(zenRgb[0] * 0.45 + horRgb[0] * 0.55)},${Math.round(zenRgb[1] * 0.45 + horRgb[1] * 0.55)},${Math.round(zenRgb[2] * 0.45 + horRgb[2] * 0.55)})`);
  skyGrad.addColorStop(1.0, `rgb(${horRgb[0]},${horRgb[1]},${horRgb[2]})`);
  ctx.fillStyle = skyGrad;
  ctx.fillRect(0, 0, cssW, cssH);

  // Directional solar / sunset horizon scatter glow when facing toward the Sun (atmospheric worlds only)
  const sunDAz = wrapDeltaAzDeg(sunAzDeg, viewAzDeg);
  if (hasAtm && sunAltDeg > -14.0 && Math.abs(sunDAz) < 88.0) {
    const sx = dAzToX(sunDAz);
    const sy = altToY(Math.max(-4.0, sunAltDeg));
    const glowStrength =
      Math.max(0.0, 1.0 - Math.abs(sunDAz) / 88.0) *
      (twiFactor * 0.58 + dayFactor * 0.24);
    if (glowStrength > 0.01) {
      const gR = Math.round((sRgb[0] * 0.7 + (sky.sunRgb ? sky.sunRgb[0] : 1.0) * 0.3) * 255);
      const gG = Math.round((sRgb[1] * 0.7 + (sky.sunRgb ? sky.sunRgb[1] : 0.9) * 0.3) * 255);
      const gB = Math.round((sRgb[2] * 0.7 + (sky.sunRgb ? sky.sunRgb[2] : 0.6) * 0.3) * 255);
      const radGlow = ctx.createRadialGradient(sx, sy, 2.0, sx, sy, cssW * 0.78);
      radGlow.addColorStop(0.0, `rgba(${gR},${gG},${gB},${Math.min(0.65, glowStrength).toFixed(3)})`);
      radGlow.addColorStop(1.0, `rgba(${gR},${gG},${gB},0)`);
      ctx.fillStyle = radGlow;
      ctx.fillRect(0, 0, cssW, cssH);
    }
  }

  // 2. Stars & Constellations in horizontal (az, alt) space
  const starAlpha = hasAtm ? Math.max(0.18, 1.0 - dayFactor * 0.76) : 0.94;
  if (sky.constellations) {
    for (const c of sky.constellations) {
      const projStars = (c.stars || []).map((st) => {
        const eq = raDecDegToEquatorial(st[0], st[1]);
        const horiz = equatorialToHorizontal(eq, siderealNow, latRad);
        const aa = horizVecToAzAlt(horiz);
        const dAz = wrapDeltaAzDeg(aa.azDeg, viewAzDeg);
        return {
          x: dAzToX(dAz),
          y: altToY(aa.altDeg),
          dAz,
          altDeg: aa.altDeg,
          mag: st[2] || 1.8,
        };
      });

      ctx.strokeStyle = `rgba(56, 189, 248, ${(0.55 * starAlpha).toFixed(3)})`;
      ctx.lineWidth = 1.0;
      for (const edge of c.edges || []) {
        const a = projStars[edge[0]];
        const b = projStars[edge[1]];
        if (
          a &&
          b &&
          (a.altDeg >= -4.0 || b.altDeg >= -4.0) &&
          Math.abs(a.dAz) <= 62.0 &&
          Math.abs(b.dAz) <= 62.0 &&
          Math.abs(a.dAz - b.dAz) < 92.0
        ) {
          ctx.beginPath();
          ctx.moveTo(a.x, a.y);
          ctx.lineTo(b.x, b.y);
          ctx.stroke();
        }
      }

      for (const ps of projStars) {
        if (ps.altDeg < -4.0 || Math.abs(ps.dAz) > 53.0) continue;
        const rStar = Math.max(1.1, 2.5 - 0.42 * ps.mag);
        ctx.fillStyle = `rgba(241, 245, 249, ${(0.95 * starAlpha).toFixed(3)})`;
        ctx.beginPath();
        ctx.arc(ps.x, ps.y, rStar, 0, Math.PI * 2);
        ctx.fill();
      }

      if (
        c.name &&
        projStars.length > 0 &&
        projStars[0].altDeg > 6.0 &&
        projStars[0].altDeg < 85.0 &&
        Math.abs(projStars[0].dAz) < 42.0
      ) {
        ctx.fillStyle = `rgba(125, 211, 252, ${(0.78 * starAlpha).toFixed(3)})`;
        ctx.font = "8.5px sans-serif";
        ctx.fillText(c.name, projStars[0].x + 4, projStars[0].y - 3);
      }
    }
  }

  // 3. Planets in (az, alt)
  if (sky.planets) {
    for (const p of sky.planets) {
      const pEq = planetEquatorialAtDays(p, sky, totalDays, epsRad);
      const pHoriz = equatorialToHorizontal(pEq, siderealNow, latRad);
      const aa = horizVecToAzAlt(pHoriz);
      const dAz = wrapDeltaAzDeg(aa.azDeg, viewAzDeg);
      if (aa.altDeg >= -4.0 && Math.abs(dAz) <= 54.0) {
        const px = dAzToX(dAz);
        const py = altToY(aa.altDeg);
        const pCss = rgbFloatToCss(p.rgb, 240, 190, 110);
        if (p.hasRings) {
          ctx.strokeStyle = "rgba(253, 230, 138, 0.85)";
          ctx.lineWidth = 1.0;
          ctx.beginPath();
          ctx.ellipse(px, py, 5.8, 2.2, -0.35, 0, Math.PI * 2);
          ctx.stroke();
        }
        ctx.fillStyle = pCss;
        ctx.strokeStyle = "#0f172a";
        ctx.lineWidth = 0.9;
        ctx.beginPath();
        ctx.arc(px, py, 3.0, 0, Math.PI * 2);
        ctx.fill();
        ctx.stroke();
        if (Math.abs(dAz) < 44.0) {
          ctx.fillStyle = "#fde68a";
          ctx.font = "8.5px sans-serif";
          ctx.fillText(p.name, px + 4, py + 3);
        }
      }
    }
  }

  // 4. Moons in (az, alt) + track brightest moon illumination for nocturnal landscape light
  let nightMoonBoost = 0.0;
  let primaryMoonWaterRefl = null;
  if (sky.moons) {
    for (const m of sky.moons) {
      const mEq = moonEquatorialAtDays(m, totalDays, epsRad);
      const mHoriz = equatorialToHorizontal(mEq, siderealNow, latRad);
      const aa = horizVecToAzAlt(mHoriz);
      const dAz = wrapDeltaAzDeg(aa.azDeg, viewAzDeg);
      const cosElong = mEq[0] * sunEqNow[0] + mEq[1] * sunEqNow[1] + mEq[2] * sunEqNow[2];
      const illum = 0.5 * (1.0 - cosElong);
      if (aa.altDeg > 0.0) {
        nightMoonBoost = Math.max(nightMoonBoost, illum * Math.min(1.0, aa.altDeg / 25.0));
        if (!primaryMoonWaterRefl && Math.abs(dAz) <= 48.0 && illum > 0.22) {
          primaryMoonWaterRefl = { dAz, altDeg: aa.altDeg, illum };
        }
      }
      if (aa.altDeg >= -4.0 && Math.abs(dAz) <= 55.0) {
        const mx = dAzToX(dAz);
        const my = altToY(aa.altDeg);
        const rMoon = Math.max(3.6, 5.2 - m.index * 0.5);
        ctx.fillStyle = "#1e293b";
        ctx.strokeStyle = "#94a3b8";
        ctx.lineWidth = 0.9;
        ctx.beginPath();
        ctx.arc(mx, my, rMoon, 0, Math.PI * 2);
        ctx.fill();
        ctx.stroke();

        ctx.fillStyle = "#f1f5f9";
        ctx.beginPath();
        ctx.arc(mx, my, rMoon * Math.max(0.2, illum), 0, Math.PI * 2);
        ctx.fill();

        if (Math.abs(dAz) < 44.0) {
          ctx.fillStyle = "#e2e8f0";
          ctx.font = "8.5px sans-serif";
          ctx.fillText(m.name, mx + rMoon + 3, my - 2);
        }
      }
    }
  }

  // 5. Sun in (az, alt) (drawn BEFORE horizon & mountains so it sets cleanly behind peaks!)
  const sunCss = rgbFloatToCss(sky.sunRgb, 255, 228, 132);
  if (sunAltDeg >= -5.0 && Math.abs(sunDAz) <= 58.0) {
    const sx = dAzToX(sunDAz);
    const sy = altToY(sunAltDeg);
    const sunGlow = ctx.createRadialGradient(sx, sy, 1.5, sx, sy, 15.0);
    sunGlow.addColorStop(0.0, "#ffffff");
    sunGlow.addColorStop(0.35, sunCss);
    sunGlow.addColorStop(1.0, "rgba(250, 204, 21, 0.0)");
    ctx.fillStyle = sunGlow;
    ctx.beginPath();
    ctx.arc(sx, sy, 15.0, 0, Math.PI * 2);
    ctx.fill();

    ctx.fillStyle = sunCss;
    ctx.strokeStyle = "#fef08a";
    ctx.lineWidth = 1.1;
    ctx.beginPath();
    ctx.arc(sx, sy, 5.2, 0, Math.PI * 2);
    ctx.fill();
    ctx.stroke();

    if (Math.abs(sunDAz) < 43.0) {
      ctx.fillStyle = "#fef08a";
      ctx.font = "bold 9px sans-serif";
      ctx.fillText(sky.sunName || "Sun", sx + 7, sy - 4);
    }
  }

  // =========================================================================
  // 6. Accurate Horizon, Sea, Distant Peaks, Adjacent Neighbors & Foreground
  // =========================================================================
  const R = Math.max(500.0, vantage.planetRadiusKm || 6371.0);
  const ring1Km = Math.max(8.0, vantage.ring1DistKm || 100.0);
  const obsIsLand = !hasAtm || !!vantage.obsIsLand;
  const obsGroundKm = obsIsLand ? Math.max(0.0, vantage.obsElevKm || 0.0) : 0.0;
  const hObsKm = obsGroundKm + 0.015;
  const pfRgb = vantage.plantFoliageRgb || [0.18, 0.52, 0.20];
  const phRgb = vantage.plantHighlightRgb || [0.34, 0.68, 0.28];
  const plantCssRgb = [
    Math.round(Math.max(0, Math.min(255, (pfRgb[0] * 0.72 + phRgb[0] * 0.28) * 255))),
    Math.round(Math.max(0, Math.min(255, (pfRgb[1] * 0.72 + phRgb[1] * 0.28) * 255))),
    Math.round(Math.max(0, Math.min(255, (pfRgb[2] * 0.72 + phRgb[2] * 0.28) * 255))),
  ];
  const absLat = Math.abs(sky.latDeg || 0.0);
  const snowlineKm = Math.max(0.8, 4.7 - 0.048 * absLat);

  // Spherical sea-horizon depression angle from observer's elevation
  const seaDipDeg = -Math.min(
    7.8,
    Math.sqrt((2.0 * Math.max(0.002, hObsKm)) / R) * (180.0 / Math.PI) * 2.8
  );
  const ySeaHor = altToY(seaDipDeg);

  // Ambient + direct illumination factor for terrain at world azimuth `az`
  const baseLight = hasAtm
    ? 0.16 + 0.14 * nightMoonBoost + 0.78 * dayFactor + 0.14 * twiFactor
    : 0.08 + 0.12 * nightMoonBoost + 0.90 * dayFactor;
  const sunTintRgb = [
    (sky.sunRgb ? sky.sunRgb[0] : 1.0) * (1.0 - twiFactor * 0.45) + sRgb[0] * twiFactor * 0.45,
    (sky.sunRgb ? sky.sunRgb[1] : 0.96) * (1.0 - twiFactor * 0.45) + sRgb[1] * twiFactor * 0.45,
    (sky.sunRgb ? sky.sunRgb[2] : 0.88) * (1.0 - twiFactor * 0.45) + sRgb[2] * twiFactor * 0.45,
  ];
  const shadeTerrainRgb = (rgb, worldAz, slopeContrast, hazeFrac) => {
    const azDiffRad = (wrapDeltaAzDeg(worldAz, sunAzDeg) * Math.PI) / 180.0;
    const direct =
      dayFactor > 0.02
        ? Math.max(-0.35, Math.cos(azDiffRad)) * slopeContrast * Math.min(1.0, (sunAltDeg + 6.0) / 30.0)
        : 0.0;
    const lit = Math.max(hasAtm ? 0.11 : 0.06, Math.min(1.32, baseLight + direct));
    const rLit = rgb[0] * lit * (0.86 + 0.18 * sunTintRgb[0]);
    const gLit = rgb[1] * lit * (0.86 + 0.18 * sunTintRgb[1]);
    const bLit = rgb[2] * lit * (0.88 + 0.16 * sunTintRgb[2]);
    const hz = hasAtm ? Math.max(0.0, Math.min(0.84, hazeFrac)) : 0.0;
    const rOut = Math.round(Math.max(0, Math.min(255, rLit * (1.0 - hz) + horRgb[0] * hz)));
    const gOut = Math.round(Math.max(0, Math.min(255, gLit * (1.0 - hz) + horRgb[1] * hz)));
    const bOut = Math.round(Math.max(0, Math.min(255, bLit * (1.0 - hz) + horRgb[2] * hz)));
    return `rgb(${rOut},${gOut},${bOut})`;
  };

  const blendTileSurfaceRgb = (rockRgb, vegFrac, mountNorm, koeppen, elevKm) => {
    if (hasAtm && (koeppen === 12 || koeppen === 13 || elevKm > snowlineKm + 0.65)) {
      return [228, 236, 246];
    }
    const effVeg = hasAtm ? vegFrac : 0.0;
    const vegCover = Math.max(0.0, Math.min(0.88, effVeg * (0.88 - 0.36 * mountNorm)));
    let r = rockRgb[0] * (1.0 - vegCover) + plantCssRgb[0] * vegCover;
    let g = rockRgb[1] * (1.0 - vegCover) + plantCssRgb[1] * vegCover;
    let b = rockRgb[2] * (1.0 - vegCover) + plantCssRgb[2] * vegCover;
    if (hasAtm && (koeppen === 4 || koeppen === 5)) {
      // Warm arid regolith / desert sand tint when sparsely vegetated
      const aridW = (1.0 - vegCover) * 0.28;
      r = r * (1.0 - aridW) + 208 * aridW;
      g = g * (1.0 - aridW) + 172 * aridW;
      b = b * (1.0 - aridW) + 112 * aridW;
    }
    return [r, g, b];
  };

  // 6a. Base Sea / Ocean horizon plane (only when planet has an atmosphere & water may be visible)
  if (hasAtm && (!isCoarse || !obsIsLand)) {
    const seaSkyR = Math.round((0.04 + 0.42 * zRgb[0]) * 255 * Math.max(0.18, baseLight));
    const seaSkyG = Math.round((0.09 + 0.50 * zRgb[1]) * 255 * Math.max(0.18, baseLight));
    const seaSkyB = Math.round((0.20 + 0.64 * zRgb[2]) * 255 * Math.max(0.20, baseLight));
    const seaHorR = Math.round(seaSkyR * 0.55 + horRgb[0] * 0.45);
    const seaHorG = Math.round(seaSkyG * 0.55 + horRgb[1] * 0.45);
    const seaHorB = Math.round(seaSkyB * 0.55 + horRgb[2] * 0.45);
    const seaGrad = ctx.createLinearGradient(0, ySeaHor, 0, cssH);
    seaGrad.addColorStop(0.0, `rgb(${seaHorR},${seaHorG},${seaHorB})`);
    seaGrad.addColorStop(0.45, `rgb(${seaSkyR},${seaSkyG},${seaSkyB})`);
    seaGrad.addColorStop(
      1.0,
      `rgb(${Math.round(seaSkyR * 0.68)},${Math.round(seaSkyG * 0.72)},${Math.round(seaSkyB * 0.78)})`
    );
    ctx.fillStyle = seaGrad;
    ctx.fillRect(0, ySeaHor, cssW, cssH - ySeaHor);

    // Subtle ocean wave shimmer & solar/lunar water reflection track
    ctx.strokeStyle = `rgba(186, 230, 253, ${(0.10 + 0.14 * dayFactor).toFixed(3)})`;
    ctx.lineWidth = 0.75;
    for (let wIdx = 0; wIdx < 7; wIdx++) {
      const tW = Math.pow((wIdx + 0.5) / 7.2, 1.7);
      const wy = ySeaHor + 2.0 + tW * (cssH - ySeaHor - 4.0);
      ctx.beginPath();
      for (let seg = 0; seg < 6; seg++) {
        const ph = ((viewAzDeg * 0.04 + seg * 0.19 + wIdx * 0.31) % 1.0 + 1.0) % 1.0;
        const wx0 = ph * cssW;
        const wLen = 8.0 + tW * 18.0;
        ctx.moveTo(wx0, wy);
        ctx.lineTo(Math.min(cssW, wx0 + wLen), wy);
      }
      ctx.stroke();
    }
    if (sunAltDeg > -2.0 && Math.abs(sunDAz) < 52.0) {
      const sx = dAzToX(sunDAz);
      for (let rIdx = 0; rIdx < 9; rIdx++) {
        const tr = rIdx / 8.0;
        const ry = ySeaHor + 1.5 + tr * tr * (cssH - ySeaHor - 3.0);
        const rw = 2.5 + tr * 11.0;
        ctx.fillStyle = `rgba(254, 240, 138, ${(0.34 * (1.0 - tr * 0.55)).toFixed(3)})`;
        ctx.fillRect(sx - rw * 0.5, ry, rw, 1.3);
      }
    } else if (primaryMoonWaterRefl) {
      const mx = dAzToX(primaryMoonWaterRefl.dAz);
      for (let rIdx = 0; rIdx < 7; rIdx++) {
        const tr = rIdx / 6.0;
        const ry = ySeaHor + 2.0 + tr * tr * (cssH - ySeaHor - 4.0);
        const rw = 2.0 + tr * 8.0;
        ctx.fillStyle = `rgba(226, 232, 240, ${(0.24 * primaryMoonWaterRefl.illum * (1.0 - tr * 0.5)).toFixed(3)})`;
        ctx.fillRect(mx - rw * 0.5, ry, rw, 1.1);
      }
    }
  }

  // When `isCoarse` is true (pre-subdivided Tectonics/Prehistory or coarse OceanCoarse tiles),
  // coarse neighbors are hundreds of km away beyond planetary curvature, so we NEVER render
  // neighboring peaks/water — only the selected tile's own local 360° horizon!
  if (isCoarse && obsIsLand) {
    const obsRockRgb = vantageRockBaseRgb(
      vantage.obsRockName,
      vantage.obsRockKind,
      vantage.obsFelsity
    );
    const obsMount = Math.max(0.0, Math.min(1.0, vantage.obsMountainosityNorm || 0.0));
    const obsElev = vantage.obsElevKm || 0.0;
    const obsKoeppen = hasAtm ? vantage.obsKoeppen || 0 : 0;
    const obsSurfRgb = blendTileSurfaceRgb(
      obsRockRgb,
      vantage.obsVegFrac || 0.0,
      obsMount,
      obsKoeppen,
      obsElev
    );
    const seedBase = (view.selectedTileIdx || 0) + 19;
    const N_COLS = 96;

    // Layer A: Far local horizon within this coarse tile (~10-14 km out)
    const farPts = [];
    for (let i = 0; i <= N_COLS; i++) {
      const dAz = -50.0 + (100.0 * i) / N_COLS;
      const wAz = ((viewAzDeg + dAz) % 360.0 + 360.0) % 360.0;
      const crag = vantageRidgeNoise(wAz, seedBase);
      const crag2 = vantageRidgeNoise(wAz * 2.0 + 41.0, seedBase + 13);
      const reliefKm =
        obsMount * (0.45 + 2.35 * Math.max(0.0, 0.52 + crag + 0.25 * crag2)) +
        0.035 * (0.5 + crag);
      const altDeg =
        seaDipDeg * (1.0 - 0.5 * obsMount) +
        (Math.atan2(reliefKm, 12.5) * 180.0) / Math.PI;
      farPts.push({
        x: dAzToX(dAz),
        y: altToY(altDeg),
        wAz,
        peakKm: Math.max(0.0, obsElev) + reliefKm,
      });
    }
    for (let j = 0; j < farPts.length - 1; j++) {
      const a = farPts[j];
      const b = farPts[j + 1];
      const slope = (a.y - b.y) * 0.16;
      ctx.fillStyle = shadeTerrainRgb(
        obsSurfRgb,
        a.wAz - slope * 20.0,
        0.24 + 0.32 * obsMount,
        0.26
      );
      ctx.beginPath();
      ctx.moveTo(a.x - 0.4, cssH);
      ctx.lineTo(a.x - 0.4, a.y);
      ctx.lineTo(b.x + 0.4, b.y);
      ctx.lineTo(b.x + 0.4, cssH);
      ctx.closePath();
      ctx.fill();

      if (obsMount > 0.16) {
        const rockFrac = Math.min(0.58, 0.20 + 0.48 * obsMount);
        const ayMid = a.y + (altToY(-4.5) - a.y) * rockFrac;
        const byMid = b.y + (altToY(-4.5) - b.y) * rockFrac;
        ctx.fillStyle = shadeTerrainRgb(obsRockRgb, a.wAz - slope * 26.0, 0.38, 0.20);
        ctx.beginPath();
        ctx.moveTo(a.x - 0.3, ayMid);
        ctx.lineTo(a.x - 0.3, a.y);
        ctx.lineTo(b.x + 0.3, b.y);
        ctx.lineTo(b.x + 0.3, byMid);
        ctx.closePath();
        ctx.fill();
      }

      if (hasAtm && (obsKoeppen === 12 || obsKoeppen === 13 || a.peakKm > snowlineKm)) {
        const snowFrac =
          obsKoeppen === 12 || obsKoeppen === 13
            ? 0.65
            : Math.min(0.42, (a.peakKm - snowlineKm) * 0.24);
        const ayS = a.y + (altToY(-4.5) - a.y) * snowFrac;
        const byS = b.y + (altToY(-4.5) - b.y) * snowFrac;
        ctx.fillStyle = shadeTerrainRgb([238, 244, 252], a.wAz, 0.24, 0.16);
        ctx.beginPath();
        ctx.moveTo(a.x - 0.3, ayS);
        ctx.lineTo(a.x - 0.3, a.y);
        ctx.lineTo(b.x + 0.3, b.y);
        ctx.lineTo(b.x + 0.3, byS);
        ctx.closePath();
        ctx.fill();
      }
    }
    ctx.strokeStyle = "rgba(15, 23, 42, 0.48)";
    ctx.lineWidth = 0.85;
    ctx.beginPath();
    ctx.moveTo(farPts[0].x, farPts[0].y);
    for (let j = 1; j < farPts.length; j++) ctx.lineTo(farPts[j].x, farPts[j].y);
    ctx.stroke();

    // Layer B: Mid-ground local ridge (~3-5 km within this coarse tile)
    const midSteps = 64;
    ctx.fillStyle = shadeTerrainRgb(obsSurfRgb, viewAzDeg + 12.0, 0.26, 0.10);
    ctx.beginPath();
    ctx.moveTo(0, cssH);
    for (let k = 0; k <= midSteps; k++) {
      const dAz = -50.0 + (100.0 * k) / midSteps;
      const wAz = ((viewAzDeg + dAz) % 360.0 + 360.0) % 360.0;
      const und = vantageRidgeNoise(wAz + 85.0, seedBase + 47);
      const midAlt = -2.8 + (0.35 + 4.8 * obsMount) * (0.25 + und);
      ctx.lineTo(dAzToX(dAz), altToY(midAlt));
    }
    ctx.lineTo(cssW, cssH);
    ctx.closePath();
    ctx.fill();
  }

  // 6b. Distant & Mid-ground Land Tiles (2..5 rings away, sorted farthest to nearest; subdivided tiles only)
  const samples = !isCoarse && Array.isArray(vantage.samples) ? vantage.samples : [];
  for (const smp of samples) {
    if (smp.isAdjacent || !smp.isLand) continue;
    const dAzCenter = wrapDeltaAzDeg(smp.azDeg, viewAzDeg);
    const spanDeg = Math.max(4.5, (smp.halfWidthDeg || 10.0) * 1.38);
    if (Math.abs(dAzCenter) > 50.0 + spanDeg) continue;

    // True spherical curvature drop (km) at distance `smp.distKm` on planet of radius `R`
    const curvDropKm = (smp.distKm * smp.distKm) / (2.0 * R);
    const rRatio = Math.max(1.2, smp.distKm / ring1Km);
    const dVisKm = 14.5 * rRatio;
    const peakPotKm =
      smp.elevKm * (1.0 + 0.68 * smp.mountainosityNorm) + 0.45 * smp.mountainosityNorm;
    const maxAltEst =
      (Math.atan2(peakPotKm - hObsKm - curvDropKm * 0.58, dVisKm) * 180.0) / Math.PI;
    if (maxAltEst < seaDipDeg - 0.8) continue;

    const rockRgb = vantageRockBaseRgb(smp.rockName, smp.rockKind, smp.felsity);
    const surfRgb = blendTileSurfaceRgb(
      rockRgb,
      smp.vegFrac,
      smp.mountainosityNorm,
      smp.koeppen,
      smp.elevKm
    );
    const haze = Math.min(0.78, 1.0 - Math.exp(-smp.distKm / (ring1Km * 2.85)));
    const fillCss = shadeTerrainRgb(surfRgb, smp.azDeg, 0.24 + 0.28 * smp.mountainosityNorm, haze);

    const steps = 18;
    const yBase = Math.min(cssH, ySeaHor + 3.0);
    ctx.beginPath();
    ctx.moveTo(dAzToX(dAzCenter - spanDeg), yBase);
    let anyAbove = false;
    const ridgePts = [];
    for (let k = 0; k <= steps; k++) {
      const u = (k / steps) * 2.0 - 1.0; // -1..+1 across tile massif
      const env = Math.pow(Math.max(0.0, 1.0 - u * u), 0.72 - 0.22 * smp.mountainosityNorm);
      const wAz = smp.azDeg + u * spanDeg;
      const crag = vantageRidgeNoise(wAz, smp.displayIdx + 17);
      const localH =
        env *
        Math.max(
          0.02,
          smp.elevKm * (1.0 + 0.68 * smp.mountainosityNorm * (0.45 + crag)) +
            0.38 * smp.mountainosityNorm * Math.max(0.0, crag + 0.35)
        );
      const altDeg =
        (Math.atan2(localH - hObsKm * env - curvDropKm * 0.58, dVisKm) * 180.0) / Math.PI;
      const px = dAzToX(dAzCenter + u * spanDeg);
      const py = Math.min(yBase, altToY(altDeg));
      if (py < ySeaHor - 0.4) anyAbove = true;
      ridgePts.push({ px, py, localH });
      ctx.lineTo(px, py);
    }
    ctx.lineTo(dAzToX(dAzCenter + spanDeg), yBase);
    ctx.closePath();
    if (anyAbove) {
      ctx.fillStyle = fillCss;
      ctx.fill();

      // Snowcap or bare-rock upper crest accent on tall distant peaks
      if (smp.koeppen === 12 || smp.koeppen === 13 || peakPotKm > snowlineKm) {
        const capCss = shadeTerrainRgb([236, 242, 250], smp.azDeg, 0.22, haze * 0.85);
        ctx.strokeStyle = capCss;
        ctx.lineWidth = 1.2;
        ctx.beginPath();
        let pen = false;
        for (const rp of ridgePts) {
          if (rp.localH > snowlineKm * 0.92 && rp.py < ySeaHor - 0.5) {
            if (!pen) {
              ctx.moveTo(rp.px, rp.py + 0.7);
              pen = true;
            } else {
              ctx.lineTo(rp.px, rp.py + 0.7);
            }
          } else {
            pen = false;
          }
        }
        ctx.stroke();
      }
    }
  }

  // 6c. 1-Ring Adjacent Neighbor Tiles (continuous 360° horizon & coastal relief)
  const adjSamples = samples
    .filter((s) => s.isAdjacent)
    .slice()
    .sort((a, b) => a.azDeg - b.azDeg);

  if (adjSamples.length > 0) {
    const N_COLS = 110;
    const dVis1Km = 13.5;
    const colData = [];

    for (let i = 0; i <= N_COLS; i++) {
      const dAz = -50.0 + (100.0 * i) / N_COLS;
      const worldAz = ((viewAzDeg + dAz) % 360.0 + 360.0) % 360.0;

      // Smooth angular kernel across adjacent tiles (sigma ≈ 24° bridges ~60° neighbor centers cleanly)
      let wSum = 0.0;
      let landW = 0.0;
      let elevSum = 0.0;
      let mountSum = 0.0;
      let vegSum = 0.0;
      let snowW = 0.0;
      let riverW = 0.0;
      let rRock = 0.0;
      let gRock = 0.0;
      let bRock = 0.0;
      let rSurf = 0.0;
      let gSurf = 0.0;
      let bSurf = 0.0;

      for (const adj of adjSamples) {
        const ang = Math.abs(wrapDeltaAzDeg(worldAz, adj.azDeg));
        const halfSpan = Math.max(26.0, adj.halfWidthDeg || 32.0);
        if (ang > halfSpan * 1.65) continue;
        const t = ang / (halfSpan * 0.78);
        const w = Math.exp(-t * t * 1.45);
        wSum += w;
        if (adj.isLand) {
          landW += w;
          const rk = vantageRockBaseRgb(adj.rockName, adj.rockKind, adj.felsity);
          const sf = blendTileSurfaceRgb(
            rk,
            adj.vegFrac,
            adj.mountainosityNorm,
            adj.koeppen,
            adj.elevKm
          );
          elevSum += w * Math.max(0.02, adj.elevKm);
          mountSum += w * adj.mountainosityNorm;
          vegSum += w * adj.vegFrac;
          if (adj.koeppen === 12 || adj.koeppen === 13) snowW += w;
          if (adj.isRiver) riverW += w;
          rRock += w * rk[0];
          gRock += w * rk[1];
          bRock += w * rk[2];
          rSurf += w * sf[0];
          gSurf += w * sf[1];
          bSurf += w * sf[2];
        }
      }

      const landFrac = wSum > 1e-5 ? landW / wSum : 0.0;
      if (landFrac > 0.14 && landW > 1e-5) {
        const meanElev = elevSum / landW;
        const meanMount = mountSum / landW;
        const meanVeg = vegSum / landW;
        const snowFrac = snowW / landW;
        const riverFrac = riverW / landW;
        const coastTaper = Math.min(1.0, (landFrac - 0.14) / 0.42);
        const crag = vantageRidgeNoise(worldAz, (view.selectedTileIdx || 0) + 5);
        const cragFine = vantageRidgeNoise(worldAz * 2.0 + 37.0, (view.selectedTileIdx || 0) + 29);

        const crestKm =
          coastTaper *
          (meanElev * (1.0 + 0.72 * meanMount * (0.38 + crag)) +
            0.46 * meanMount * Math.max(0.0, crag + 0.35 + 0.25 * cragFine) +
            0.04 * (0.5 + crag));
        const altCrestDeg = (Math.atan2(crestKm - hObsKm * coastTaper, dVis1Km) * 180.0) / Math.PI;
        // Coastline shoreline base meets the water when bordering sea, or extends into foreground when inland
        const shoreAltDeg = obsIsLand
          ? -6.8 * coastTaper + (seaDipDeg - 1.4) * (1.0 - coastTaper)
          : seaDipDeg - 0.35 * coastTaper;

        colData.push({
          hasLand: true,
          x: dAzToX(dAz),
          worldAz,
          crestKm,
          meanMount,
          meanVeg,
          snowFrac,
          riverFrac,
          yCrest: altToY(altCrestDeg),
          yBase: altToY(shoreAltDeg),
          rockRgb: [rRock / landW, gRock / landW, bRock / landW],
          surfRgb: [rSurf / landW, gSurf / landW, bSurf / landW],
        });
      } else {
        colData.push({
          hasLand: false,
          x: dAzToX(dAz),
          worldAz,
        });
      }
    }

    // Draw contiguous 1-ring land spans with lithology crags, plant cover, snowline & coastal beaches
    let idx = 0;
    while (idx < colData.length) {
      if (!colData[idx].hasLand) {
        idx++;
        continue;
      }
      let endIdx = idx;
      while (endIdx + 1 < colData.length && colData[endIdx + 1].hasLand) {
        endIdx++;
      }
      const seg = colData.slice(idx, endIdx + 1);
      if (seg.length >= 2) {
        // Draw column-by-column quads so color smoothly transitions from e.g. lush forest neighbor to sandstone desert neighbor!
        for (let j = 0; j < seg.length - 1; j++) {
          const a = seg[j];
          const b = seg[j + 1];
          const slopeDeg = (a.yCrest - b.yCrest) * 0.14;
          const colCss = shadeTerrainRgb(
            a.surfRgb,
            a.worldAz - slopeDeg * 18.0,
            0.28 + 0.30 * a.meanMount,
            0.12
          );
          ctx.fillStyle = colCss;
          ctx.beginPath();
          ctx.moveTo(a.x - 0.4, a.yBase);
          ctx.lineTo(a.x - 0.4, a.yCrest);
          ctx.lineTo(b.x + 0.4, b.yCrest);
          ctx.lineTo(b.x + 0.4, b.yBase);
          ctx.closePath();
          ctx.fill();

          // Upper bare-rock crag facets when mountainosity > 0.16 (shows true rock lithology color!)
          if (a.meanMount > 0.16) {
            const rockFrac = Math.min(0.62, 0.22 + 0.52 * a.meanMount);
            const ayMid = a.yCrest + (a.yBase - a.yCrest) * rockFrac;
            const byMid = b.yCrest + (b.yBase - b.yCrest) * rockFrac;
            ctx.fillStyle = shadeTerrainRgb(
              a.rockRgb,
              a.worldAz - slopeDeg * 24.0,
              0.38,
              0.10
            );
            ctx.beginPath();
            ctx.moveTo(a.x - 0.3, ayMid);
            ctx.lineTo(a.x - 0.3, a.yCrest);
            ctx.lineTo(b.x + 0.3, b.yCrest);
            ctx.lineTo(b.x + 0.3, byMid);
            ctx.closePath();
            ctx.fill();
          }

          // Alpine snow / glacier cap on high peaks
          if (a.snowFrac > 0.45 || a.crestKm > snowlineKm) {
            const snowDepth = a.snowFrac > 0.45 ? 0.65 : Math.min(0.42, (a.crestKm - snowlineKm) * 0.24);
            const aySnow = a.yCrest + (a.yBase - a.yCrest) * snowDepth;
            const bySnow = b.yCrest + (b.yBase - b.yCrest) * snowDepth;
            ctx.fillStyle = shadeTerrainRgb([238, 244, 252], a.worldAz, 0.24, 0.08);
            ctx.beginPath();
            ctx.moveTo(a.x - 0.3, aySnow);
            ctx.lineTo(a.x - 0.3, a.yCrest);
            ctx.lineTo(b.x + 0.3, b.yCrest);
            ctx.lineTo(b.x + 0.3, bySnow);
            ctx.closePath();
            ctx.fill();
          }

          // River valley glint when facing a riverine neighbor
          if (a.riverFrac > 0.42) {
            const ry = a.yBase - (a.yBase - a.yCrest) * 0.16;
            ctx.fillStyle = `rgba(56, 189, 248, ${(0.55 * Math.max(0.3, baseLight)).toFixed(3)})`;
            ctx.fillRect(a.x, ry, Math.max(1.5, b.x - a.x + 0.5), 1.8);
          }
        }

        // Crisp silhouette ridge line along the 1-ring crest
        ctx.strokeStyle = "rgba(15, 23, 42, 0.52)";
        ctx.lineWidth = 0.9;
        ctx.beginPath();
        ctx.moveTo(seg[0].x, seg[0].yCrest);
        for (let j = 1; j < seg.length; j++) {
          ctx.lineTo(seg[j].x, seg[j].yCrest);
        }
        ctx.stroke();
      }
      idx = endIdx + 1;
    }
  }

  // 6d. Immediate Foreground (Observer's own clicked tile `obs_*` at bottom of vantage frame)
  if (obsIsLand) {
    const obsRockRgb = vantageRockBaseRgb(
      vantage.obsRockName,
      vantage.obsRockKind,
      vantage.obsFelsity
    );
    const obsSurfRgb = blendTileSurfaceRgb(
      obsRockRgb,
      vantage.obsVegFrac || 0.0,
      vantage.obsMountainosityNorm || 0.0,
      vantage.obsKoeppen || 0,
      vantage.obsElevKm || 0.0
    );
    const fgTopCss = shadeTerrainRgb(obsSurfRgb, viewAzDeg, 0.22, 0.03);
    const fgBotRgb = [
      obsSurfRgb[0] * 0.76 + obsRockRgb[0] * 0.24,
      obsSurfRgb[1] * 0.76 + obsRockRgb[1] * 0.24,
      obsSurfRgb[2] * 0.76 + obsRockRgb[2] * 0.24,
    ];
    const fgBotCss = shadeTerrainRgb(fgBotRgb, viewAzDeg + 20.0, 0.18, 0.0);
    const fgGrad = ctx.createLinearGradient(0, altToY(-5.8), 0, cssH);
    fgGrad.addColorStop(0.0, fgTopCss);
    fgGrad.addColorStop(1.0, fgBotCss);

    const fgSteps = 32;
    const obsMount = vantage.obsMountainosityNorm || 0.0;
    ctx.fillStyle = fgGrad;
    ctx.beginPath();
    ctx.moveTo(0, cssH);
    for (let k = 0; k <= fgSteps; k++) {
      const dAz = -50.0 + (100.0 * k) / fgSteps;
      const wAz = ((viewAzDeg + dAz) % 360.0 + 360.0) % 360.0;
      const undul =
        vantageRidgeNoise(wAz * 1.5, (view.selectedTileIdx || 0) + 101) *
        (0.55 + 2.4 * obsMount);
      const fgAlt = -6.0 + undul;
      ctx.lineTo(dAzToX(dAz), altToY(fgAlt));
    }
    ctx.lineTo(cssW, cssH);
    ctx.closePath();
    ctx.fill();

    // Foreground rock boulders / crag texture if mountainous, or foliage tufts if vegetated
    ctx.strokeStyle = "rgba(15, 23, 42, 0.38)";
    ctx.lineWidth = 0.85;
    ctx.beginPath();
    for (let k = 0; k <= fgSteps; k++) {
      const dAz = -50.0 + (100.0 * k) / fgSteps;
      const wAz = ((viewAzDeg + dAz) % 360.0 + 360.0) % 360.0;
      const undul =
        vantageRidgeNoise(wAz * 1.5, (view.selectedTileIdx || 0) + 101) *
        (0.55 + 2.4 * obsMount);
      const px = dAzToX(dAz);
      const py = altToY(-6.0 + undul);
      if (k === 0) ctx.moveTo(px, py);
      else ctx.lineTo(px, py);
    }
    ctx.stroke();

    // Foreground river stream if observer tile has a river
    if (vantage.obsIsRiver) {
      const rivAzOff =
        Math.sin(((viewAzDeg * Math.PI) / 180.0) * 2.0 + (view.selectedTileIdx || 0) * 0.3) * 18.0;
      const rxTop = dAzToX(rivAzOff * 0.45);
      const rxBot = dAzToX(rivAzOff);
      const ryTop = altToY(-6.0);
      ctx.fillStyle = `rgba(56, 189, 248, ${(0.68 * Math.max(0.28, baseLight)).toFixed(3)})`;
      ctx.beginPath();
      ctx.moveTo(rxTop - 2.0, ryTop);
      ctx.lineTo(rxTop + 2.0, ryTop);
      ctx.lineTo(rxBot + 9.0, cssH);
      ctx.lineTo(rxBot - 9.0, cssH);
      ctx.closePath();
      ctx.fill();
    }
  }

  // 7. Subtle HUD overlays: +90° zenith / altitude ticks & compass bearings along top
  ctx.strokeStyle = "rgba(148, 163, 184, 0.20)";
  ctx.lineWidth = 0.7;
  ctx.setLineDash([2, 3]);
  ctx.beginPath();
  ctx.moveTo(0, yHor0);
  ctx.lineTo(cssW, yHor0);
  ctx.stroke();
  ctx.setLineDash([]);

  ctx.fillStyle = "rgba(203, 213, 225, 0.62)";
  ctx.font = "8px sans-serif";
  ctx.textAlign = "left";
  ctx.textBaseline = "top";
  ctx.fillText("+90°", 3, 2);
  ctx.textBaseline = "middle";
  for (const deg of [60, 30, 0]) {
    const ty = altToY(deg);
    ctx.strokeStyle = "rgba(148, 163, 184, 0.35)";
    ctx.beginPath();
    ctx.moveTo(0, ty);
    ctx.lineTo(4, ty);
    ctx.stroke();
    ctx.fillText(`${deg}°`, 5, ty);
  }

  // Cardinal / intercardinal compass bearings across the 100° horizontal FOV
  const cardinals = [
    [0, "N"],
    [45, "NE"],
    [90, "E"],
    [135, "SE"],
    [180, "S"],
    [225, "SW"],
    [270, "W"],
    [315, "NW"],
  ];
  ctx.textAlign = "center";
  ctx.textBaseline = "top";
  ctx.font = "bold 8.5px sans-serif";
  for (const [cAz, cLbl] of cardinals) {
    const dAz = wrapDeltaAzDeg(cAz, viewAzDeg);
    if (Math.abs(dAz) <= 46.0) {
      const cxTick = dAzToX(dAz);
      ctx.strokeStyle = "rgba(226, 232, 240, 0.55)";
      ctx.beginPath();
      ctx.moveTo(cxTick, 0);
      ctx.lineTo(cxTick, 4);
      ctx.stroke();
      ctx.fillStyle = cLbl.length === 1 ? "#fde047" : "rgba(226, 232, 240, 0.80)";
      ctx.fillText(cLbl, cxTick, 5);
    }
  }

  // Subtle center crosshair tick at active adjacent neighbor bearing (dAz = 0°)
  ctx.strokeStyle = "rgba(56, 189, 248, 0.65)";
  ctx.lineWidth = 1.1;
  ctx.beginPath();
  ctx.moveTo(cssW * 0.5, cssH - 5);
  ctx.lineTo(cssW * 0.5, cssH);
  ctx.stroke();

  ctx.restore();
}

function ensureTileSkyLoop(view) {
  if (!view || typeof window === "undefined") return;
  if (view.tileSkyRafId) return;
  let lastTs = performance.now();
  const stepSky = (now) => {
    if (activeView !== view || !view.tileSky) {
      view.tileSkyRafId = null;
      return;
    }
    const dt = Math.min(0.1, Math.max(0.0, (now - lastTs) * 0.001));
    lastTs = now;
    if (view.tileSkyAnimated !== false) {
      const daysPerYear = Math.max(1, view.tileSky.daysPerYear || 365);
      view.tileSkyHourFrac = (view.tileSkyHourFrac || 0.0) + dt * 0.22;
      if (view.tileSkyHourFrac >= 1.0) {
        const addDays = Math.floor(view.tileSkyHourFrac);
        view.tileSkyHourFrac -= addDays;
        let nextDay = (view.tileSkyDay || 0) + addDays;
        if (nextDay >= daysPerYear) {
          const addYears = Math.floor(nextDay / daysPerYear);
          nextDay = nextDay % daysPerYear;
          view.tileSkyYear = Math.max(0, (view.tileSkyYear || 0) + addYears);
          const yearInput = document.getElementById("tile-sky-year-input");
          if (yearInput && document.activeElement !== yearInput) {
            yearInput.value = String(view.tileSkyYear);
          }
        }
        view.tileSkyDay = nextDay;
        const daySlider = document.getElementById("tile-sky-day-slider");
        if (daySlider) {
          daySlider.value = String(view.tileSkyDay);
        }
        const dayLabel = document.getElementById("tile-sky-day-label");
        if (dayLabel) {
          dayLabel.textContent = `Day in Year: Day ${view.tileSkyDay} / ${Math.max(1, daysPerYear - 1)}`;
        }
      }
    }
    paintTileSkyCanvas(view);
    paintTileVantageCanvas(view);
    view.tileSkyRafId = window.requestAnimationFrame(stepSky);
  };
  view.tileSkyRafId = window.requestAnimationFrame(stepSky);
}

function paintStateZoomCanvas(view) {
  const canvas = document.getElementById("state-zoom-canvas");
  if (!canvas || !view || !view.stateMapPolygons || view.stateMapPolygons.length === 0) {
    return;
  }
  const rect = canvas.getBoundingClientRect();
  const cssW = Math.max(220, Math.round(rect.width || 270));
  const cssH = Math.max(170, Math.round(rect.height || 195));
  const dpr = window.devicePixelRatio || 1;
  const targetW = Math.round(cssW * dpr);
  const targetH = Math.round(cssH * dpr);
  if (canvas.width !== targetW || canvas.height !== targetH) {
    canvas.width = targetW;
    canvas.height = targetH;
  }

  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  ctx.save();
  ctx.scale(dpr, dpr);

  ctx.fillStyle = "#081224";
  ctx.fillRect(0, 0, cssW, cssH);

  const size = Math.min(cssW, cssH);
  const offsetX = (cssW - size) * 0.5;
  const offsetY = (cssH - size) * 0.5;

  const tracePoly = (pts) => {
    ctx.beginPath();
    for (let i = 0; i < pts.length; i++) {
      const px = offsetX + pts[i][0] * size;
      const py = offsetY + pts[i][1] * size;
      if (i === 0) ctx.moveTo(px, py);
      else ctx.lineTo(px, py);
    }
    ctx.closePath();
  };

  // Pass 1: Context tiles (coastal water, unclaimed land, neighboring states)
  for (const poly of view.stateMapPolygons) {
    if (poly.isInSelectedState || !poly.points || poly.points.length < 3) continue;
    tracePoly(poly.points);
    ctx.fillStyle = rgbFloatToCss(poly.rgb, 40, 60, 50);
    ctx.fill();
    ctx.strokeStyle = "rgba(15, 23, 42, 0.7)";
    ctx.lineWidth = 0.75;
    ctx.stroke();
  }

  // Pass 2: Selected state's tiles
  for (const poly of view.stateMapPolygons) {
    if (!poly.isInSelectedState || !poly.points || poly.points.length < 3) continue;
    tracePoly(poly.points);
    ctx.fillStyle = rgbFloatToCss(poly.rgb, 60, 180, 90);
    ctx.fill();
    ctx.strokeStyle = poly.isRiver ? "rgba(56, 189, 248, 0.9)" : "rgba(226, 232, 240, 0.85)";
    ctx.lineWidth = 1.15;
    ctx.stroke();
  }

  // Pass 3: Selected tile highlight & capital star
  for (const poly of view.stateMapPolygons) {
    if (!poly.points || poly.points.length < 3) continue;
    if (poly.isSelectedTile) {
      tracePoly(poly.points);
      ctx.strokeStyle = "#facc15";
      ctx.lineWidth = 2.6;
      ctx.stroke();
    }
    if (poly.isCapital) {
      let cx = 0;
      let cy = 0;
      for (const pt of poly.points) {
        cx += pt[0];
        cy += pt[1];
      }
      cx = offsetX + (cx / poly.points.length) * size;
      cy = offsetY + (cy / poly.points.length) * size;
      ctx.fillStyle = "#fef08a";
      ctx.strokeStyle = "#0f172a";
      ctx.lineWidth = 1.2;
      ctx.beginPath();
      ctx.arc(cx, cy, 4.0, 0, Math.PI * 2);
      ctx.fill();
      ctx.stroke();
    }
  }

  ctx.restore();

  canvas.onclick = (ev) => {
    if (!view || !view.stateMapPolygons) return;
    const r = canvas.getBoundingClientRect();
    const clickX = ev.clientX - r.left;
    const clickY = ev.clientY - r.top;
    const w = Math.max(1, r.width);
    const h = Math.max(1, r.height);
    const s = Math.min(w, h);
    const ox = (w - s) * 0.5;
    const oy = (h - s) * 0.5;
    const u = (clickX - ox) / s;
    const v = (clickY - oy) / s;

    let hit = null;
    for (const poly of view.stateMapPolygons) {
      if (poly.isInSelectedState && pointInPolygon2D(u, v, poly.points)) {
        hit = poly;
        break;
      }
    }
    if (!hit) {
      for (let i = view.stateMapPolygons.length - 1; i >= 0; i--) {
        const poly = view.stateMapPolygons[i];
        if (pointInPolygon2D(u, v, poly.points)) {
          hit = poly;
          break;
        }
      }
    }
    if (!hit) {
      let bestDist2 = 0.004;
      for (const poly of view.stateMapPolygons) {
        if (!poly.points || poly.points.length === 0) continue;
        let cx = 0;
        let cy = 0;
        for (const pt of poly.points) {
          cx += pt[0];
          cy += pt[1];
        }
        cx /= poly.points.length;
        cy /= poly.points.length;
        const d2 = (u - cx) * (u - cx) + (v - cy) * (v - cy);
        if (d2 < bestDist2) {
          bestDist2 = d2;
          hit = poly;
        }
      }
    }
    if (hit) {
      view.selectedTileIdx = hit.displayIdx;
      if (hit.stateId >= 0) {
        view.selectedStateId = hit.stateId;
      }
      refreshActiveInspectorView(view);
    }
  };
}

function buildHistoryInspectorBundle(view) {
  const s = view.session;
  let stateView = emptyStateView();
  let tileView = emptyTileView();
  let languageDetail = emptyLanguageDetail();
  let nationDetail = emptyNationDetail();

  const canInspectTile = typeof s.inspect_tile_structured_json === "function";
  const canInspectPolities =
    typeof s.inspect_state_json === "function" &&
    (typeof s.stage_index !== "function" || s.stage_index() === 2);

  if (!canInspectTile) {
    view.stateMapPolygons = [];
    view.tileSky = null;
    view.tileVantage = null;
    return { stateView, tileView, languageDetail, nationDetail };
  }

  if (view.selectedTileIdx >= 0) {
    try {
      const parsedTile = JSON.parse(s.inspect_tile_structured_json(view.selectedTileIdx));
      if (parsedTile && parsedTile.hasTile) {
        const sky = parsedTile.sky || null;
        const vantage = parsedTile.vantage || null;
        view.tileSky = sky;
        view.tileVantage = vantage;
        view.tileStratigraphy = parsedTile.stratigraphy || null;
        if (view.tileSkyYear === undefined) view.tileSkyYear = 0;
        if (view.tileSkyDay === undefined) view.tileSkyDay = 0;
        if (view.tileSkyHourFrac === undefined) view.tileSkyHourFrac = 0.35;
        if (view.tileSkyAnimated === undefined) view.tileSkyAnimated = true;

        const nbs = vantage && Array.isArray(vantage.neighbors) ? vantage.neighbors : [];
        if (view.tileVantageTileIdx !== view.selectedTileIdx) {
          view.tileVantageTileIdx = view.selectedTileIdx;
          if (nbs.length > 0 && typeof view.tileVantagePreferredAzDeg === "number") {
            let bestIdx = 0;
            let bestDiff = 999.0;
            for (let i = 0; i < nbs.length; i++) {
              const diff = Math.abs(wrapDeltaAzDeg(nbs[i].azDeg || 0.0, view.tileVantagePreferredAzDeg));
              if (diff < bestDiff) {
                bestDiff = diff;
                bestIdx = i;
              }
            }
            view.tileVantageNeighborIdx = bestIdx;
          } else {
            view.tileVantageNeighborIdx = 0;
          }
        } else if (nbs.length > 0) {
          view.tileVantageNeighborIdx =
            ((view.tileVantageNeighborIdx | 0) % nbs.length + nbs.length) % nbs.length;
        } else {
          view.tileVantageNeighborIdx = 0;
        }

        const daysPerYear = sky && sky.daysPerYear ? sky.daysPerYear : 365;
        if (view.tileSkyDay >= daysPerYear) {
          view.tileSkyDay = view.tileSkyDay % Math.max(1, daysPerYear);
        }
        const latVal = sky && typeof sky.latDeg === "number" ? sky.latDeg : 0.0;
        const lonVal = sky && typeof sky.lonDeg === "number" ? sky.lonDeg : 0.0;
        const latStr = `${Math.abs(latVal).toFixed(1)}°${latVal >= 0 ? "N" : "S"}`;
        const lonStr = `${Math.abs(lonVal).toFixed(1)}°${lonVal >= 0 ? "E" : "W"}`;
        const vSum = vantageSummaryStrings(view);

        tileView = {
          hasTile: true,
          displayIdx: parsedTile.displayIdx ?? view.selectedTileIdx,
          stateId: parsedTile.stateId ?? -1,
          title: parsedTile.title || "",
          locationSummary: parsedTile.locationSummary || "",
          populationSummary: parsedTile.populationSummary || "",
          technology: parsedTile.technology || "",
          nationalities: parsedTile.nationalities || [],
          languages: parsedTile.languages || [],
          flora: parsedTile.flora || [],
          fauna: parsedTile.fauna || [],
          skyYear: view.tileSkyYear,
          skyDay: view.tileSkyDay,
          skyDaysPerYear: daysPerYear,
          skyLatLonSummary: `${latStr}, ${lonStr}`,
          skyAnimated: view.tileSkyAnimated !== false,
          vantageBearingSummary: vSum.bearingSummary,
          vantageNeighborSummary: vSum.neighborSummary,
        };
        if (parsedTile.stateId !== undefined && parsedTile.stateId >= 0) {
          view.selectedStateId = parsedTile.stateId;
        }
      } else {
        view.tileSky = null;
        view.tileVantage = null;
      }
    } catch (_) {
      view.tileSky = null;
      view.tileVantage = null;
    }
  } else {
    view.tileSky = null;
    view.tileVantage = null;
  }

  if (canInspectPolities && view.selectedStateId !== undefined && view.selectedStateId >= 0) {
    try {
      let parsedState = JSON.parse(
        s.inspect_state_json(view.selectedStateId, view.selectedTileIdx)
      );
      if ((!parsedState || !parsedState.hasState) && view.selectedTileIdx >= 0) {
        parsedState = JSON.parse(s.inspect_state_for_tile_json(view.selectedTileIdx));
      }
      if (parsedState && parsedState.hasState) {
        view.selectedStateId = parsedState.stateId;
        view.stateMapPolygons = parsedState.mapPolygons || [];
        stateView = {
          hasState: true,
          stateId: parsedState.stateId,
          stateName: parsedState.stateName || "",
          colorCss: parsedState.colorCss || "rgb(38, 242, 89)",
          continents: parsedState.continents || "",
          technology: parsedState.technology || "",
          populationText: parsedState.populationText || "",
          capacityText: parsedState.capacityText || "",
          tileCount: parsedState.tileCount || 0,
          asabiyaText: parsedState.asabiyaText || "",
          nationalities: parsedState.nationalities || [],
          languages: parsedState.languages || [],
          exports: parsedState.exports || [],
          imports: parsedState.imports || [],
          tradePartners: parsedState.tradePartners || [],
          conflicts: parsedState.conflicts || [],
          waterRoutes: parsedState.waterRoutes || [],
        };
      } else {
        view.stateMapPolygons = [];
      }
    } catch (_) {
      view.stateMapPolygons = [];
    }
  } else {
    view.stateMapPolygons = [];
  }

  if (canInspectPolities && view.selectedLangId !== undefined && view.selectedLangId >= 0) {
    try {
      const parsedLang = JSON.parse(s.inspect_language_json(view.selectedLangId));
      if (parsedLang && parsedLang.hasLanguage) {
        languageDetail = {
          hasLanguage: true,
          langId: parsedLang.langId,
          name: parsedLang.name || "",
          totalSpeakersText: parsedLang.totalSpeakersText || "",
          phonemeSummary: parsedLang.phonemeSummary || "",
          speakersByLocation: parsedLang.speakersByLocation || [],
          speakersByNation: parsedLang.speakersByNation || [],
          lexicon: parsedLang.lexicon || [],
        };
      }
    } catch (_) {}
  }

  if (canInspectPolities && view.selectedNationId !== undefined && view.selectedNationId >= 0) {
    try {
      const parsedNat = JSON.parse(s.inspect_nation_json(view.selectedNationId));
      if (parsedNat && parsedNat.hasNation) {
        nationDetail = {
          hasNation: true,
          natId: parsedNat.natId,
          name: parsedNat.name || "",
          totalPopText: parsedNat.totalPopText || "",
          originSummary: parsedNat.originSummary || "",
          heritageSummary: parsedNat.heritageSummary || "",
          currentLocations: parsedNat.currentLocations || [],
          spokenLanguages: parsedNat.spokenLanguages || [],
        };
      }
    } catch (_) {}
  }

  if (stateView.hasState && typeof window !== "undefined") {
    window.requestAnimationFrame(() => paintStateZoomCanvas(view));
    setTimeout(() => paintStateZoomCanvas(view), 45);
  }
  if (tileView.hasTile && view.tileSky && typeof window !== "undefined") {
    window.requestAnimationFrame(() => {
      paintTileSkyCanvas(view);
      paintTileVantageCanvas(view);
      ensureTileSkyLoop(view);
    });
    setTimeout(() => {
      paintTileSkyCanvas(view);
      paintTileVantageCanvas(view);
      ensureTileSkyLoop(view);
    }, 45);
  }
  if (tileView.hasTile && typeof window !== "undefined") {
    window.requestAnimationFrame(() => paintStratigraphySideCanvas(view));
    setTimeout(() => paintStratigraphySideCanvas(view), 45);
  }

  return { stateView, tileView, languageDetail, nationDetail };
}

export const setTileSkyYear = (year) => () => {
  if (!activeView || !activeView.session) return;
  activeView.tileSkyYear = Math.max(0, year | 0);
  refreshActiveInspectorView(activeView);
};

export const setTileSkyDay = (day) => () => {
  if (!activeView || !activeView.session) return;
  const maxDay =
    activeView.tileSky && activeView.tileSky.daysPerYear
      ? Math.max(0, activeView.tileSky.daysPerYear - 1)
      : 364;
  activeView.tileSkyDay = Math.max(0, Math.min(maxDay, day | 0));
  refreshActiveInspectorView(activeView);
};

export const toggleTileSkyAnimation = () => {
  if (!activeView || !activeView.session) return;
  activeView.tileSkyAnimated = activeView.tileSkyAnimated === false;
  refreshActiveInspectorView(activeView);
};

export const stepTileVantageDirection = (delta) => () => {
  if (!activeView || !activeView.tileVantage || !Array.isArray(activeView.tileVantage.neighbors)) {
    return;
  }
  const nbs = activeView.tileVantage.neighbors;
  if (nbs.length === 0) return;
  const curIdx = activeView.tileVantageNeighborIdx | 0;
  const nextIdx = ((curIdx + (delta | 0)) % nbs.length + nbs.length) % nbs.length;
  activeView.tileVantageNeighborIdx = nextIdx;
  activeView.tileVantagePreferredAzDeg = nbs[nextIdx].azDeg || 0.0;

  const vSum = vantageSummaryStrings(activeView);
  const dirEl = document.getElementById("tile-vantage-dir-label");
  if (dirEl) {
    dirEl.textContent = vSum.bearingSummary;
  }
  const sumEl = document.getElementById("tile-vantage-summary-label");
  if (sumEl) {
    sumEl.textContent = vSum.neighborSummary;
  }
  paintTileSkyCanvas(activeView);
  paintTileVantageCanvas(activeView);
};

function rockLayerBaseStyle(layer) {
  const name = (layer.name || "").toLowerCase();
  const kind = layer.kind || "I";
  const f = typeof layer.felsity === "number" ? layer.felsity : 0.5;
  switch (name) {
    case "basalt":
      if (f <= 0.35) {
        return { fill: "#163537", stroke: "#2dd4bf", pattern: "pebbles" };
      }
      return { fill: "#28303d", stroke: "#475569", pattern: "mafic" };
    case "gabbro":
      return { fill: "#1e2530", stroke: "#334155", pattern: "plutonic_dark" };
    case "andesite":
      return { fill: "#4b4e63", stroke: "#64748b", pattern: "porphyritic" };
    case "diorite":
      return { fill: "#43495c", stroke: "#64748b", pattern: "plutonic_mid" };
    case "dacite":
      return { fill: "#6c677d", stroke: "#94a3b8", pattern: "porphyritic" };
    case "granodiorite":
      return { fill: "#8c7785", stroke: "#94a3b8", pattern: "granitic" };
    case "rhyolite":
      return { fill: "#b57c78", stroke: "#fda4af", pattern: "flow_banded" };
    case "granite":
      return { fill: "#c48882", stroke: "#fecdd3", pattern: "granitic" };
    case "claystone":
      return { fill: "#575a4e", stroke: "#78716c", pattern: "shale" };
    case "siltstone":
      return { fill: "#706552", stroke: "#a8a29e", pattern: "silt" };
    case "sandstone":
      return { fill: "#bf8f46", stroke: "#fcd34d", pattern: "sand" };
    case "conglomerate":
      return { fill: "#9e6738", stroke: "#fdba74", pattern: "pebbles" };
    case "limestone":
      return { fill: "#b8b6a6", stroke: "#e7e5e4", pattern: "carbonate" };
    case "chalk":
      return { fill: "#d9d7c9", stroke: "#f5f5f4", pattern: "carbonate" };
    case "pelite":
      return { fill: "#4c5464", stroke: "#94a3b8", pattern: "foliated" };
    case "psammite":
      return { fill: "#8c7252", stroke: "#d6d3d1", pattern: "foliated" };
    case "psephite":
      return { fill: "#7c583c", stroke: "#d6d3d1", pattern: "pebbles" };
    case "greenschist":
      return { fill: "#365944", stroke: "#6ee7b7", pattern: "schist" };
    case "blueschist":
      return { fill: "#294670", stroke: "#7dd3fc", pattern: "schist" };
    case "amphibolite":
      return { fill: "#23393d", stroke: "#5eead4", pattern: "foliated" };
    case "eclogite":
      return { fill: "#214736", stroke: "#6ee7b7", pattern: "eclogite" };
    case "hornfels":
      return { fill: "#4c3844", stroke: "#c084fc", pattern: "baked" };
    case "granulite":
      return { fill: "#5c4033", stroke: "#fdba74", pattern: "granitic" };
    case "marble":
      return { fill: "#e5e3da", stroke: "#f8fafc", pattern: "marble" };
    default: {
      if (kind === "S") return { fill: "#8c7853", stroke: "#cbd5e1", pattern: "silt" };
      if (kind === "M") return { fill: "#475569", stroke: "#94a3b8", pattern: "foliated" };
      const r = Math.round(45 + 150 * f);
      const g = Math.round(50 + 85 * f);
      const b = Math.round(65 + 65 * f);
      return { fill: `rgb(${r},${g},${b})`, stroke: "#64748b", pattern: "granitic" };
    }
  }
}

function detRand(seed, a, b) {
  const x = Math.sin(seed * 12.9898 + a * 78.233 + b * 37.719) * 43758.5453;
  return x - Math.floor(x);
}

function paintStratigraphySideCanvas(view) {
  const canvas = document.getElementById("stratigraphy-side-canvas");
  if (!canvas || !view) return;

  const zoomBtn = document.getElementById("stratigraphy-zoom-btn");
  if (zoomBtn) {
    zoomBtn.textContent = view.stratigraphyTopZoom ? "🔍 Full Crust" : "🔍 Top 8 km Zoom";
  }

  const rect = canvas.getBoundingClientRect();
  const cssW = Math.max(240, Math.round(rect.width || 280));
  const cssH = Math.max(185, Math.round(rect.height || 215));
  const dpr = window.devicePixelRatio || 1;
  const targetW = Math.round(cssW * dpr);
  const targetH = Math.round(cssH * dpr);
  if (canvas.width !== targetW || canvas.height !== targetH) {
    canvas.width = targetW;
    canvas.height = targetH;
  }

  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  ctx.save();
  ctx.scale(dpr, dpr);

  ctx.fillStyle = "#080f1d";
  ctx.fillRect(0, 0, cssW, cssH);

  const strat = view.tileStratigraphy;
  if (!strat || !strat.hasColumn || !strat.layers || strat.layers.length === 0) {
    ctx.fillStyle = "#64748b";
    ctx.font = "11px sans-serif";
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(
      "Click any tile on the globe to inspect its 2D rock strata & mineral veins",
      cssW * 0.5,
      cssH * 0.5
    );
    ctx.restore();
    return;
  }

  const elevKm = typeof strat.elevationKm === "number" ? strat.elevationKm : 0.0;
  const mohoKm = typeof strat.bottomKm === "number" ? strat.bottomKm : elevKm - 10.0;
  const mountNorm = Math.max(0.0, Math.min(1.0, strat.mountainosityNorm || 0.0));
  const impactScar = Math.max(0.0, Math.min(1.0, strat.impactScar || 0.0));
  const tileSeed = (strat.tileIdx || 0) + 17;

  // Vertical coordinate mapping (km elevation -> canvas Y)
  const topSkyMarginKm = elevKm >= 0.0 ? Math.max(1.8, elevKm * 0.28) : 1.5;
  const viewTopKm = Math.max(elevKm + topSkyMarginKm, 1.5);
  const fullBottomKm = mohoKm - Math.max(2.2, (elevKm - mohoKm) * 0.1);
  const zoomBottomKm = Math.max(fullBottomKm, elevKm - 8.5);
  const viewBottomKm = view.stratigraphyTopZoom ? zoomBottomKm : fullBottomKm;
  const spanKm = Math.max(1.0, viewTopKm - viewBottomKm);

  const padTop = 10;
  const padBot = 10;
  const plotH = cssH - padTop - padBot;
  const colLeft = 42;
  const colRight = cssW - 116;
  const colW = Math.max(80, colRight - colLeft);

  const kmToY = (km) => padTop + ((viewTopKm - km) / spanKm) * plotH;
  const yToKm = (y) => viewTopKm - ((y - padTop) / plotH) * spanKm;

  // Tectonic folding / crater bowl wave offset (in pixels) at horizontal fraction u in [0, 1]
  const foldOffsetPx = (u, depthKm) => {
    const waveFreq = 2.0 + Math.round(mountNorm * 2.0);
    const phase = (tileSeed % 11) * 0.55;
    const foldAmpPx = mountNorm * 9.5 * Math.sin(u * Math.PI * waveFreq + phase);
    const craterBowlPx =
      impactScar > 0.35
        ? Math.min(10.0, impactScar * 7.5) *
          Math.sin(u * Math.PI) *
          Math.exp(-Math.max(0.0, depthKm) * 0.12)
        : 0.0;
    return foldAmpPx + craterBowlPx;
  };

  // 1. Sky & Ocean Background inside column
  ctx.save();
  ctx.beginPath();
  ctx.rect(colLeft, padTop, colW, plotH);
  ctx.clip();

  const ySea = kmToY(0.0);
  const ySurfBase = kmToY(elevKm);
  const skyGrad = ctx.createLinearGradient(0, padTop, 0, Math.max(padTop + 20, Math.min(ySea, ySurfBase)));
  skyGrad.addColorStop(0.0, "#0b192c");
  skyGrad.addColorStop(1.0, "#1e3a5f");
  ctx.fillStyle = skyGrad;
  ctx.fillRect(colLeft, padTop, colW, plotH);

  // If submarine (elevKm < 0), fill ocean water above the seafloor
  if (elevKm < 0.0) {
    const oceanGrad = ctx.createLinearGradient(0, Math.max(padTop, ySea), 0, Math.min(cssH, ySurfBase + 12));
    oceanGrad.addColorStop(0.0, "rgba(14, 116, 144, 0.78)");
    oceanGrad.addColorStop(0.6, "rgba(12, 74, 110, 0.88)");
    oceanGrad.addColorStop(1.0, "rgba(8, 47, 73, 0.95)");
    ctx.fillStyle = oceanGrad;
    ctx.beginPath();
    ctx.moveTo(colLeft, Math.max(padTop, ySea));
    ctx.lineTo(colRight, Math.max(padTop, ySea));
    const steps = 28;
    for (let s = steps; s >= 0; s--) {
      const u = s / steps;
      const x = colLeft + u * colW;
      const y = ySurfBase + foldOffsetPx(u, 0.0);
      ctx.lineTo(x, y);
    }
    ctx.closePath();
    ctx.fill();

    // Active seafloor Black Smoker hydrothermal plumes if top layer has VMS sulfidity
    const topL = strat.layers[0];
    if (topL && topL.kind === "I" && (topL.sulfidity || 0) >= 0.12) {
      const ventPositions = [0.28, 0.68];
      for (let vi = 0; vi < ventPositions.length; vi++) {
        const vu = ventPositions[vi];
        const vx = colLeft + vu * colW;
        const vy = ySurfBase + foldOffsetPx(vu, 0.0);
        // Sulfide chimney mound
        ctx.fillStyle = "#ca8a04";
        ctx.beginPath();
        ctx.moveTo(vx - 4, vy + 1);
        ctx.lineTo(vx - 1.5, vy - 6);
        ctx.lineTo(vx + 1.5, vy - 6);
        ctx.lineTo(vx + 4, vy + 1);
        ctx.closePath();
        ctx.fill();
        // Black smoker plume rising into ocean water
        const plumeGrad = ctx.createRadialGradient(vx, vy - 13, 1, vx, vy - 16, 14);
        plumeGrad.addColorStop(0.0, "rgba(30, 41, 59, 0.92)");
        plumeGrad.addColorStop(0.5, "rgba(71, 85, 105, 0.55)");
        plumeGrad.addColorStop(1.0, "rgba(15, 23, 42, 0.0)");
        ctx.fillStyle = plumeGrad;
        ctx.beginPath();
        ctx.arc(vx, vy - 15, 13, 0, Math.PI * 2);
        ctx.fill();
      }
    }
  }

  // 2. Upper Mantle below Moho
  const yMohoBase = kmToY(mohoKm);
  if (yMohoBase < cssH) {
    const mantleGrad = ctx.createLinearGradient(0, Math.max(padTop, yMohoBase - 5), 0, cssH);
    mantleGrad.addColorStop(0.0, "#7c2d12");
    mantleGrad.addColorStop(0.5, "#9a3412");
    mantleGrad.addColorStop(1.0, "#431407");
    ctx.fillStyle = mantleGrad;
    ctx.fillRect(colLeft, Math.max(padTop, yMohoBase - 12), colW, cssH - Math.max(padTop, yMohoBase - 12));
  }

  // 3. Draw each RockLayer polygon from top (index 0) to bottom
  const steps = 28;
  const hitBands = [];

  for (let i = 0; i < strat.layers.length; i++) {
    const layer = strat.layers[i];
    const yTopBase = kmToY(layer.topKm);
    const yBotBase = kmToY(layer.bottomKm);
    if (yBotBase < padTop - 10 || yTopBase > cssH + 10) {
      continue;
    }

    const style = rockLayerBaseStyle(layer);
    const topPts = [];
    const botPts = [];
    for (let s = 0; s <= steps; s++) {
      const u = s / steps;
      const x = colLeft + u * colW;
      topPts.push([x, yTopBase + foldOffsetPx(u, layer.depthTopKm)]);
      botPts.push([x, yBotBase + foldOffsetPx(u, layer.depthBottomKm)]);
    }

    const traceLayerPoly = () => {
      ctx.beginPath();
      for (let s = 0; s <= steps; s++) {
        if (s === 0) ctx.moveTo(topPts[s][0], topPts[s][1]);
        else ctx.lineTo(topPts[s][0], topPts[s][1]);
      }
      for (let s = steps; s >= 0; s--) {
        ctx.lineTo(botPts[s][0], botPts[s][1]);
      }
      ctx.closePath();
    };

    traceLayerPoly();
    ctx.fillStyle = style.fill;
    ctx.fill();

    // Clip interior textures & mineral veins to this rock layer
    ctx.save();
    traceLayerPoly();
    ctx.clip();

    const midY = 0.5 * (yTopBase + yBotBase);
    const bandH = Math.max(2.0, yBotBase - yTopBase);
    const lSeed = tileSeed * 31 + i * 13;

    // Lithological background texture
    if (style.pattern === "shale" || style.pattern === "silt") {
      ctx.strokeStyle = "rgba(15, 23, 42, 0.28)";
      ctx.lineWidth = 0.8;
      const numLines = Math.min(8, Math.max(1, Math.floor(bandH / 5)));
      for (let ln = 1; ln <= numLines; ln++) {
        const frac = ln / (numLines + 1);
        ctx.beginPath();
        for (let s = 0; s <= steps; s += 2) {
          const x = topPts[s][0];
          const y = topPts[s][1] * (1 - frac) + botPts[s][1] * frac;
          if (s === 0) ctx.moveTo(x, y);
          else ctx.lineTo(x, y);
        }
        ctx.stroke();
      }
    } else if (style.pattern === "carbonate" || style.pattern === "marble") {
      ctx.strokeStyle = "rgba(30, 41, 59, 0.25)";
      ctx.lineWidth = 0.8;
      const rowStep = Math.max(5, Math.min(10, bandH * 0.45));
      for (let ry = yTopBase - 4; ry <= yBotBase + 6; ry += rowStep) {
        ctx.beginPath();
        ctx.moveTo(colLeft, ry);
        ctx.lineTo(colRight, ry);
        ctx.stroke();
      }
    } else if (style.pattern === "foliated" || style.pattern === "schist") {
      ctx.strokeStyle = "rgba(226, 232, 240, 0.18)";
      ctx.lineWidth = 0.9;
      const numWaves = Math.min(6, Math.max(1, Math.floor(bandH / 6)));
      for (let w = 1; w <= numWaves; w++) {
        const frac = w / (numWaves + 1);
        ctx.beginPath();
        for (let s = 0; s <= steps; s++) {
          const x = topPts[s][0];
          const y =
            topPts[s][1] * (1 - frac) +
            botPts[s][1] * frac +
            Math.sin(s * 0.9 + w) * 1.3;
          if (s === 0) ctx.moveTo(x, y);
          else ctx.lineTo(x, y);
        }
        ctx.stroke();
      }
    } else if (style.pattern === "granitic" || style.pattern === "plutonic_mid" || style.pattern === "porphyritic") {
      const numSpecks = Math.min(28, Math.max(6, Math.floor((colW * bandH) / 95)));
      for (let sp = 0; sp < numSpecks; sp++) {
        const u = detRand(lSeed, sp, 1);
        const v = detRand(lSeed, sp, 2);
        const idx = Math.min(steps, Math.floor(u * steps));
        const x = colLeft + u * colW;
        const y = topPts[idx][1] * (1 - v) + botPts[idx][1] * v;
        ctx.fillStyle = sp % 2 === 0 ? "rgba(255,255,255,0.22)" : "rgba(15,23,42,0.28)";
        ctx.fillRect(x, y, 1.6, 1.6);
      }
    } else if (style.pattern === "eclogite") {
      // Red garnet porphyroblasts in green omphacite matrix
      const numGarnets = Math.min(18, Math.max(4, Math.floor(bandH * 0.7)));
      for (let g = 0; g < numGarnets; g++) {
        const u = detRand(lSeed, g, 3);
        const v = 0.15 + 0.7 * detRand(lSeed, g, 4);
        const idx = Math.min(steps, Math.floor(u * steps));
        const x = colLeft + u * colW;
        const y = topPts[idx][1] * (1 - v) + botPts[idx][1] * v;
        ctx.fillStyle = "#dc2626";
        ctx.beginPath();
        ctx.arc(x, y, 1.8, 0, Math.PI * 2);
        ctx.fill();
      }
    } else if (style.pattern === "pebbles" || style.pattern === "sand") {
      const numGrains = Math.min(26, Math.max(6, Math.floor((colW * bandH) / 90)));
      for (let g = 0; g < numGrains; g++) {
        const u = detRand(lSeed, g, 5);
        const v = 0.1 + 0.8 * detRand(lSeed, g, 6);
        const idx = Math.min(steps, Math.floor(u * steps));
        const x = colLeft + u * colW;
        const y = topPts[idx][1] * (1 - v) + botPts[idx][1] * v;
        ctx.fillStyle = g % 2 === 0 ? "rgba(254, 243, 199, 0.35)" : "rgba(69, 26, 3, 0.35)";
        ctx.beginPath();
        ctx.arc(x, y, style.pattern === "pebbles" ? 2.1 : 1.1, 0, Math.PI * 2);
        ctx.fill();
      }
    }

    // --- EMBEDDED MINERAL DEPOSITS & VEINS ---
    const bog = layer.bogginess || 0.0;
    const calc = layer.calcity || 0.0;
    const qtz = layer.quartzity || 0.0;
    const sulf = layer.sulfidity || 0.0;
    const sal = layer.salinity || 0.0;
    const lname = (layer.name || "").toLowerCase();

    // A. Banded Iron Formation (BIF) alternating red chert & dark magnetite bands
    if (layer.kind === "S" && sulf >= 0.08 && qtz >= 0.08) {
      const bifBands = Math.min(7, Math.max(3, Math.floor(bandH / 3.5)));
      for (let b = 0; b < bifBands; b++) {
        const frac = (b + 0.5) / bifBands;
        ctx.strokeStyle = b % 2 === 0 ? "rgba(185, 28, 28, 0.82)" : "rgba(15, 23, 42, 0.88)";
        ctx.lineWidth = Math.max(1.2, (bandH / bifBands) * 0.45);
        ctx.beginPath();
        for (let s = 0; s <= steps; s++) {
          const x = topPts[s][0];
          const y = topPts[s][1] * (1 - frac) + botPts[s][1] * frac;
          if (s === 0) ctx.moveTo(x, y);
          else ctx.lineTo(x, y);
        }
        ctx.stroke();
      }
    }

    // B. Evaporite Salt / Gypsum / Potash Beds (`salinity >= 0.08`)
    if (sal >= 0.08) {
      const saltFrac = 0.32;
      ctx.strokeStyle = sal >= 0.45 ? "rgba(253, 164, 175, 0.9)" : "rgba(248, 250, 252, 0.88)";
      ctx.lineWidth = Math.max(2.0, Math.min(6.0, bandH * 0.28 * Math.min(1.0, sal + 0.3)));
      ctx.beginPath();
      for (let s = 0; s <= steps; s++) {
        const x = topPts[s][0];
        const y = topPts[s][1] * (1 - saltFrac) + botPts[s][1] * saltFrac;
        if (s === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.stroke();
    }

    // C. Coal & Graphite Seams (`bogginess >= 0.05`) + Eclogite Diamonds
    if (bog >= 0.05) {
      const isGraphite =
        layer.kind === "M" &&
        ["greenschist", "amphibolite", "granulite", "hornfels", "eclogite"].includes(lname);
      const isLignite = layer.kind === "S" && layer.depthTopKm < 0.8 && bog < 0.2;
      ctx.strokeStyle = isGraphite
        ? "#94a3b8"
        : isLignite
          ? "#3f271d"
          : "#090d16";
      const seamWidth = Math.max(2.2, Math.min(7.5, bandH * 0.32 * Math.min(1.0, bog * 1.4)));
      ctx.lineWidth = seamWidth;
      const seamFrac = 0.52;
      ctx.beginPath();
      for (let s = 0; s <= steps; s++) {
        const x = topPts[s][0];
        const y = topPts[s][1] * (1 - seamFrac) + botPts[s][1] * seamFrac;
        if (s === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.stroke();

      // Glossy highlight along coal/anthracite/graphite seam
      ctx.strokeStyle = isGraphite ? "rgba(226, 232, 240, 0.7)" : "rgba(148, 163, 184, 0.38)";
      ctx.lineWidth = 0.8;
      ctx.stroke();

      // Ultra-high-pressure Diamonds in organic-rich Eclogite
      if (lname === "eclogite" && bog >= 0.05) {
        for (let d = 0; d < 4; d++) {
          const u = 0.18 + d * 0.21;
          const idx = Math.min(steps, Math.floor(u * steps));
          const dx = colLeft + u * colW;
          const dy = topPts[idx][1] * 0.48 + botPts[idx][1] * 0.52;
          ctx.fillStyle = "#67e8f9";
          ctx.strokeStyle = "#ffffff";
          ctx.lineWidth = 0.9;
          ctx.beginPath();
          ctx.moveTo(dx, dy - 3.2);
          ctx.lineTo(dx + 2.8, dy);
          ctx.lineTo(dx, dy + 3.2);
          ctx.lineTo(dx - 2.8, dy);
          ctx.closePath();
          ctx.fill();
          ctx.stroke();
        }
      }
    }

    // D. Karst Caverns in shallow high-calcity Limestone / Marble
    if (calc >= 0.45 && (lname === "limestone" || lname === "chalk" || lname === "marble") && layer.depthTopKm < 2.5) {
      for (let c = 0; c < 3; c++) {
        const u = 0.22 + c * 0.27 + (detRand(lSeed, c, 9) - 0.5) * 0.08;
        const idx = Math.min(steps, Math.floor(u * steps));
        const cx = colLeft + u * colW;
        const cy = topPts[idx][1] * 0.45 + botPts[idx][1] * 0.55;
        ctx.fillStyle = "#090d16";
        ctx.strokeStyle = "rgba(148, 163, 184, 0.6)";
        ctx.lineWidth = 0.9;
        ctx.beginPath();
        ctx.ellipse(cx, cy, 6.5, Math.min(4.2, Math.max(2.0, bandH * 0.26)), 0, 0, Math.PI * 2);
        ctx.fill();
        ctx.stroke();
      }
    }

    // Extract continuous mineral grades (`0.0..1.0`) from structured `layer.occurrences`:
    const minGrades = {};
    for (const occ of layer.occurrences || []) {
      for (const pair of occ.minerals || []) {
        if (Array.isArray(pair) && pair.length >= 2) {
          const mk = pair[0];
          const mg = typeof pair[1] === "number" ? pair[1] : 0.0;
          if (mg > (minGrades[mk] || 0.0)) minGrades[mk] = mg;
        }
      }
    }
    const goldGrade = minGrades.gold || 0.0;
    const tinGrade = Math.max(minGrades.tin || 0.0, minGrades.tungsten || 0.0);

    // E. Hydrothermal Quartz / Gold / Tin-Tungsten Veins (`quartzity >= 0.08`)
    if (qtz >= 0.08 && !(layer.kind === "S" && sulf >= 0.12) && layer.kind !== "S") {
      const veinCount = Math.min(6, Math.max(1, Math.round(qtz * 6.5)));
      const metalGrade = Math.max(goldGrade, tinGrade);
      const metalVeinLimit = metalGrade >= 0.06 ? Math.max(1, Math.ceil(veinCount * Math.min(1.0, metalGrade * 1.4))) : 0;
      for (let v = 0; v < veinCount; v++) {
        const u0 = 0.1 + 0.8 * detRand(lSeed, v, 11);
        const u1 = Math.max(0.04, Math.min(0.96, u0 + (detRand(lSeed, v, 12) - 0.5) * 0.22));
        const uMid = 0.5 * (u0 + u1) + (detRand(lSeed, v, 13) - 0.5) * 0.08;
        const i0 = Math.min(steps, Math.floor(u0 * steps));
        const iMid = Math.min(steps, Math.floor(Math.max(0, Math.min(1, uMid)) * steps));
        const i1 = Math.min(steps, Math.floor(u1 * steps));

        const x0 = colLeft + u0 * colW;
        const y0 = botPts[i0][1] + 1;
        const xMid = colLeft + uMid * colW;
        const yMid = 0.5 * (topPts[iMid][1] + botPts[iMid][1]);
        const x1 = colLeft + u1 * colW;
        const y1 = topPts[i1][1] - 1;

        // Milky white quartz vein
        ctx.strokeStyle = "rgba(248, 250, 252, 0.88)";
        ctx.lineWidth = qtz >= 0.35 ? 2.1 : 1.3;
        ctx.beginPath();
        ctx.moveTo(x0, y0);
        ctx.lineTo(xMid, yMid);
        ctx.lineTo(x1, y1);
        ctx.stroke();

        // Branching offshoot veinlet
        const xb = xMid + (detRand(lSeed, v, 14) > 0.5 ? 9 : -9);
        const yb = yMid - 0.25 * bandH;
        ctx.lineWidth = 0.9;
        ctx.beginPath();
        ctx.moveTo(xMid, yMid);
        ctx.lineTo(xb, yb);
        ctx.stroke();

        // Gold / Cassiterite flecks ONLY when continuous gold/tin grade is present (`>= 0.06`)
        if (v < metalVeinLimit) {
          ctx.fillStyle = goldGrade >= tinGrade ? "#facc15" : "#e2e8f0";
          const fleckR = 1.15 + 1.35 * Math.min(1.0, metalGrade);
          ctx.beginPath();
          ctx.arc(xMid, yMid, fleckR, 0, Math.PI * 2);
          ctx.fill();
        }
      }
    }

    // F. Sulfide Ore Veins / Porphyry Stockwork / VMS / Impact Nickel-Copper Sulfides (`sulfidity >= 0.15`)
    if (sulf >= 0.15 && !(layer.kind === "S" && qtz >= 0.18)) {
      const sulfCount = Math.min(6, Math.max(2, Math.round(sulf * 6)));
      for (let sv = 0; sv < sulfCount; sv++) {
        const u0 = 0.12 + 0.76 * detRand(lSeed, sv, 21);
        const u1 = Math.max(0.05, Math.min(0.95, u0 + (detRand(lSeed, sv, 22) - 0.5) * 0.24));
        const i0 = Math.min(steps, Math.floor(u0 * steps));
        const i1 = Math.min(steps, Math.floor(u1 * steps));
        const x0 = colLeft + u0 * colW;
        const y0 = botPts[i0][1];
        const x1 = colLeft + u1 * colW;
        const y1 = topPts[i1][1] * 0.65 + botPts[i1][1] * 0.35;

        ctx.strokeStyle = sv % 2 === 0 ? "#eab308" : "#f97316";
        ctx.lineWidth = sulf >= 0.4 ? 2.0 : 1.3;
        ctx.beginPath();
        ctx.moveTo(x0, y0);
        ctx.lineTo(0.5 * (x0 + x1) - 4, 0.5 * (y0 + y1));
        ctx.lineTo(x1, y1);
        ctx.stroke();
      }
      // Basal massive sulfide lens for VMS or impact melt sheet
      if (sulf >= 0.35) {
        ctx.strokeStyle = "#f59e0b";
        ctx.lineWidth = Math.min(4.0, Math.max(1.8, bandH * 0.22));
        ctx.beginPath();
        for (let s = 4; s <= steps - 4; s++) {
          const x = botPts[s][0];
          const y = botPts[s][1] - 1.5;
          if (s === 4) ctx.moveTo(x, y);
          else ctx.lineTo(x, y);
        }
        ctx.stroke();
      }
    }

    // G. Alluvial Placer Gold / Tin Nuggets (ONLY when `gold` or `tin` grade >= 0.06; barren quartz draws pale pebbles)
    if (layer.kind === "S" && (lname === "conglomerate" || lname === "sandstone")) {
      const placerGrade = Math.max(goldGrade, tinGrade);
      if (placerGrade >= 0.06) {
        const nuggetCount = Math.min(9, Math.max(2, Math.round(1.5 + placerGrade * 8.0)));
        const nuggetR = 1.25 + 1.25 * Math.min(1.0, placerGrade);
        for (let n = 0; n < nuggetCount; n++) {
          const u = 0.08 + 0.84 * detRand(lSeed, n, 31);
          const idx = Math.min(steps, Math.floor(u * steps));
          const nx = colLeft + u * colW;
          const ny = botPts[idx][1] - 2.2;
          ctx.fillStyle = goldGrade >= tinGrade ? "#facc15" : "#cbd5e1";
          ctx.strokeStyle = goldGrade >= tinGrade ? "#713f12" : "#334155";
          ctx.lineWidth = 0.7;
          ctx.beginPath();
          ctx.arc(nx, ny, nuggetR, 0, Math.PI * 2);
          ctx.fill();
          ctx.stroke();
        }
      } else if ((minGrades.quartz || 0.0) >= 0.12) {
        const pebbleCount = Math.min(6, Math.max(2, Math.round((minGrades.quartz || 0.1) * 6)));
        for (let p = 0; p < pebbleCount; p++) {
          const u = 0.10 + 0.80 * detRand(lSeed, p, 33);
          const idx = Math.min(steps, Math.floor(u * steps));
          const px = colLeft + u * colW;
          const py = botPts[idx][1] - 2.0;
          ctx.fillStyle = "rgba(241, 245, 249, 0.65)";
          ctx.beginPath();
          ctx.arc(px, py, 1.5, 0, Math.PI * 2);
          ctx.fill();
        }
      }
    }

    // H. Precious & Semi-Precious Gemstones (crystal count & size scaled by continuous `grade`)
    let gemStyle = null;
    if (layer.kind !== "I" && minGrades.diamond >= 0.06) {
      gemStyle = { fill: "#67e8f9", stroke: "#ffffff", grade: minGrades.diamond };
    } else if (minGrades.ruby >= 0.06) {
      gemStyle = { fill: "#ef4444", stroke: "#fecdd3", grade: minGrades.ruby };
    } else if (minGrades.sapphire >= 0.06) {
      gemStyle = { fill: "#3b82f6", stroke: "#bfdbfe", grade: minGrades.sapphire };
    } else if (minGrades.emerald >= 0.06) {
      gemStyle = { fill: "#10b981", stroke: "#a7f3d0", grade: minGrades.emerald };
    } else if (minGrades.jade >= 0.06) {
      gemStyle = { fill: "#4ade80", stroke: "#dcfce7", grade: minGrades.jade };
    } else if (minGrades.lapis >= 0.06) {
      gemStyle = { fill: "#1d4ed8", stroke: "#facc15", grade: minGrades.lapis };
    } else if (minGrades.turquoise >= 0.06) {
      gemStyle = { fill: "#06b6d4", stroke: "#cffafe", grade: minGrades.turquoise };
    } else if (minGrades.opal >= 0.06) {
      gemStyle = { fill: "#f0abfc", stroke: "#fef08a", grade: minGrades.opal };
    }
    if (gemStyle) {
      const gemCount = Math.min(6, Math.max(1, Math.round(1.0 + gemStyle.grade * 5.0)));
      const gemR = 2.0 + 1.4 * Math.min(1.0, gemStyle.grade);
      for (let g = 0; g < gemCount; g++) {
        const u = ((g + 0.65) / (gemCount + 0.3)) + (detRand(lSeed, g, 41) - 0.5) * 0.06;
        const uClamped = Math.max(0.08, Math.min(0.92, u));
        const idx = Math.min(steps, Math.floor(uClamped * steps));
        const gx = colLeft + uClamped * colW;
        const gy = topPts[idx][1] * 0.48 + botPts[idx][1] * 0.52;
        ctx.fillStyle = gemStyle.fill;
        ctx.strokeStyle = gemStyle.stroke;
        ctx.lineWidth = 0.9;
        ctx.beginPath();
        ctx.moveTo(gx, gy - gemR * 1.1);
        ctx.lineTo(gx + gemR * 0.9, gy);
        ctx.lineTo(gx, gy + gemR * 1.1);
        ctx.lineTo(gx - gemR * 0.9, gy);
        ctx.closePath();
        ctx.fill();
        ctx.stroke();
      }
    }

    ctx.restore(); // End layer clip

    // Draw layer boundary stroke (highlight if hovered)
    const isHovered = view.hoveredStratLayerIdx === i && (view.hoveredStratInterfaceIdx === undefined || view.hoveredStratInterfaceIdx < 0);
    traceLayerPoly();
    ctx.strokeStyle = isHovered ? "#38bdf8" : style.stroke;
    ctx.lineWidth = isHovered ? 2.0 : 0.9;
    ctx.stroke();

    hitBands.push({
      idx: i,
      layer,
      yTop: Math.max(padTop, Math.min(cssH - padBot, yTopBase)),
      yBot: Math.max(padTop, Math.min(cssH - padBot, yBotBase)),
      midY: Math.max(padTop + 6, Math.min(cssH - padBot - 6, midY)),
      style,
    });
  }

  // Layer-to-Layer Contact Interface Deposits (Skarns, Pegmatite-Schist Emeralds, Redox Traps, Unconformity Placers)
  const hitInterfaces = [];
  for (let ii = 0; ii < (strat.interfaces || []).length; ii++) {
    const intf = strat.interfaces[ii];
    const yBase = kmToY(intf.elevationKm);
    if (yBase < padTop - 8 || yBase > cssH - padBot + 8) continue;
    const depJoined = (intf.deposits || []).join(" ");
    const grade = Math.max(0.06, Math.min(1.0, intf.peakGrade || 0.25));
    const isIntfHovered = view.hoveredStratInterfaceIdx === ii;

    let strokeCol = "#f59e0b";
    let glowCol = "rgba(245, 158, 11, 0.35)";
    if (depJoined.includes("Emerald")) {
      strokeCol = "#10b981";
      glowCol = "rgba(16, 185, 129, 0.42)";
    } else if (depJoined.includes("Ruby")) {
      strokeCol = "#ef4444";
      glowCol = "rgba(239, 68, 68, 0.40)";
    } else if (depJoined.includes("Jade")) {
      strokeCol = "#4ade80";
      glowCol = "rgba(74, 222, 128, 0.40)";
    } else if (depJoined.includes("Lapis")) {
      strokeCol = "#3b82f6";
      glowCol = "rgba(59, 130, 246, 0.40)";
    } else if (depJoined.includes("Diamond")) {
      strokeCol = "#67e8f9";
      glowCol = "rgba(103, 232, 249, 0.42)";
    } else if (depJoined.includes("Gold")) {
      strokeCol = "#facc15";
      glowCol = "rgba(250, 204, 21, 0.42)";
    } else if (depJoined.includes("Tungsten")) {
      strokeCol = "#e2e8f0";
      glowCol = "rgba(226, 232, 240, 0.38)";
    }

    const haloPx = Math.max(3.2, Math.min(9.0, 2.5 + grade * 5.5));
    ctx.strokeStyle = isIntfHovered ? "rgba(56, 189, 248, 0.65)" : glowCol;
    ctx.lineWidth = isIntfHovered ? haloPx + 2.5 : haloPx;
    ctx.beginPath();
    for (let s = 0; s <= steps; s++) {
      const u = s / steps;
      const x = colLeft + u * colW;
      const y = yBase + foldOffsetPx(u, intf.depthKm || 0.0);
      if (s === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.stroke();

    ctx.strokeStyle = isIntfHovered ? "#fef08a" : strokeCol;
    ctx.lineWidth = isIntfHovered ? 2.2 : 1.4 + grade * 1.1;
    ctx.setLineDash([5, 2]);
    ctx.stroke();
    ctx.setLineDash([]);

    hitInterfaces.push({
      idx: ii,
      intf,
      yBase: Math.max(padTop + 4, Math.min(cssH - padBot - 4, yBase)),
      strokeCol,
    });
  }

  // Kimberlite Diatreme Pipe ascending from the mantle Moho through the crust
  for (let i = 0; i < layers.length; i++) {
    const kl = layers[i];
    if (kl.kind === "I" && typeof kl.felsity === "number" && kl.felsity <= 0.35) {
      const pipeU = 0.5 + (detRand((strat.tileIdx || 0) + 17, i, 77) - 0.5) * 0.28;
      const pipeX = colLeft + pipeU * colW;
      const yPipeTop = kmToY(kl.topKm);
      const yPipeBot = Math.min(cssH + 6, Math.max(yPipeTop + 14, yMohoBase));
      const topHalfW = 7.5;
      const botHalfW = 2.2;

      ctx.fillStyle = "rgba(20, 53, 55, 0.92)";
      ctx.strokeStyle = "#2dd4bf";
      ctx.lineWidth = 1.1;
      ctx.beginPath();
      ctx.moveTo(pipeX - topHalfW, yPipeTop);
      ctx.lineTo(pipeX + topHalfW, yPipeTop);
      ctx.lineTo(pipeX + botHalfW, yPipeBot);
      ctx.lineTo(pipeX - botHalfW, yPipeBot);
      ctx.closePath();
      ctx.fill();
      ctx.stroke();

      const hasDiamonds = (kl.bogginess || 0) >= 0.05;
      const sparkCount = 6;
      for (let sp = 0; sp < sparkCount; sp++) {
        const frac = (sp + 0.6) / (sparkCount + 0.4);
        const sy = yPipeTop + frac * (yPipeBot - yPipeTop);
        const curHalfW = topHalfW * (1 - frac) + botHalfW * frac;
        const sx = pipeX + (detRand(i + 3, sp, 88) - 0.5) * 1.2 * curHalfW;
        if (hasDiamonds && sp % 2 === 0) {
          ctx.fillStyle = "#67e8f9";
          ctx.strokeStyle = "#ffffff";
          ctx.lineWidth = 0.85;
          ctx.beginPath();
          ctx.moveTo(sx, sy - 2.8);
          ctx.lineTo(sx + 2.3, sy);
          ctx.lineTo(sx, sy + 2.8);
          ctx.lineTo(sx - 2.3, sy);
          ctx.closePath();
          ctx.fill();
          ctx.stroke();
        } else {
          // Mantle peridot (olivine green) & pyrope garnet (crimson) xenoliths
          ctx.fillStyle = sp % 2 === 0 ? "#84cc16" : "#f43f5e";
          ctx.beginPath();
          ctx.arc(sx, sy, 1.6, 0, Math.PI * 2);
          ctx.fill();
        }
      }
    }
  }

  // Moho boundary line
  if (yMohoBase >= padTop && yMohoBase <= cssH) {
    ctx.strokeStyle = "#fb923c";
    ctx.lineWidth = 1.6;
    ctx.setLineDash([4, 2]);
    ctx.beginPath();
    for (let s = 0; s <= steps; s++) {
      const u = s / steps;
      const x = colLeft + u * colW;
      const y = yMohoBase + foldOffsetPx(u, elevKm - mohoKm);
      if (s === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.stroke();
    ctx.setLineDash([]);
  }

  // Sea level dashed line across column
  if (ySea >= padTop && ySea <= cssH) {
    ctx.strokeStyle = "rgba(56, 189, 248, 0.75)";
    ctx.lineWidth = 1.1;
    ctx.setLineDash([3, 3]);
    ctx.beginPath();
    ctx.moveTo(colLeft, ySea);
    ctx.lineTo(colRight, ySea);
    ctx.stroke();
    ctx.setLineDash([]);
  }

  ctx.restore(); // End column clip

  // Column border frame
  ctx.strokeStyle = "#334155";
  ctx.lineWidth = 1.1;
  ctx.strokeRect(colLeft, padTop, colW, plotH);

  // 4. Left Depth & Elevation Ruler
  ctx.fillStyle = "#94a3b8";
  ctx.font = "9.5px sans-serif";
  ctx.textAlign = "right";
  ctx.textBaseline = "middle";

  const drawLeftTick = (km, label, color = "#94a3b8") => {
    const y = kmToY(km);
    if (y < padTop + 4 || y > cssH - padBot - 4) return;
    ctx.strokeStyle = color;
    ctx.lineWidth = 1.0;
    ctx.beginPath();
    ctx.moveTo(colLeft - 4, y);
    ctx.lineTo(colLeft, y);
    ctx.stroke();
    ctx.fillStyle = color;
    ctx.fillText(label, colLeft - 6, y);
  };

  drawLeftTick(elevKm, `${elevKm >= 0 ? "+" : ""}${elevKm.toFixed(1)}k`, "#e2e8f0");
  if (Math.abs(elevKm) > 1.2) {
    drawLeftTick(0.0, "0.0k", "#38bdf8");
  }
  const tickStepKm = spanKm > 25 ? 10 : spanKm > 12 ? 5 : 2;
  const firstTickKm = Math.floor(Math.min(elevKm - 1.0, 0.0) / tickStepKm) * tickStepKm;
  for (let tk = firstTickKm; tk > viewBottomKm + 1.0; tk -= tickStepKm) {
    if (Math.abs(tk - elevKm) < 1.2 || Math.abs(tk - mohoKm) < 1.5) continue;
    drawLeftTick(tk, `${tk.toFixed(0)}k`, "#64748b");
  }
  if (!view.stratigraphyTopZoom && yMohoBase >= padTop + 6 && yMohoBase <= cssH - 6) {
    drawLeftTick(mohoKm, `Moho`, "#fb923c");
  }

  // 5. Right-side Layer & Contact Interface Callouts (de-overlapped in depth order)
  const minLabelSpacing = 13.5;
  let lastLabelY = padTop - 6;
  ctx.textAlign = "left";
  ctx.textBaseline = "middle";

  const shortDepositBadge = (depJoined) => {
    if (depJoined.includes("Diamond")) return " 💎";
    if (depJoined.includes("Ruby")) return " [Ruby]";
    if (depJoined.includes("Sapphire")) return " [Sapph]";
    if (depJoined.includes("Emerald")) return " [Emer]";
    if (depJoined.includes("Jade")) return " [Jade]";
    if (depJoined.includes("Lapis")) return " [Lapis]";
    if (depJoined.includes("Turquoise")) return " [Turq]";
    if (depJoined.includes("Opal")) return " [Opal]";
    if (depJoined.includes("Kimberlite Pipe")) return " [Pipe]";
    if (depJoined.includes("Banded Iron")) return " [BIF]";
    if (depJoined.includes("Anthracite") || depJoined.includes("Bituminous") || depJoined.includes("Lignite") || depJoined.includes("Peat")) return " [Coal]";
    if (depJoined.includes("Graphite")) return " [C]";
    if (depJoined.includes("Gold")) return " [Au]";
    if (depJoined.includes("Tungsten")) return " [W/Sn]";
    if (depJoined.includes("Tin")) return " [Sn]";
    if (depJoined.includes("Copper") || depJoined.includes("Sulfide") || depJoined.includes("Black-Smoker") || depJoined.includes("Skarn")) return " [Cu/S]";
    if (depJoined.includes("Halite") || depJoined.includes("Gypsum") || depJoined.includes("Potash")) return " [Salt]";
    if (depJoined.includes("Flint") || depJoined.includes("Chert")) return " [Flint]";
    if (depJoined.includes("Quartz")) return " [Qtz]";
    if (depJoined.includes("Karst")) return " [Cave]";
    return "";
  };

  const callouts = [];
  for (let i = 0; i < hitBands.length; i++) {
    const hb = hitBands[i];
    const layer = hb.layer;
    const isHovered =
      view.hoveredStratLayerIdx === hb.idx &&
      (view.hoveredStratInterfaceIdx === undefined || view.hoveredStratInterfaceIdx < 0);
    if (hb.yBot - hb.yTop < 3.5 && !isHovered && (!layer.deposits || layer.deposits.length === 0)) {
      continue;
    }
    const deps = layer.deposits || [];
    const badge = shortDepositBadge(deps.join(" "));
    const dispName =
      layer.kind === "I" && typeof layer.felsity === "number" && layer.felsity <= 0.35
        ? "kimberlite"
        : layer.name;
    callouts.push({
      midY: hb.midY,
      isHovered,
      hasBadge: Boolean(badge),
      isInterface: false,
      text: `${dispName} (${layer.thicknessKm.toFixed(1)}k)${badge}`,
      color: isHovered ? "#fef08a" : badge ? "#f8fafc" : "#cbd5e1",
    });
  }
  for (let ii = 0; ii < hitInterfaces.length; ii++) {
    const hi = hitInterfaces[ii];
    const intf = hi.intf;
    const isHovered = view.hoveredStratInterfaceIdx === hi.idx;
    const badge = shortDepositBadge((intf.deposits || []).join(" "));
    const pct = Math.round((intf.peakGrade || 0) * 100);
    callouts.push({
      midY: hi.yBase,
      isHovered,
      hasBadge: true,
      isInterface: true,
      text: `⇄ contact${badge} ${pct}%`,
      color: isHovered ? "#fef08a" : hi.strokeCol,
    });
  }
  callouts.sort((a, b) => a.midY - b.midY);

  for (let c = 0; c < callouts.length; c++) {
    const item = callouts[c];
    const labelY = Math.max(lastLabelY + minLabelSpacing, Math.min(cssH - 8, item.midY));
    if (labelY > cssH - 5) break;
    lastLabelY = labelY;

    const edgeY = Math.max(padTop + 2, Math.min(cssH - padBot - 2, item.midY));
    ctx.strokeStyle = item.isHovered
      ? "#38bdf8"
      : item.isInterface
        ? item.color
        : "rgba(100, 116, 139, 0.55)";
    ctx.lineWidth = item.isHovered ? 1.4 : item.isInterface ? 1.0 : 0.8;
    ctx.beginPath();
    ctx.moveTo(colRight, edgeY);
    ctx.lineTo(colRight + 7, labelY);
    ctx.stroke();

    ctx.fillStyle = item.color;
    ctx.font = item.isHovered || item.hasBadge ? "bold 9.5px sans-serif" : "9.5px sans-serif";
    ctx.fillText(item.text, colRight + 9, labelY, cssW - colRight - 11);
  }

  ctx.restore();

  // Interactive hover handler on canvas to inspect individual layers & contact interfaces
  const updateHoverReadout = (hoveredLayerIdx, hoveredIntfIdx) => {
    const labelEl = document.getElementById("stratigraphy-hover-label");
    if (!labelEl) return;
    if (hoveredIntfIdx >= 0 && strat.interfaces && strat.interfaces[hoveredIntfIdx]) {
      const intf = strat.interfaces[hoveredIntfIdx];
      const upL = strat.layers[intf.upperLayerIdx];
      const loL = strat.layers[intf.lowerLayerIdx];
      const upName = upL ? upL.name : "upper";
      const loName = loL ? loL.name : "lower";
      const occSummary = (intf.occurrences || [])
        .map(
          (o) =>
            `${o.label} [zone ${o.thicknessKm.toFixed(2)} km, abundance ${o.abundance.toFixed(3)}]`
        )
        .join(", ");
      labelEl.textContent = `Contact Interface (${upName} ⇄ ${loName}, depth ${intf.depthKm.toFixed(2)} km) | ${occSummary || (intf.deposits || []).join(", ")}`;
    } else if (hoveredLayerIdx >= 0 && strat.layers[hoveredLayerIdx]) {
      const l = strat.layers[hoveredLayerIdx];
      const lDispName =
        l.kind === "I" && typeof l.felsity === "number" && l.felsity <= 0.35
          ? "kimberlite"
          : l.name;
      const kindWord =
        l.kind === "I" ? "Igneous" : l.kind === "S" ? "Sedimentary" : "Metamorphic";
      const occList =
        l.occurrences && l.occurrences.length > 0
          ? l.occurrences.map(
              (o) => `${o.label} [abund ${o.abundance.toFixed(3)}]`
            )
          : l.deposits || [];
      const depsText =
        occList.length > 0
          ? ` | Veins/Deposits: ${occList.join(", ")}`
          : " | No major mineral veins";
      const fmtOpt = (v) => (typeof v === "number" ? v.toFixed(2) : "—");
      labelEl.textContent = `Layer #${hoveredLayerIdx + 1}: ${lDispName} (${kindWord}, ${l.thicknessKm.toFixed(2)} km thick, depth ${l.depthTopKm.toFixed(1)}–${l.depthBottomKm.toFixed(1)} km) | f=${l.felsity.toFixed(2)} bog=${fmtOpt(l.bogginess)} calc=${fmtOpt(l.calcity)} qtz=${fmtOpt(l.quartzity)} sulf=${fmtOpt(l.sulfidity)} sal=${fmtOpt(l.salinity)}${depsText}`;
    } else {
      const totalDeps = [];
      for (const l of strat.layers) {
        for (const d of l.deposits || []) {
          if (!totalDeps.includes(d)) totalDeps.push(d);
        }
      }
      for (const intf of strat.interfaces || []) {
        for (const d of intf.deposits || []) {
          if (!totalDeps.includes(d)) totalDeps.push(d);
        }
      }
      labelEl.textContent =
        totalDeps.length > 0
          ? `Tile #${strat.tileIdx} (${strat.layers.length} layers, ${strat.thicknessKm.toFixed(1)} km crust, fold ${(mountNorm * 100).toFixed(0)}%) — Deposits: ${totalDeps.join(" · ")}`
          : `Tile #${strat.tileIdx} (${strat.layers.length} layers, ${strat.thicknessKm.toFixed(1)} km crust, fold ${(mountNorm * 100).toFixed(0)}%) — Hover any rock layer or contact boundary for composition & veins.`;
    }
  };

  updateHoverReadout(
    typeof view.hoveredStratLayerIdx === "number" ? view.hoveredStratLayerIdx : -1,
    typeof view.hoveredStratInterfaceIdx === "number" ? view.hoveredStratInterfaceIdx : -1
  );

  canvas.onmousemove = (ev) => {
    const r = canvas.getBoundingClientRect();
    const my = ev.clientY - r.top;
    const kmAtMouse = yToKm(my);
    let foundIntf = -1;
    for (let ii = 0; ii < hitInterfaces.length; ii++) {
      if (Math.abs(my - hitInterfaces[ii].yBase) <= 4.5) {
        foundIntf = hitInterfaces[ii].idx;
        break;
      }
    }
    let foundIdx = -1;
    if (foundIntf < 0) {
      for (let i = 0; i < strat.layers.length; i++) {
        const l = strat.layers[i];
        if (kmAtMouse <= l.topKm + 0.15 && kmAtMouse >= l.bottomKm - 0.15) {
          foundIdx = i;
          break;
        }
      }
    }
    if (
      view.hoveredStratLayerIdx !== foundIdx ||
      view.hoveredStratInterfaceIdx !== foundIntf
    ) {
      view.hoveredStratLayerIdx = foundIdx;
      view.hoveredStratInterfaceIdx = foundIntf;
      paintStratigraphySideCanvas(view);
    }
  };

  canvas.onmouseleave = () => {
    if (
      (view.hoveredStratLayerIdx !== -1 && view.hoveredStratLayerIdx !== undefined) ||
      (view.hoveredStratInterfaceIdx !== -1 && view.hoveredStratInterfaceIdx !== undefined)
    ) {
      view.hoveredStratLayerIdx = -1;
      view.hoveredStratInterfaceIdx = -1;
      paintStratigraphySideCanvas(view);
    }
  };
}

function scheduleStratigraphySidePaint(view, explicitTileIdx) {
  if (!view || typeof window === "undefined") return;
  const s = view.session;
  const tid =
    explicitTileIdx !== undefined
      ? explicitTileIdx
      : view.selectedTileIdx !== undefined
        ? view.selectedTileIdx
        : view.selectedFaceIdx !== undefined
          ? view.selectedFaceIdx
          : -1;

  if (tid >= 0 && s && typeof s.inspect_tile_stratigraphy_json === "function") {
    try {
      view.tileStratigraphy = JSON.parse(s.inspect_tile_stratigraphy_json(tid));
    } catch (_) {
      view.tileStratigraphy = null;
    }
  } else if (tid < 0) {
    view.tileStratigraphy = null;
  }
  view.hoveredStratLayerIdx = -1;
  view.hoveredStratInterfaceIdx = -1;

  window.requestAnimationFrame(() => paintStratigraphySideCanvas(view));
  setTimeout(() => paintStratigraphySideCanvas(view), 45);
}

export const toggleStratigraphySideZoom = () => {
  if (!activeView) return;
  activeView.stratigraphyTopZoom = !activeView.stratigraphyTopZoom;
  paintStratigraphySideCanvas(activeView);
};

export const selectInspectorLanguage = (langId) => () => {
  if (!activeView || !activeView.session) return;
  activeView.selectedLangId = activeView.selectedLangId === langId ? -1 : langId;
  refreshActiveInspectorView(activeView);
};

export const selectInspectorNation = (natId) => () => {
  if (!activeView || !activeView.session) return;
  activeView.selectedNationId = activeView.selectedNationId === natId ? -1 : natId;
  refreshActiveInspectorView(activeView);
};

export const closeInspectorLanguage = () => {
  if (!activeView || !activeView.session) return;
  activeView.selectedLangId = -1;
  refreshActiveInspectorView(activeView);
};

export const closeInspectorNation = () => {
  if (!activeView || !activeView.session) return;
  activeView.selectedNationId = -1;
  refreshActiveInspectorView(activeView);
};

function buildHistoryInfo(view) {
  const s = view.session;
  const planetName = s.is_custom_planet()
    ? `Saved Planet (${s.land_percent()}% land)`
    : "Earth (ETOPO2)";
  const selectedReport =
    view.selectedTileIdx >= 0
      ? s.inspect_tile(view.selectedTileIdx)
      : "Click any state on the globe to open its zoomed-in State Map, demographics, imports & exports, and diplomacy, or click any tile in the state to inspect its local population and native flora & fauna.";
  const bundle = buildHistoryInspectorBundle(view);
  if (s.stage_index() !== 2) {
    scheduleStratigraphySidePaint(view, view.selectedTileIdx);
  }
  return {
    planetName,
    tileCount: s.tile_count(),
    ticks: s.history_ticks(),
    years: s.years(),
    activeStates: s.active_state_count(),
    populationText: formatPopulationThousands(s.total_population_thousands()),
    isRunning: Boolean(view.isRunning),
    hasSavedPlanet: getSavedSnapshotBytes() !== null,
    selectedReport,
    stateView: bundle.stateView,
    tileView: bundle.tileView,
    languageDetail: bundle.languageDetail,
    nationDetail: bundle.nationDetail,
  };
}

function stopHistoryLoop(view) {
  view.isRunning = false;
  if (view.timerId !== null && view.timerId !== undefined) {
    clearTimeout(view.timerId);
    view.timerId = null;
  }
}

function scheduleHistoryTick(view) {
  if (!view.isRunning || activeView !== view) return;
  view.timerId = setTimeout(() => {
    if (!view.isRunning || activeView !== view) return;
    view.session.step();
    updateHistoryDataBuffers(view, false);
    if (view.setHistoryInfo) {
      view.setHistoryInfo(buildHistoryInfo(view))();
    }
    if (view.isRunning) {
      scheduleHistoryTick(view);
    }
  }, 150);
}

export const mountHistoryView =
  (aspect) => (showRivers) => (setHistoryInfo) => () => {
    const canvas = document.getElementById("sphere-canvas");
    if (!canvas) {
      return () => {};
    }

    const view = createBaseSphereView(canvas, false);
    view.session = new WasmHistorySession();
    view.inspectorMode = "history";
    view.aspect = aspect;
    view.showRivers = showRivers;
    view.selectedTileIdx = -1;
    view.selectedStateId = -1;
    view.selectedLangId = -1;
    view.selectedNationId = -1;
    view.stateMapPolygons = [];
    view.isRunning = false;
    view.timerId = null;
    view.setHistoryInfo = setHistoryInfo;

    if (activeClimateSnapshotBytes) {
      view.session.load_snapshot_bytes(activeClimateSnapshotBytes);
    }

    const raycaster = new THREE.Raycaster();
    const pointerNdc = new THREE.Vector2();

    const handleCanvasClick = (ev) => {
      if (!view.session) return;
      const rect = canvas.getBoundingClientRect();
      const u = (ev.clientX - rect.left) / rect.width;
      const v = (ev.clientY - rect.top) / rect.height;

      pointerNdc.set(2.0 * u - 1.0, 1.0 - 2.0 * v);
      view.mesh.updateMatrixWorld(true);
      view.camera.updateMatrixWorld(true);
      raycaster.setFromCamera(pointerNdc, view.camera);
      const hits = raycaster.intersectObject(view.mesh, false);
      if (hits.length > 0 && hits[0].face && view.vertexTileIds) {
        view.selectedTileIdx = view.vertexTileIds[hits[0].face.a];
        view.selectedStateId = -1;
        updateHistoryDataBuffers(view, false);
        view.setHistoryInfo(buildHistoryInfo(view))();
      }
    };

    canvas.addEventListener("click", handleCanvasClick);

    activeView = view;
    updateHistoryDataBuffers(view, true);
    setHistoryInfo(buildHistoryInfo(view))();

    return () => {
      canvas.removeEventListener("click", handleCanvasClick);
      stopHistoryLoop(view);
      if (activeView === view) {
        activeView = null;
      }
      view.dispose();
      view.session.free();
    };
  };

export const stepHistory = (setHistoryInfo) => () => {
  if (!activeView || !activeView.session || activeView.isRunning) return;
  activeView.session.step();
  updateHistoryDataBuffers(activeView, false);
  setHistoryInfo(buildHistoryInfo(activeView))();
};

export const toggleHistoryRunning = (setHistoryInfo) => () => {
  if (!activeView || !activeView.session) return;
  if (activeView.isRunning) {
    stopHistoryLoop(activeView);
    setHistoryInfo(buildHistoryInfo(activeView))();
  } else {
    activeView.isRunning = true;
    setHistoryInfo(buildHistoryInfo(activeView))();
    scheduleHistoryTick(activeView);
  }
};

export const resetHistory = (setHistoryInfo) => () => {
  if (!activeView || !activeView.session) return;
  stopHistoryLoop(activeView);
  activeView.session.reset();
  activeView.selectedTileIdx = -1;
  activeView.selectedStateId = -1;
  activeView.selectedLangId = -1;
  activeView.selectedNationId = -1;
  updateHistoryDataBuffers(activeView, false);
  setHistoryInfo(buildHistoryInfo(activeView))();
};

export const setHistoryAspect = (aspect) => () => {
  if (!activeView || !activeView.session) return;
  activeView.aspect = aspect;
  updateHistoryDataBuffers(activeView, false);
};

export const setHistoryShowRivers = (showRivers) => () => {
  if (!activeView || !activeView.session) return;
  activeView.showRivers = showRivers;
  updateHistoryDataBuffers(activeView, false);
};

export const saveHistoryPlanet = (setHistoryInfo) => () => {
  if (!activeView || !activeView.session) return;
  const bytes = activeView.session.encode_snapshot();
  if (!bytes || bytes.length === 0) return;
  storeSavedSnapshotBytes(bytes);
  activeClimateSnapshotBytes = bytes;
  const yrs = activeView.session.years();
  triggerBinaryDownload(`tec11-history-${yrs}yr.tec11`, bytes);
  setHistoryInfo(buildHistoryInfo(activeView))();
};

export const loadEarthIntoHistory = (setHistoryInfo) => () => {
  if (!activeView || !activeView.session) return;
  stopHistoryLoop(activeView);
  activeClimateSnapshotBytes = null;
  activeView.session.load_earth();
  activeView.selectedTileIdx = -1;
  activeView.selectedStateId = -1;
  activeView.selectedLangId = -1;
  activeView.selectedNationId = -1;
  updateHistoryDataBuffers(activeView, true);
  setHistoryInfo(buildHistoryInfo(activeView))();
};

export const loadSavedPlanetIntoHistory = (setHistoryInfo) => () => {
  if (!activeView || !activeView.session) return;
  const saved = getSavedSnapshotBytes();
  if (!saved) return;
  stopHistoryLoop(activeView);
  if (activeView.session.load_snapshot_bytes(saved)) {
    activeClimateSnapshotBytes = saved;
    activeView.selectedTileIdx = -1;
    activeView.selectedStateId = -1;
    activeView.selectedLangId = -1;
    activeView.selectedNationId = -1;
    updateHistoryDataBuffers(activeView, true);
    setHistoryInfo(buildHistoryInfo(activeView))();
  }
};

export const loadFilePlanetIntoHistory = (setHistoryInfo) => () => {
  if (!activeView || !activeView.session) return;
  pickBinarySnapshotFile((bytes) => {
    if (!activeView || !activeView.session) return;
    stopHistoryLoop(activeView);
    if (activeView.session.load_snapshot_bytes(bytes)) {
      storeSavedSnapshotBytes(bytes);
      activeClimateSnapshotBytes = bytes;
      activeView.selectedTileIdx = -1;
      activeView.selectedStateId = -1;
      activeView.selectedLangId = -1;
      activeView.selectedNationId = -1;
      updateHistoryDataBuffers(activeView, true);
      setHistoryInfo(buildHistoryInfo(activeView))();
    }
  });
};

// ============================================================================
// Mode 13b: Travel Routes & Maritime Navigation (`TravelScreen`)
// ============================================================================

function updateTravelDataBuffers(view, rebuildGeometry = false) {
  const { session, geometry } = view;
  if (rebuildGeometry) {
    const positions = session.mesh_positions();
    const normals = session.mesh_normals();
    view.vertexTileIds = session.vertex_tile_ids();
    geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
    geometry.setAttribute("normal", new THREE.Float32BufferAttribute(normals, 3));
    geometry.computeBoundingSphere();
  }
  view.tileColors = session.tile_colors_rgb(
    view.aspect,
    Boolean(view.showRivers),
    view.selectedTileIdx
  );
  const colors = computeHistoryVertexColors(view);
  geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));

  const buffers = [];
  if (view.showWindsCurrents || view.aspect === "winds_currents") {
    buffers.push(session.wind_current_vector_segments());
  }
  buffers.push(session.travel_route_segments_colored(Boolean(view.showPotential)));
  setColoredArrowSegments(view, buffers);

  view.paint();
}

function buildTravelInfo(view) {
  const s = view.session;
  const planetName = s.is_custom_planet()
    ? `Saved Planet (${s.land_percent()}% land)`
    : "Earth (ETOPO2)";
  const selectedReport =
    view.selectedTileIdx >= 0
      ? s.inspect_travel_tile(view.selectedTileIdx)
      : "Click any coastal port, ocean tile, or river corridor on the globe to inspect prevailing wind & current vectors and travel routes (yellow = active trade, red = naval conflict, cyan/emerald = connected sea/river lane, violet = hypothetical/unconnected drift).";
  const bundle = buildHistoryInspectorBundle(view);
  if (s.stage_index() !== 2) {
    scheduleStratigraphySidePaint(view, view.selectedTileIdx);
  }
  return {
    planetName,
    tileCount: s.tile_count(),
    ticks: s.history_ticks(),
    years: s.years(),
    activeStates: s.active_state_count(),
    populationText: formatPopulationThousands(s.total_population_thousands()),
    activeRoutes: s.active_travel_route_count(),
    potentialRoutes: s.potential_travel_route_count(),
    portCount: s.water_access_port_count(),
    tradeRoutes: s.maritime_trade_route_count(),
    conflictRoutes: s.maritime_conflict_route_count(),
    isRunning: Boolean(view.isRunning),
    hasSavedPlanet: getSavedSnapshotBytes() !== null,
    selectedReport,
    stateView: bundle.stateView,
    tileView: bundle.tileView,
    languageDetail: bundle.languageDetail,
    nationDetail: bundle.nationDetail,
  };
}

function scheduleTravelTick(view) {
  if (!view.isRunning || activeView !== view) return;
  view.timerId = setTimeout(() => {
    if (!view.isRunning || activeView !== view) return;
    view.session.step();
    updateTravelDataBuffers(view, false);
    if (view.setTravelInfo) {
      view.setTravelInfo(buildTravelInfo(view))();
    }
    if (view.isRunning) {
      scheduleTravelTick(view);
    }
  }, 150);
}

export const mountTravelView =
  (aspect) =>
  (showPotential) =>
  (showWindsCurrents) =>
  (showRivers) =>
  (setTravelInfo) =>
  () => {
    const canvas = document.getElementById("sphere-canvas");
    if (!canvas) {
      return () => {};
    }

    const view = createBaseSphereView(canvas, false);
    view.session = new WasmHistorySession();
    view.inspectorMode = "travel";
    view.aspect = aspect;
    view.showPotential = showPotential;
    view.showWindsCurrents = showWindsCurrents;
    view.showRivers = showRivers;
    view.selectedTileIdx = -1;
    view.selectedStateId = -1;
    view.selectedLangId = -1;
    view.selectedNationId = -1;
    view.stateMapPolygons = [];
    view.isRunning = false;
    view.timerId = null;
    view.setTravelInfo = setTravelInfo;

    if (activeClimateSnapshotBytes) {
      view.session.load_snapshot_bytes(activeClimateSnapshotBytes);
    }

    const raycaster = new THREE.Raycaster();
    const pointerNdc = new THREE.Vector2();

    const handleCanvasClick = (ev) => {
      if (!view.session) return;
      const rect = canvas.getBoundingClientRect();
      const u = (ev.clientX - rect.left) / rect.width;
      const v = (ev.clientY - rect.top) / rect.height;

      pointerNdc.set(2.0 * u - 1.0, 1.0 - 2.0 * v);
      view.mesh.updateMatrixWorld(true);
      view.camera.updateMatrixWorld(true);
      raycaster.setFromCamera(pointerNdc, view.camera);
      const hits = raycaster.intersectObject(view.mesh, false);
      if (hits.length > 0 && hits[0].face && view.vertexTileIds) {
        view.selectedTileIdx = view.vertexTileIds[hits[0].face.a];
        view.selectedStateId = -1;
        updateTravelDataBuffers(view, false);
        view.setTravelInfo(buildTravelInfo(view))();
      }
    };

    canvas.addEventListener("click", handleCanvasClick);

    activeView = view;
    updateTravelDataBuffers(view, true);
    setTravelInfo(buildTravelInfo(view))();

    return () => {
      canvas.removeEventListener("click", handleCanvasClick);
      stopHistoryLoop(view);
      if (activeView === view) {
        activeView = null;
      }
      view.dispose();
      view.session.free();
    };
  };

export const stepTravel = (setTravelInfo) => () => {
  if (!activeView || !activeView.session || activeView.isRunning) return;
  activeView.session.step();
  updateTravelDataBuffers(activeView, false);
  setTravelInfo(buildTravelInfo(activeView))();
};

export const toggleTravelRunning = (setTravelInfo) => () => {
  if (!activeView || !activeView.session) return;
  if (activeView.isRunning) {
    stopHistoryLoop(activeView);
    setTravelInfo(buildTravelInfo(activeView))();
  } else {
    activeView.isRunning = true;
    setTravelInfo(buildTravelInfo(activeView))();
    scheduleTravelTick(activeView);
  }
};

export const resetTravel = (setTravelInfo) => () => {
  if (!activeView || !activeView.session) return;
  stopHistoryLoop(activeView);
  activeView.session.reset();
  activeView.selectedTileIdx = -1;
  activeView.selectedStateId = -1;
  activeView.selectedLangId = -1;
  activeView.selectedNationId = -1;
  updateTravelDataBuffers(activeView, false);
  setTravelInfo(buildTravelInfo(activeView))();
};

export const setTravelDisplayOptions =
  (aspect) => (showPotential) => (showWindsCurrents) => (showRivers) => () => {
    if (!activeView || !activeView.session) return;
    activeView.aspect = aspect;
    activeView.showPotential = showPotential;
    activeView.showWindsCurrents = showWindsCurrents;
    activeView.showRivers = showRivers;
    updateTravelDataBuffers(activeView, false);
  };

export const saveTravelPlanet = (setTravelInfo) => () => {
  if (!activeView || !activeView.session) return;
  const bytes = activeView.session.encode_snapshot();
  if (!bytes || bytes.length === 0) return;
  storeSavedSnapshotBytes(bytes);
  activeClimateSnapshotBytes = bytes;
  const yrs = activeView.session.years();
  triggerBinaryDownload(`tec11-travel-history-${yrs}yr.tec11`, bytes);
  setTravelInfo(buildTravelInfo(activeView))();
};

export const loadEarthIntoTravel = (setTravelInfo) => () => {
  if (!activeView || !activeView.session) return;
  stopHistoryLoop(activeView);
  activeClimateSnapshotBytes = null;
  activeView.session.load_earth();
  activeView.selectedTileIdx = -1;
  activeView.selectedStateId = -1;
  activeView.selectedLangId = -1;
  activeView.selectedNationId = -1;
  updateTravelDataBuffers(activeView, true);
  setTravelInfo(buildTravelInfo(activeView))();
};

export const loadSavedPlanetIntoTravel = (setTravelInfo) => () => {
  if (!activeView || !activeView.session) return;
  const saved = getSavedSnapshotBytes();
  if (!saved) return;
  stopHistoryLoop(activeView);
  if (activeView.session.load_snapshot_bytes(saved)) {
    activeClimateSnapshotBytes = saved;
    activeView.selectedTileIdx = -1;
    activeView.selectedStateId = -1;
    activeView.selectedLangId = -1;
    activeView.selectedNationId = -1;
    updateTravelDataBuffers(activeView, true);
    setTravelInfo(buildTravelInfo(activeView))();
  }
};

export const loadFilePlanetIntoTravel = (setTravelInfo) => () => {
  if (!activeView || !activeView.session) return;
  pickBinarySnapshotFile((bytes) => {
    if (!activeView || !activeView.session) return;
    stopHistoryLoop(activeView);
    if (activeView.session.load_snapshot_bytes(bytes)) {
      storeSavedSnapshotBytes(bytes);
      activeClimateSnapshotBytes = bytes;
      activeView.selectedTileIdx = -1;
      activeView.selectedStateId = -1;
      activeView.selectedLangId = -1;
      activeView.selectedNationId = -1;
      updateTravelDataBuffers(activeView, true);
      setTravelInfo(buildTravelInfo(activeView))();
    }
  });
};

// ============================================================================
// Mode 14: Complete World Simulation (`worldsimulation.py`, `worlddisplay.py`, `worldpresenter.py`)
// ============================================================================

function computeWorldVertexColors(view) {
  const { vertexTileIds, tileColors } = view;
  const vertexCount = vertexTileIds.length;
  const colors = new Float32Array(vertexCount * 3);
  for (let v = 0; v < vertexCount; v++) {
    const t = vertexTileIds[v];
    colors[v * 3] = tileColors[t * 3];
    colors[v * 3 + 1] = tileColors[t * 3 + 1];
    colors[v * 3 + 2] = tileColors[t * 3 + 2];
  }
  return colors;
}

function updateWorldDataBuffers(view, rebuildGeometry = false) {
  const { session, geometry } = view;
  const currentTileCount = session.tile_count();
  if (rebuildGeometry || view.lastTileCount !== currentTileCount) {
    view.lastTileCount = currentTileCount;
    const positions = session.mesh_positions();
    const normals = session.mesh_normals();
    view.vertexTileIds = session.vertex_tile_ids();
    geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
    geometry.setAttribute("normal", new THREE.Float32BufferAttribute(normals, 3));
    geometry.computeBoundingSphere();
  }
  view.tileColors = session.tile_colors_rgb(view.aspect, view.selectedTileIdx);
  const colors = computeWorldVertexColors(view);
  geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
  if (view.aspect === "travel" && session.stage_index() === 2) {
    const routeSegs = session.travel_route_segments_colored();
    setColoredArrowSegments(view, [routeSegs]);
  } else {
    view.arrowLines.visible = false;
  }
  view.paint();
}

function rgbFloatToCss(rgb, fallbackR = 255, fallbackG = 230, fallbackB = 180) {
  if (!rgb || rgb.length < 3) {
    return `rgb(${fallbackR}, ${fallbackG}, ${fallbackB})`;
  }
  const r255 = Math.max(0, Math.min(255, Math.round(rgb[0] * 255)));
  const g255 = Math.max(0, Math.min(255, Math.round(rgb[1] * 255)));
  const b255 = Math.max(0, Math.min(255, Math.round(rgb[2] * 255)));
  return `rgb(${r255}, ${g255}, ${b255})`;
}

function buildWorldInfo(view) {
  const s = view.session;
  const selectedReport =
    view.selectedTileIdx >= 0
      ? s.inspect_tile(view.selectedTileIdx)
      : "Click any tile on the globe to inspect its rock strata, climate classification, and population / heritage ancestry tree.";
  const moonCount = s.param_moon_count();
  const moonMass = s.param_moon_mass();
  const moonDistTarget = s.param_moon_dist_radii();
  const meteorFreq = s.param_meteor_freq();
  const curDay = s.current_day_hours();
  const curTilt = s.current_tilt_deg();
  const curMoonDist = s.current_moon_dist_radii();
  const curTides = s.current_tidal_strength();
  const impacts = s.impact_count();
  const orbitalSummaryText =
    moonCount === 0
      ? `Day: ${curDay.toFixed(1)}h | Tilt: ${curTilt.toFixed(1)}° | Moons: 0 (Solar Tides ${curTides.toFixed(2)}x) | Meteors: ${meteorFreq.toFixed(1)}x (${impacts} hits)`
      : `Day: ${curDay.toFixed(1)}h (→${s.param_day_hours()}h) | Tilt: ${curTilt.toFixed(1)}° | Moons: ${moonCount} (${moonMass.toFixed(1)}x @ ${curMoonDist.toFixed(1)}→${moonDistTarget} Rp, Tides ${curTides.toFixed(1)}x) | Meteors: ${meteorFreq.toFixed(1)}x (${impacts} hits)`;
  const bundle = buildHistoryInspectorBundle(view);
  if (s.stage_index() !== 2) {
    scheduleStratigraphySidePaint(view, view.selectedTileIdx);
  }
  return {
    stageIndex: s.stage_index(),
    stageName: s.stage_name(),
    tileCount: s.tile_count(),
    yearsText: s.years_formatted(),
    populationText: formatPopulationThousands(s.total_population_thousands()),
    gridSize: s.param_grid_size(),
    dayHours: s.param_day_hours(),
    tiltDeg: s.param_tilt_deg(),
    landPct: s.param_land_pct(),
    atmDtMyr: s.param_atm_dt_myr(),
    lifeDtMyr: s.param_life_dt_myr(),
    peopleDtMyr: s.param_people_dt_myr(),
    moonCount,
    moonMassTenths: Math.round(moonMass * 10.0),
    moonDistRadii: moonDistTarget,
    meteorFreqTenths: Math.round(meteorFreq * 10.0),
    orbitalSummaryText,
    sunFilter: s.star_filter(),
    atmosphereId: s.atmosphere_id(),
    sunSpectralType: s.sun_spectral_type(),
    sunColorCss: rgbFloatToCss(s.sun_rgb(), 255, 255, 119),
    atmosphereName: s.atmosphere_name(),
    skyColorCss: rgbFloatToCss(s.sky_zenith_rgb(), 41, 130, 240),
    skyColorName: s.sky_color_name(),
    sunsetColorCss: rgbFloatToCss(s.sunset_rgb(), 250, 97, 36),
    sunsetColorName: s.sunset_color_name(),
    plantColorCss: rgbFloatToCss(s.plant_foliage_rgb(), 36, 138, 41),
    plantColorName: s.plant_color_name(),
    planetsSummaryText: s.planets_summary_text(),
    isRunning: Boolean(view.isRunning),
    hasSavedPlanet: getSavedSnapshotBytes() !== null,
    selectedReport,
    stateView: bundle.stateView,
    tileView: bundle.tileView,
    languageDetail: bundle.languageDetail,
    nationDetail: bundle.nationDetail,
  };
}

function stopWorldLoop(view) {
  view.isRunning = false;
  if (view.timerId !== null && view.timerId !== undefined) {
    clearTimeout(view.timerId);
    view.timerId = null;
  }
}

function scheduleWorldTick(view) {
  if (!view.isRunning || activeView !== view) return;
  view.timerId = setTimeout(() => {
    if (!view.isRunning || activeView !== view) return;
    view.session.step();
    updateWorldDataBuffers(view, false);
    if (view.setWorldInfo) {
      view.setWorldInfo(buildWorldInfo(view))();
    }
    if (view.isRunning) {
      scheduleWorldTick(view);
    }
  }, 120);
}

export const mountWorldView =
  (gridSize) =>
  (dayHours) =>
  (tiltDeg) =>
  (landPct) =>
  (atmDt) =>
  (lifeDt) =>
  (peopleDt) =>
  (moonCount) =>
  (moonMassTenths) =>
  (moonDistRadii) =>
  (meteorFreqTenths) =>
  (aspect) =>
  (setWorldInfo) =>
  () => {
    const canvas = document.getElementById("sphere-canvas");
    if (!canvas) {
      return () => {};
    }

    const view = createBaseSphereView(canvas, false);
    view.session = new WasmWorldSession(
      gridSize,
      dayHours,
      tiltDeg,
      landPct,
      atmDt,
      lifeDt,
      peopleDt,
      moonCount,
      moonMassTenths / 10.0,
      moonDistRadii,
      meteorFreqTenths / 10.0
    );
    view.inspectorMode = "world";
    view.aspect = aspect;
    view.selectedTileIdx = -1;
    view.selectedStateId = -1;
    view.selectedLangId = -1;
    view.selectedNationId = -1;
    view.stateMapPolygons = [];
    view.isRunning = false;
    view.timerId = null;
    view.setWorldInfo = setWorldInfo;

    const raycaster = new THREE.Raycaster();
    const pointerNdc = new THREE.Vector2();

    const handleCanvasClick = (ev) => {
      if (!view.session) return;
      const rect = canvas.getBoundingClientRect();
      const u = (ev.clientX - rect.left) / rect.width;
      const v = (ev.clientY - rect.top) / rect.height;

      pointerNdc.set(2.0 * u - 1.0, 1.0 - 2.0 * v);
      view.mesh.updateMatrixWorld(true);
      view.camera.updateMatrixWorld(true);
      raycaster.setFromCamera(pointerNdc, view.camera);
      const hits = raycaster.intersectObject(view.mesh, false);
      if (hits.length > 0 && hits[0].face && view.vertexTileIds) {
        view.selectedTileIdx = view.vertexTileIds[hits[0].face.a];
        view.selectedStateId = -1;
        updateWorldDataBuffers(view, false);
        view.setWorldInfo(buildWorldInfo(view))();
      }
    };

    canvas.addEventListener("click", handleCanvasClick);

    activeView = view;
    updateWorldDataBuffers(view, true);
    setWorldInfo(buildWorldInfo(view))();

    return () => {
      canvas.removeEventListener("click", handleCanvasClick);
      stopWorldLoop(view);
      if (activeView === view) {
        activeView = null;
      }
      view.dispose();
      view.session.free();
    };
  };

export const createWorld =
  (gridSize) =>
  (dayHours) =>
  (tiltDeg) =>
  (landPct) =>
  (atmDt) =>
  (lifeDt) =>
  (peopleDt) =>
  (moonCount) =>
  (moonMassTenths) =>
  (moonDistRadii) =>
  (meteorFreqTenths) =>
  (randomize) =>
  (setWorldInfo) =>
  () => {
    if (!activeView || !activeView.session) return;
    stopWorldLoop(activeView);
    activeView.session.create_world(
      gridSize,
      dayHours,
      tiltDeg,
      landPct,
      atmDt,
      lifeDt,
      peopleDt,
      moonCount,
      moonMassTenths / 10.0,
      moonDistRadii,
      meteorFreqTenths / 10.0,
      randomize
    );
    activeView.selectedTileIdx = -1;
    activeView.selectedStateId = -1;
    activeView.selectedLangId = -1;
    activeView.selectedNationId = -1;
    updateWorldDataBuffers(activeView, true);
    setWorldInfo(buildWorldInfo(activeView))();
  };

export const stepWorld = (setWorldInfo) => () => {
  if (!activeView || !activeView.session || activeView.isRunning) return;
  activeView.session.step();
  updateWorldDataBuffers(activeView, false);
  setWorldInfo(buildWorldInfo(activeView))();
};

export const advanceWorldStage = (setWorldInfo) => () => {
  if (!activeView || !activeView.session) return;
  activeView.session.advance_stage();
  activeView.selectedTileIdx = -1;
  activeView.selectedStateId = -1;
  activeView.selectedLangId = -1;
  activeView.selectedNationId = -1;
  updateWorldDataBuffers(activeView, false);
  setWorldInfo(buildWorldInfo(activeView))();
};

export const toggleWorldRunning = (setWorldInfo) => () => {
  if (!activeView || !activeView.session) return;
  if (activeView.isRunning) {
    stopWorldLoop(activeView);
    setWorldInfo(buildWorldInfo(activeView))();
  } else {
    activeView.isRunning = true;
    setWorldInfo(buildWorldInfo(activeView))();
    scheduleWorldTick(activeView);
  }
};

export const setWorldAspect = (aspect) => () => {
  if (!activeView || !activeView.session) return;
  activeView.aspect = aspect;
  updateWorldDataBuffers(activeView, false);
};

export const setWorldSunAndAtmosphere =
  (starFilter) =>
  (atmosphereId) =>
  (setWorldInfo) =>
  () => {
    if (!activeView || !activeView.session) return;
    activeView.session.set_sun_and_atmosphere(starFilter, atmosphereId);
    updateWorldDataBuffers(activeView, false);
    setWorldInfo(buildWorldInfo(activeView))();
  };

export const saveWorldPlanet = (setWorldInfo) => () => {
  if (!activeView || !activeView.session) return;
  const bytes = activeView.session.encode_snapshot();
  if (!bytes || bytes.length === 0) return;
  storeSavedSnapshotBytes(bytes);
  activeClimateSnapshotBytes = bytes;
  triggerBinaryDownload("tec11-world.tec11", bytes);
  setWorldInfo(buildWorldInfo(activeView))();
};

export const loadSavedPlanetIntoWorld = (setWorldInfo) => () => {
  if (!activeView || !activeView.session) return;
  const saved = getSavedSnapshotBytes();
  if (!saved) return;
  stopWorldLoop(activeView);
  if (activeView.session.load_snapshot_bytes(saved)) {
    activeClimateSnapshotBytes = saved;
    activeView.selectedTileIdx = -1;
    activeView.selectedStateId = -1;
    activeView.selectedLangId = -1;
    activeView.selectedNationId = -1;
    updateWorldDataBuffers(activeView, true);
    setWorldInfo(buildWorldInfo(activeView))();
  }
};

export const loadFilePlanetIntoWorld = (setWorldInfo) => () => {
  if (!activeView || !activeView.session) return;
  pickBinarySnapshotFile((bytes) => {
    if (!activeView || !activeView.session) return;
    stopWorldLoop(activeView);
    if (activeView.session.load_snapshot_bytes(bytes)) {
      storeSavedSnapshotBytes(bytes);
      activeClimateSnapshotBytes = bytes;
      activeView.selectedTileIdx = -1;
      activeView.selectedStateId = -1;
      activeView.selectedLangId = -1;
      activeView.selectedNationId = -1;
      updateWorldDataBuffers(activeView, true);
      setWorldInfo(buildWorldInfo(activeView))();
    }
  });
};

// ============================================================================
// Mode 15: Subdivided Terrain & Fine River Routing (`terrainsimulation.py`, `terraindisplay.py`, `terrainpresenter.py`)
// ============================================================================

function computeTerrainVertexColors(view) {
  const { vertexTileIds, tileColors } = view;
  const vertexCount = vertexTileIds.length;
  const colors = new Float32Array(vertexCount * 3);
  for (let v = 0; v < vertexCount; v++) {
    const t = vertexTileIds[v];
    colors[v * 3] = tileColors[t * 3];
    colors[v * 3 + 1] = tileColors[t * 3 + 1];
    colors[v * 3 + 2] = tileColors[t * 3 + 2];
  }
  return colors;
}

function updateTerrainDataBuffers(view, rebuildGeometry = false) {
  const { session, geometry } = view;
  if (rebuildGeometry) {
    const positions = session.mesh_positions();
    const normals = session.mesh_normals();
    view.vertexTileIds = session.vertex_tile_ids();
    geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
    geometry.setAttribute("normal", new THREE.Float32BufferAttribute(normals, 3));
    geometry.computeBoundingSphere();
  }
  view.tileColors = session.tile_colors_rgb(
    Boolean(view.showRivers),
    view.selectedFaceIdx
  );
  const colors = computeTerrainVertexColors(view);
  geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
  view.paint();
}

function buildTerrainInfo(view) {
  const s = view.session;
  const planetName = s.is_custom_planet()
    ? `Saved Planet (${s.land_percent()}% land)`
    : "Earth (ETOPO2)";
  const selectedReport =
    view.selectedFaceIdx >= 0
      ? s.inspect_face(view.selectedFaceIdx)
      : "Click any subdivided land face or routed river channel on the globe to inspect its 7-octave Simplex noise elevation and coarse tile parentage.";
  scheduleStratigraphySidePaint(view, view.selectedFaceIdx);
  return {
    planetName,
    displayFaceCount: s.display_face_count(),
    fineFaceCount: s.fine_face_count(),
    coarseTileCount: s.coarse_tile_count(),
    riverCount: s.river_count(),
    hasSavedPlanet: getSavedSnapshotBytes() !== null,
    selectedReport,
  };
}

export const mountTerrainView = (showRivers) => (setTerrainInfo) => () => {
  const canvas = document.getElementById("sphere-canvas");
  if (!canvas) {
    return () => {};
  }

  const view = createBaseSphereView(canvas, false);
  view.session = new WasmTerrainSession();
  view.showRivers = showRivers;
  view.selectedFaceIdx = -1;
  view.setTerrainInfo = setTerrainInfo;

  if (activeClimateSnapshotBytes) {
    view.session.load_snapshot_bytes(activeClimateSnapshotBytes);
  }

  const raycaster = new THREE.Raycaster();
  const pointerNdc = new THREE.Vector2();

  const handleCanvasClick = (ev) => {
    if (!view.session) return;
    const rect = canvas.getBoundingClientRect();
    const u = (ev.clientX - rect.left) / rect.width;
    const v = (ev.clientY - rect.top) / rect.height;

    pointerNdc.set(2.0 * u - 1.0, 1.0 - 2.0 * v);
    view.mesh.updateMatrixWorld(true);
    view.camera.updateMatrixWorld(true);
    raycaster.setFromCamera(pointerNdc, view.camera);
    const hits = raycaster.intersectObject(view.mesh, false);
    if (hits.length > 0 && hits[0].face && view.vertexTileIds) {
      view.selectedFaceIdx = view.vertexTileIds[hits[0].face.a];
      updateTerrainDataBuffers(view, false);
      view.setTerrainInfo(buildTerrainInfo(view))();
    }
  };

  canvas.addEventListener("click", handleCanvasClick);

  activeView = view;
  updateTerrainDataBuffers(view, true);
  setTerrainInfo(buildTerrainInfo(view))();

  return () => {
    canvas.removeEventListener("click", handleCanvasClick);
    if (activeView === view) {
      activeView = null;
    }
    view.dispose();
    view.session.free();
  };
};

export const setTerrainShowRivers = (showRivers) => () => {
  if (!activeView || !activeView.session) return;
  activeView.showRivers = showRivers;
  updateTerrainDataBuffers(activeView, false);
};

export const saveTerrainPlanet = (setTerrainInfo) => () => {
  if (!activeView || !activeView.session) return;
  const bytes = activeView.session.encode_snapshot();
  if (!bytes || bytes.length === 0) return;
  storeSavedSnapshotBytes(bytes);
  activeClimateSnapshotBytes = bytes;
  triggerBinaryDownload("tec11-terrain.tec11", bytes);
  setTerrainInfo(buildTerrainInfo(activeView))();
};

export const loadEarthIntoTerrain = (setTerrainInfo) => () => {
  if (!activeView || !activeView.session) return;
  activeClimateSnapshotBytes = null;
  activeView.session.load_earth();
  activeView.selectedFaceIdx = -1;
  updateTerrainDataBuffers(activeView, true);
  setTerrainInfo(buildTerrainInfo(activeView))();
};

export const loadSavedPlanetIntoTerrain = (setTerrainInfo) => () => {
  if (!activeView || !activeView.session) return;
  const saved = getSavedSnapshotBytes();
  if (!saved) return;
  if (activeView.session.load_snapshot_bytes(saved)) {
    activeClimateSnapshotBytes = saved;
    activeView.selectedFaceIdx = -1;
    updateTerrainDataBuffers(activeView, true);
    setTerrainInfo(buildTerrainInfo(activeView))();
  }
};

export const loadFilePlanetIntoTerrain = (setTerrainInfo) => () => {
  if (!activeView || !activeView.session) return;
  pickBinarySnapshotFile((bytes) => {
    if (!activeView || !activeView.session) return;
    if (activeView.session.load_snapshot_bytes(bytes)) {
      storeSavedSnapshotBytes(bytes);
      activeClimateSnapshotBytes = bytes;
      activeView.selectedFaceIdx = -1;
      updateTerrainDataBuffers(activeView, true);
      setTerrainInfo(buildTerrainInfo(activeView))();
    }
  });
};

// ============================================================================
// Mode 16: Moons, Harmonic Orbits & Tidal Erosion Sandbox (`MoonScreen`)
// ============================================================================

function computeMoonVertexColors(view) {
  const { vertexTileIds, tileColors } = view;
  const vertexCount = vertexTileIds.length;
  const colors = new Float32Array(vertexCount * 3);
  for (let v = 0; v < vertexCount; v++) {
    const t = vertexTileIds[v];
    colors[v * 3] = tileColors[t * 3];
    colors[v * 3 + 1] = tileColors[t * 3 + 1];
    colors[v * 3 + 2] = tileColors[t * 3 + 2];
  }
  return colors;
}

function updateMoonDataBuffers(view, rebuildGeometry = false) {
  const { session, geometry } = view;
  if (rebuildGeometry) {
    const positions = session.mesh_positions();
    const normals = session.mesh_normals();
    view.vertexTileIds = session.vertex_tile_ids();
    geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
    geometry.setAttribute("normal", new THREE.Float32BufferAttribute(normals, 3));
    geometry.computeBoundingSphere();
  }
  view.tileColors = session.tile_colors_rgb(
    view.aspect,
    Boolean(view.showTidalScour),
    Boolean(view.showFluvial),
    Boolean(view.showDeposition),
    view.selectedTileIdx
  );
  const colors = computeMoonVertexColors(view);
  geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
  view.paint();
}

function buildMoonInfo(view) {
  const s = view.session;
  const planetName = s.is_custom_planet()
    ? `Loaded Planet (${s.loaded_geoticks() * 5} Myr, ${s.loaded_land_percent()}% land)`
    : "Earth (ETOPO2)";
  const selectedReport =
    view.selectedTileIdx >= 0
      ? s.inspect_tile(view.selectedTileIdx)
      : "Click any coastal, ocean, or inland tile on the globe to inspect local tidal range (m), tidal scour vs. rainfall erosion rates, shelf deposition, and rock strata.";
  scheduleStratigraphySidePaint(view, view.selectedTileIdx);
  return {
    planetName,
    tileCount: s.tile_count(),
    steps: s.steps(),
    harmonicSummary: s.harmonic_chain_summary(),
    tidalSharePctText: `${s.coastal_tidal_erosion_share_pct().toFixed(1)}%`,
    meanCoastalScourText: `${s.mean_coastal_tidal_scour().toFixed(4)} km/tick`,
    maxTidalRangeText: `${s.max_tidal_range_m().toFixed(1)} m`,
    hasSavedPlanet: getSavedSnapshotBytes() !== null,
    selectedReport,
  };
}

export const mountMoonView =
  (moonCount) =>
  (moonMassTenths) =>
  (moonDistRadii) =>
  (epochPct) =>
  (aspect) =>
  (showTidalScour) =>
  (showFluvial) =>
  (showDeposition) =>
  (setMoonInfo) =>
  () => {
    const canvas = document.getElementById("sphere-canvas");
    if (!canvas) {
      return () => {};
    }

    const view = createBaseSphereView(canvas, false);
    view.session = new WasmMoonSession(
      moonCount,
      moonMassTenths / 10.0,
      moonDistRadii,
      epochPct
    );
    view.aspect = aspect;
    view.showTidalScour = showTidalScour;
    view.showFluvial = showFluvial;
    view.showDeposition = showDeposition;
    view.selectedTileIdx = -1;
    view.setMoonInfo = setMoonInfo;

    if (activeClimateSnapshotBytes) {
      view.session.load_snapshot_bytes(activeClimateSnapshotBytes);
      view.session.set_moon_params(
        moonCount,
        moonMassTenths / 10.0,
        moonDistRadii,
        epochPct
      );
    }

    const raycaster = new THREE.Raycaster();
    const pointerNdc = new THREE.Vector2();

    const handleCanvasClick = (ev) => {
      if (!view.session) return;
      const rect = canvas.getBoundingClientRect();
      const u = (ev.clientX - rect.left) / rect.width;
      const v = (ev.clientY - rect.top) / rect.height;

      pointerNdc.set(2.0 * u - 1.0, 1.0 - 2.0 * v);
      view.mesh.updateMatrixWorld(true);
      view.camera.updateMatrixWorld(true);
      raycaster.setFromCamera(pointerNdc, view.camera);
      const hits = raycaster.intersectObject(view.mesh, false);
      if (hits.length > 0 && hits[0].face && view.vertexTileIds) {
        view.selectedTileIdx = view.vertexTileIds[hits[0].face.a];
        updateMoonDataBuffers(view, false);
        view.setMoonInfo(buildMoonInfo(view))();
      }
    };

    canvas.addEventListener("click", handleCanvasClick);

    activeView = view;
    updateMoonDataBuffers(view, true);
    setMoonInfo(buildMoonInfo(view))();

    return () => {
      canvas.removeEventListener("click", handleCanvasClick);
      if (activeView === view) {
        activeView = null;
      }
      view.dispose();
      view.session.free();
    };
  };

export const setMoonParams =
  (moonCount) =>
  (moonMassTenths) =>
  (moonDistRadii) =>
  (epochPct) =>
  (setMoonInfo) =>
  () => {
    if (!activeView || !activeView.session) return;
    activeView.session.set_moon_params(
      moonCount,
      moonMassTenths / 10.0,
      moonDistRadii,
      epochPct
    );
    updateMoonDataBuffers(activeView, false);
    setMoonInfo(buildMoonInfo(activeView))();
  };

export const setMoonDisplayOptions =
  (aspect) =>
  (showTidalScour) =>
  (showFluvial) =>
  (showDeposition) =>
  () => {
    if (!activeView || !activeView.session) return;
    activeView.aspect = aspect;
    activeView.showTidalScour = showTidalScour;
    activeView.showFluvial = showFluvial;
    activeView.showDeposition = showDeposition;
    updateMoonDataBuffers(activeView, false);
  };

export const stepMoonErode = (setMoonInfo) => () => {
  if (!activeView || !activeView.session) return;
  activeView.session.step_erode();
  updateMoonDataBuffers(activeView, false);
  setMoonInfo(buildMoonInfo(activeView))();
};

export const resetMoonLayers = (setMoonInfo) => () => {
  if (!activeView || !activeView.session) return;
  activeView.session.reset_layers();
  updateMoonDataBuffers(activeView, false);
  setMoonInfo(buildMoonInfo(activeView))();
};

export const loadEarthIntoMoon = (setMoonInfo) => () => {
  if (!activeView || !activeView.session) return;
  activeClimateSnapshotBytes = null;
  activeView.session.load_earth();
  activeView.selectedTileIdx = -1;
  updateMoonDataBuffers(activeView, true);
  setMoonInfo(buildMoonInfo(activeView))();
};

export const loadSavedPlanetIntoMoon = (setMoonInfo) => () => {
  if (!activeView || !activeView.session) return;
  const saved = getSavedSnapshotBytes();
  if (!saved) return;
  if (activeView.session.load_snapshot_bytes(saved)) {
    activeClimateSnapshotBytes = saved;
    activeView.selectedTileIdx = -1;
    updateMoonDataBuffers(activeView, true);
    setMoonInfo(buildMoonInfo(activeView))();
  }
};

export const loadFilePlanetIntoMoon = (setMoonInfo) => () => {
  if (!activeView || !activeView.session) return;
  pickBinarySnapshotFile((bytes) => {
    if (!activeView || !activeView.session) return;
    if (activeView.session.load_snapshot_bytes(bytes)) {
      storeSavedSnapshotBytes(bytes);
      activeClimateSnapshotBytes = bytes;
      activeView.selectedTileIdx = -1;
      updateMoonDataBuffers(activeView, true);
      setMoonInfo(buildMoonInfo(activeView))();
    }
  });
};

// ============================================================================
// Mode 17: Meteors, Crater Excavation & Impact Winter Sandbox (`MeteorScreen`)
// ============================================================================

function computeMeteorVertexColors(view) {
  const { vertexTileIds, tileColors } = view;
  const vertexCount = vertexTileIds.length;
  const colors = new Float32Array(vertexCount * 3);
  for (let v = 0; v < vertexCount; v++) {
    const t = vertexTileIds[v];
    colors[v * 3] = tileColors[t * 3];
    colors[v * 3 + 1] = tileColors[t * 3 + 1];
    colors[v * 3 + 2] = tileColors[t * 3 + 2];
  }
  return colors;
}

function updateMeteorDataBuffers(view, rebuildGeometry = false) {
  const { session, geometry } = view;
  if (rebuildGeometry) {
    const positions = session.mesh_positions();
    const normals = session.mesh_normals();
    view.vertexTileIds = session.vertex_tile_ids();
    geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
    geometry.setAttribute("normal", new THREE.Float32BufferAttribute(normals, 3));
    geometry.computeBoundingSphere();
  }
  view.tileColors = session.tile_colors_rgb(
    view.aspect,
    Boolean(view.showEjecta),
    Boolean(view.showBlast),
    view.selectedTileIdx
  );
  const colors = computeMeteorVertexColors(view);
  geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
  view.paint();
}

function buildMeteorInfo(view) {
  const s = view.session;
  const planetName = s.is_custom_planet()
    ? `Loaded Planet (${s.loaded_geoticks() * 5} Myr, ${s.loaded_land_percent()}% land)`
    : "Earth (ETOPO2)";
  const selectedReport =
    view.selectedTileIdx >= 0
      ? s.inspect_tile(view.selectedTileIdx)
      : "Click any tile on the globe to target it for a meteor strike and inspect crater excavation, ejecta breccia, impact winter cooling, and rock strata.";
  scheduleStratigraphySidePaint(view, view.selectedTileIdx);
  return {
    planetName,
    tileCount: s.tile_count(),
    impacts: s.impact_count(),
    recoverySteps: s.recovery_steps(),
    impactSummary: s.impact_summary(),
    coolingText: `-${s.current_cooling_c().toFixed(1)} °C`,
    biospherePctText: `${s.biosphere_survival_pct().toFixed(0)}%`,
    hasSavedPlanet: getSavedSnapshotBytes() !== null,
    selectedReport,
  };
}

export const mountMeteorView =
  (diameterKm) =>
  (velocityKmS) =>
  (densityKgM3) =>
  (aspect) =>
  (showEjecta) =>
  (showBlast) =>
  (setMeteorInfo) =>
  () => {
    const canvas = document.getElementById("sphere-canvas");
    if (!canvas) {
      return () => {};
    }

    const view = createBaseSphereView(canvas, false);
    view.session = new WasmMeteorSession(diameterKm, velocityKmS, densityKgM3);
    view.aspect = aspect;
    view.showEjecta = showEjecta;
    view.showBlast = showBlast;
    view.selectedTileIdx = -1;
    view.setMeteorInfo = setMeteorInfo;

    if (activeClimateSnapshotBytes) {
      view.session.load_snapshot_bytes(activeClimateSnapshotBytes);
      view.session.set_impactor_params(diameterKm, velocityKmS, densityKgM3);
    }

    const raycaster = new THREE.Raycaster();
    const pointerNdc = new THREE.Vector2();

    const handleCanvasClick = (ev) => {
      if (!view.session) return;
      const rect = canvas.getBoundingClientRect();
      const u = (ev.clientX - rect.left) / rect.width;
      const v = (ev.clientY - rect.top) / rect.height;

      pointerNdc.set(2.0 * u - 1.0, 1.0 - 2.0 * v);
      view.mesh.updateMatrixWorld(true);
      view.camera.updateMatrixWorld(true);
      raycaster.setFromCamera(pointerNdc, view.camera);
      const hits = raycaster.intersectObject(view.mesh, false);
      if (hits.length > 0 && hits[0].face && view.vertexTileIds) {
        view.selectedTileIdx = view.vertexTileIds[hits[0].face.a];
        updateMeteorDataBuffers(view, false);
        view.setMeteorInfo(buildMeteorInfo(view))();
      }
    };

    canvas.addEventListener("click", handleCanvasClick);

    activeView = view;
    updateMeteorDataBuffers(view, true);
    setMeteorInfo(buildMeteorInfo(view))();

    return () => {
      canvas.removeEventListener("click", handleCanvasClick);
      if (activeView === view) {
        activeView = null;
      }
      view.dispose();
      view.session.free();
    };
  };

export const setMeteorParams =
  (diameterKm) =>
  (velocityKmS) =>
  (densityKgM3) =>
  (setMeteorInfo) =>
  () => {
    if (!activeView || !activeView.session) return;
    activeView.session.set_impactor_params(diameterKm, velocityKmS, densityKgM3);
    setMeteorInfo(buildMeteorInfo(activeView))();
  };

export const setMeteorDisplayOptions =
  (aspect) =>
  (showEjecta) =>
  (showBlast) =>
  () => {
    if (!activeView || !activeView.session) return;
    activeView.aspect = aspect;
    activeView.showEjecta = showEjecta;
    activeView.showBlast = showBlast;
    updateMeteorDataBuffers(activeView, false);
  };

export const strikeMeteorTile = (onlyRandom) => (setMeteorInfo) => () => {
  if (!activeView || !activeView.session) return;
  const target = onlyRandom ? -1 : activeView.selectedTileIdx;
  const struck = activeView.session.strike_tile(target);
  activeView.selectedTileIdx = struck;
  updateMeteorDataBuffers(activeView, false);
  setMeteorInfo(buildMeteorInfo(activeView))();
};

export const strikeMeteorBarrage = (setMeteorInfo) => () => {
  if (!activeView || !activeView.session) return;
  const struck = activeView.session.strike_barrage(8);
  activeView.selectedTileIdx = struck;
  updateMeteorDataBuffers(activeView, false);
  setMeteorInfo(buildMeteorInfo(activeView))();
};

export const stepMeteorRecover = (setMeteorInfo) => () => {
  if (!activeView || !activeView.session) return;
  activeView.session.step_recover_and_erode();
  updateMeteorDataBuffers(activeView, false);
  setMeteorInfo(buildMeteorInfo(activeView))();
};

export const resetMeteorPlanet = (setMeteorInfo) => () => {
  if (!activeView || !activeView.session) return;
  activeView.session.reset_planet();
  updateMeteorDataBuffers(activeView, false);
  setMeteorInfo(buildMeteorInfo(activeView))();
};

export const loadEarthIntoMeteor = (setMeteorInfo) => () => {
  if (!activeView || !activeView.session) return;
  activeClimateSnapshotBytes = null;
  activeView.session.load_earth();
  activeView.selectedTileIdx = -1;
  updateMeteorDataBuffers(activeView, true);
  setMeteorInfo(buildMeteorInfo(activeView))();
};

export const loadSavedPlanetIntoMeteor = (setMeteorInfo) => () => {
  if (!activeView || !activeView.session) return;
  const saved = getSavedSnapshotBytes();
  if (!saved) return;
  if (activeView.session.load_snapshot_bytes(saved)) {
    activeClimateSnapshotBytes = saved;
    activeView.selectedTileIdx = -1;
    updateMeteorDataBuffers(activeView, true);
    setMeteorInfo(buildMeteorInfo(activeView))();
  }
};

export const loadFilePlanetIntoMeteor = (setMeteorInfo) => () => {
  if (!activeView || !activeView.session) return;
  pickBinarySnapshotFile((bytes) => {
    if (!activeView || !activeView.session) return;
    if (activeView.session.load_snapshot_bytes(bytes)) {
      storeSavedSnapshotBytes(bytes);
      activeClimateSnapshotBytes = bytes;
      activeView.selectedTileIdx = -1;
      updateMeteorDataBuffers(activeView, true);
      setMeteorInfo(buildMeteorInfo(activeView))();
    }
  });
};

// ============================================================================
// Mode 18: Sun, Plausible Galaxy & Celestial Sphere Star Map (`SunScreen`)
// ============================================================================

function computeSunVertexColors(view) {
  const { vertexTileIds, tileColors } = view;
  const vertexCount = vertexTileIds.length;
  const colors = new Float32Array(vertexCount * 3);
  for (let v = 0; v < vertexCount; v++) {
    const t = vertexTileIds[v];
    colors[v * 3] = tileColors[t * 3];
    colors[v * 3 + 1] = tileColors[t * 3 + 1];
    colors[v * 3 + 2] = tileColors[t * 3 + 2];
  }
  return colors;
}

function drawSkyPlants2dPicture(view) {
  const { session, pic2dCanvas } = view;
  if (!pic2dCanvas || !session) return;
  const ctx = pic2dCanvas.getContext("2d");
  if (!ctx) return;

  const W = pic2dCanvas.width;
  const H = pic2dCanvas.height;

  const sunRgb = session.sun_rgb();
  const skyZen = session.sky_zenith_rgb();
  const skyHor = session.sky_horizon_rgb();
  const sunsetRgb = session.sunset_rgb();
  const twiRgb = session.twilight_rgb();
  const plantFol = session.plant_foliage_rgb();
  const plantHi = session.plant_highlight_rgb();

  const toRgb = (arr, alpha = 1.0) => {
    const r = Math.max(0, Math.min(255, Math.round((arr[0] || 0) * 255)));
    const g = Math.max(0, Math.min(255, Math.round((arr[1] || 0) * 255)));
    const b = Math.max(0, Math.min(255, Math.round((arr[2] || 0) * 255)));
    return alpha < 1.0 ? `rgba(${r}, ${g}, ${b}, ${alpha})` : `rgb(${r}, ${g}, ${b})`;
  };
  const mixRgb = (a, b, t, alpha = 1.0) => {
    const r = Math.max(0, Math.min(255, Math.round(((a[0] || 0) * (1 - t) + (b[0] || 0) * t) * 255)));
    const g = Math.max(0, Math.min(255, Math.round(((a[1] || 0) * (1 - t) + (b[1] || 0) * t) * 255)));
    const bl = Math.max(0, Math.min(255, Math.round(((a[2] || 0) * (1 - t) + (b[2] || 0) * t) * 255)));
    return alpha < 1.0 ? `rgba(${r}, ${g}, ${bl}, ${alpha})` : `rgb(${r}, ${g}, ${bl})`;
  };

  const horizonY = 278;

  // 1. Daytime Sky Backdrop (full width, zenith -> daytime horizon):
  const dayGrad = ctx.createLinearGradient(0, 0, 0, horizonY);
  dayGrad.addColorStop(0.0, toRgb(skyZen));
  dayGrad.addColorStop(0.58, mixRgb(skyZen, skyHor, 0.55));
  dayGrad.addColorStop(1.0, toRgb(skyHor));
  ctx.fillStyle = dayGrad;
  ctx.fillRect(0, 0, W, horizonY + 12);

  // 2. Smooth Sunset & Twilight Zone on the Right Half (`x = 170..460`):
  //    Draw vertical strips that blend from 0% sunset at x=170 to 100% sunset/twilight at x=330..460:
  for (let x = 165; x < W; x += 3) {
    const w = Math.min(1.0, Math.pow(Math.max(0, (x - 165) / 185), 1.25));
    const colGrad = ctx.createLinearGradient(0, 0, 0, horizonY + 8);
    colGrad.addColorStop(0.0, toRgb(twiRgb, w * 0.92));
    colGrad.addColorStop(0.50, mixRgb(twiRgb, sunsetRgb, 0.52, w * 0.96));
    colGrad.addColorStop(1.0, toRgb(sunsetRgb, w));
    ctx.fillStyle = colGrad;
    ctx.fillRect(x, 0, 4, horizonY + 8);
  }

  // 3. Daytime Home Sun (upper-left sky) scaled by true solar radius:
  const rSolar = session.sun_radius_solar();
  const sunRadPx = Math.max(11, Math.min(34, 20 * Math.sqrt(rSolar)));
  const sunX = 112;
  const sunY = 82;
  const corona = ctx.createRadialGradient(sunX, sunY, sunRadPx * 0.3, sunX, sunY, sunRadPx * 3.4);
  corona.addColorStop(0.0, toRgb(sunRgb, 0.95));
  corona.addColorStop(0.32, toRgb(sunRgb, 0.45));
  corona.addColorStop(1.0, toRgb(sunRgb, 0.0));
  ctx.fillStyle = corona;
  ctx.beginPath();
  ctx.arc(sunX, sunY, sunRadPx * 3.4, 0, Math.PI * 2);
  ctx.fill();

  ctx.fillStyle = toRgb(sunRgb);
  ctx.beginPath();
  ctx.arc(sunX, sunY, sunRadPx, 0, Math.PI * 2);
  ctx.fill();

  // 4. Setting Sun on the Right Horizon (`x = 366, y = horizonY - 4`):
  const setX = 366;
  const setY = horizonY - 2;
  const setGlow = ctx.createRadialGradient(setX, setY, sunRadPx * 0.2, setX, setY, sunRadPx * 4.2);
  setGlow.addColorStop(0.0, mixRgb(sunRgb, sunsetRgb, 0.45, 0.95));
  setGlow.addColorStop(0.38, toRgb(sunsetRgb, 0.55));
  setGlow.addColorStop(1.0, toRgb(sunsetRgb, 0.0));
  ctx.fillStyle = setGlow;
  ctx.beginPath();
  ctx.arc(setX, setY, sunRadPx * 4.2, 0, Math.PI * 2);
  ctx.fill();

  ctx.fillStyle = mixRgb(sunRgb, sunsetRgb, 0.58);
  ctx.beginPath();
  ctx.arc(setX, setY, sunRadPx * 0.95, Math.PI, 0, false);
  ctx.fill();

  // 5. Atmospheric Clouds (Daytime cumulus on left + Sunset-lit stratus on right):
  const drawCloud = (cx, cy, rx, ry, col) => {
    ctx.fillStyle = col;
    ctx.beginPath();
    ctx.ellipse(cx, cy, rx, ry, 0, 0, Math.PI * 2);
    ctx.ellipse(cx - rx * 0.55, cy + ry * 0.18, rx * 0.65, ry * 0.75, 0, 0, Math.PI * 2);
    ctx.ellipse(cx + rx * 0.58, cy + ry * 0.15, rx * 0.70, ry * 0.78, 0, 0, Math.PI * 2);
    ctx.fill();
  };
  drawCloud(72, 165, 38, 10, mixRgb(sunRgb, skyHor, 0.35, 0.55));
  drawCloud(195, 138, 44, 11, mixRgb(sunRgb, skyHor, 0.40, 0.50));
  drawCloud(310, 162, 48, 8, mixRgb(sunsetRgb, twiRgb, 0.30, 0.62));
  drawCloud(405, 195, 42, 7, mixRgb(sunsetRgb, sunRgb, 0.35, 0.68));

  // 6. Distant Mountain Range along the Horizon (with atmospheric perspective haze):
  ctx.fillStyle = mixRgb(plantFol, skyHor, 0.55);
  ctx.beginPath();
  ctx.moveTo(0, horizonY);
  const mtnPts = [
    [0, 246], [38, 224], [76, 252], [118, 214], [162, 248],
    [204, 232], [248, 258], [292, 238], [335, 266], [460, 276],
  ];
  for (const [mx, my] of mtnPts) {
    ctx.lineTo(mx, my);
  }
  ctx.lineTo(W, horizonY);
  ctx.closePath();
  ctx.fill();

  // 7. Coastal Ocean / Bay (`y = horizonY .. 365`) reflecting Sky & Sunset:
  const oceanGrad = ctx.createLinearGradient(0, horizonY, W, 365);
  oceanGrad.addColorStop(0.0, mixRgb(skyZen, [0.04, 0.12, 0.24], 0.38));
  oceanGrad.addColorStop(0.52, mixRgb(skyHor, [0.05, 0.15, 0.28], 0.35));
  oceanGrad.addColorStop(1.0, mixRgb(sunsetRgb, twiRgb, 0.38));
  ctx.fillStyle = oceanGrad;
  ctx.fillRect(0, horizonY, W, 92);

  // Shimmering sunset water reflection path below the setting sun (`x ≈ 366`):
  for (let ry = horizonY + 4; ry < 358; ry += 5) {
    const t = (ry - horizonY) / 80.0;
    const rw = 16 + t * 42;
    const rx = setX - rw * 0.5 + Math.sin(ry * 0.7) * 4.0;
    ctx.fillStyle = mixRgb(sunsetRgb, sunRgb, 0.45, 0.72 * (1.0 - t * 0.5));
    ctx.fillRect(rx, ry, rw, 2.5);
  }

  // 8. Foreground Rolling Hills & Photosynthetic Plant Foliage (`y = 292 .. 460`):
  // Farther hill slope (left-to-center):
  const hillBackGrad = ctx.createLinearGradient(0, 285, 0, 390);
  hillBackGrad.addColorStop(0.0, mixRgb(plantHi, skyHor, 0.18));
  hillBackGrad.addColorStop(1.0, toRgb(plantFol));
  ctx.fillStyle = hillBackGrad;
  ctx.beginPath();
  ctx.moveTo(0, 294);
  ctx.bezierCurveTo(95, 278, 195, 312, 315, 348);
  ctx.lineTo(315, H);
  ctx.lineTo(0, H);
  ctx.closePath();
  ctx.fill();

  // Midground hill slope (right-to-left):
  const hillMidGrad = ctx.createLinearGradient(0, 325, 0, H);
  hillMidGrad.addColorStop(0.0, mixRgb(plantHi, sunsetRgb, 0.16));
  hillMidGrad.addColorStop(1.0, mixRgb(plantFol, [0.04, 0.04, 0.06], 0.22));
  ctx.fillStyle = hillMidGrad;
  ctx.beginPath();
  ctx.moveTo(W, 336);
  ctx.bezierCurveTo(350, 322, 235, 344, 95, 378);
  ctx.lineTo(95, H);
  ctx.lineTo(W, H);
  ctx.closePath();
  ctx.fill();

  // Foreground lush botanical meadow & canopy ridge (full bottom):
  const hillFrontGrad = ctx.createLinearGradient(0, 355, 0, H);
  hillFrontGrad.addColorStop(0.0, toRgb(plantHi));
  hillFrontGrad.addColorStop(0.55, toRgb(plantFol));
  hillFrontGrad.addColorStop(1.0, mixRgb(plantFol, [0.02, 0.02, 0.04], 0.35));
  ctx.fillStyle = hillFrontGrad;
  ctx.beginPath();
  ctx.moveTo(0, 356);
  ctx.bezierCurveTo(130, 340, 280, 368, W, 354);
  ctx.lineTo(W, H);
  ctx.lineTo(0, H);
  ctx.closePath();
  ctx.fill();

  // 9. Botanical Tree / Flora Canopy Clusters showing Foliage + Sunlit Highlight Pigments:
  const drawFloraCluster = (tx, ty, scale) => {
    // Trunk / stem:
    ctx.fillStyle = "rgba(48, 34, 28, 0.85)";
    ctx.fillRect(tx - 2.2 * scale, ty - 6 * scale, 4.4 * scale, 22 * scale);

    // Shaded lower canopy (`plantFol`):
    ctx.fillStyle = mixRgb(plantFol, [0.02, 0.02, 0.04], 0.20);
    ctx.beginPath();
    ctx.arc(tx, ty - 10 * scale, 14 * scale, 0, Math.PI * 2);
    ctx.arc(tx - 10 * scale, ty - 6 * scale, 10 * scale, 0, Math.PI * 2);
    ctx.arc(tx + 10 * scale, ty - 6 * scale, 10 * scale, 0, Math.PI * 2);
    ctx.fill();

    // Mid-tone main foliage (`plantFol`):
    ctx.fillStyle = toRgb(plantFol);
    ctx.beginPath();
    ctx.arc(tx - 2 * scale, ty - 13 * scale, 12 * scale, 0, Math.PI * 2);
    ctx.arc(tx - 10 * scale, ty - 9 * scale, 8.5 * scale, 0, Math.PI * 2);
    ctx.arc(tx + 8 * scale, ty - 9 * scale, 8.5 * scale, 0, Math.PI * 2);
    ctx.fill();

    // Sunlit upper-left crown highlights (`plantHi`):
    ctx.fillStyle = toRgb(plantHi);
    ctx.beginPath();
    ctx.arc(tx - 4 * scale, ty - 16 * scale, 8.5 * scale, 0, Math.PI * 2);
    ctx.arc(tx - 11 * scale, ty - 11 * scale, 6.0 * scale, 0, Math.PI * 2);
    ctx.arc(tx + 5 * scale, ty - 12 * scale, 5.5 * scale, 0, Math.PI * 2);
    ctx.fill();
  };

  drawFloraCluster(44, 355, 1.35);
  drawFloraCluster(98, 366, 1.05);
  drawFloraCluster(158, 358, 1.22);
  drawFloraCluster(228, 376, 0.95);
  drawFloraCluster(308, 372, 1.15);
  drawFloraCluster(392, 362, 1.28);
  drawFloraCluster(438, 378, 0.92);

  // 10. Clean Caption Badges on the 2D Picture so each region is immediately clear:
  const drawBadge = (bx, by, swatchRgb, label) => {
    ctx.font = "600 11px Inter, system-ui, sans-serif";
    const textW = ctx.measureText(label).width;
    const padW = textW + 28;
    ctx.fillStyle = "rgba(10, 14, 23, 0.72)";
    ctx.strokeStyle = "rgba(255, 255, 255, 0.22)";
    ctx.lineWidth = 1;
    ctx.beginPath();
    if (ctx.roundRect) {
      ctx.roundRect(bx, by, padW, 22, 6);
    } else {
      ctx.rect(bx, by, padW, 22);
    }
    ctx.fill();
    ctx.stroke();

    ctx.fillStyle = toRgb(swatchRgb);
    ctx.beginPath();
    ctx.arc(bx + 11, by + 11, 5.5, 0, Math.PI * 2);
    ctx.fill();

    ctx.fillStyle = "#f8fafc";
    ctx.fillText(label, bx + 21, by + 15);
  };

  drawBadge(10, 10, skyZen, `Sky: ${session.sky_color_name()}`);
  drawBadge(10, 38, sunRgb, `Sun: ${session.sun_spectral_type()} (${Math.round(session.sun_temp_k())} K)`);
  drawBadge(W - 215, 10, sunsetRgb, `Sunset: ${session.sunset_color_name()}`);
  drawBadge(10, H - 32, plantFol, `Plants: ${session.plant_color_name()}`);
}

function updateSunDataBuffers(view, rebuildGeometry = false) {
  const {
    session,
    geometry,
    mesh,
    starGeometry,
    starMaterial,
    starPoints,
    arrowGeometry,
    arrowMaterial,
    arrowLines,
  } = view;

  if (rebuildGeometry) {
    const positions = session.mesh_positions();
    const normals = session.mesh_normals();
    view.vertexTileIds = session.vertex_tile_ids();
    geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
    geometry.setAttribute("normal", new THREE.Float32BufferAttribute(normals, 3));
    geometry.computeBoundingSphere();
  }

  if (view.aspect === "sky_plants") {
    view.hideSurfaceMesh = true;
    if (view.material) {
      view.material.colorWrite = false;
      view.material.depthWrite = false;
    }
    starPoints.visible = false;
    view.starPointsRaw = null;
    arrowLines.visible = false;
    drawSkyPlants2dPicture(view);
    if (view.pic2dTexture) {
      view.pic2dTexture.needsUpdate = true;
    }
    if (view.quad2dMesh) {
      view.quad2dMesh.visible = true;
    }
    view.paint();
    return;
  }

  if (view.quad2dMesh) {
    view.quad2dMesh.visible = false;
  }

  view.tileColors = session.tile_colors_rgb(
    view.aspect,
    Boolean(view.showGalacticPlane),
    view.selectedTileIdx
  );
  const colors = computeSunVertexColors(view);
  geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));

  const is3dInterior = view.aspect === "galaxy" || view.aspect === "solar_system";
  view.hideSurfaceMesh = is3dInterior;
  if (view.material) {
    view.material.colorWrite = !is3dInterior;
    view.material.depthWrite = !is3dInterior;
  }

  if (view.aspect === "home_sun") {
    const rSolar = session.sun_radius_solar();
    const sunScale = Math.max(0.24, Math.min(1.04, rSolar * 0.68));
    mesh.scale.set(sunScale, sunScale, sunScale);
    const lineSegs = session.overlay_line_segments("home_sun", false, false);
    if (lineSegs && lineSegs.length >= 6) {
      arrowGeometry.setAttribute(
        "position",
        new THREE.Float32BufferAttribute(lineSegs, 3)
      );
      arrowMaterial.color.setHex(0x60a5fa);
      arrowMaterial.opacity = 0.65;
      view.arrowStrokeStyle = "rgba(96, 165, 250, 0.65)";
      view.disableArrowBackfaceCull = true;
      arrowLines.visible = true;
    } else {
      arrowLines.visible = false;
    }
    starPoints.visible = false;
    view.starPointsRaw = null;
  } else {
    mesh.scale.set(1.0, 1.0, 1.0);
    const raw = session.star_points_data(view.aspect);
    view.starPointsRaw = raw;
    const count = Math.floor(raw.length / 7);
    const posArr = new Float32Array(count * 3);
    const colArr = new Float32Array(count * 3);
    const sizeArr = new Float32Array(count);
    for (let i = 0; i < count; i++) {
      const b = i * 7;
      posArr[i * 3] = raw[b];
      posArr[i * 3 + 1] = raw[b + 1];
      posArr[i * 3 + 2] = raw[b + 2];
      colArr[i * 3] = raw[b + 3];
      colArr[i * 3 + 1] = raw[b + 4];
      colArr[i * 3 + 2] = raw[b + 5];
      sizeArr[i] = raw[b + 6];
    }
    starGeometry.setAttribute("position", new THREE.Float32BufferAttribute(posArr, 3));
    starGeometry.setAttribute("customColor", new THREE.Float32BufferAttribute(colArr, 3));
    starGeometry.setAttribute("size", new THREE.Float32BufferAttribute(sizeArr, 1));
    starMaterial.uniforms.cullBackface.value = is3dInterior ? 0.0 : 1.0;
    view.cullStarBackface = !is3dInterior;
    starPoints.visible = true;

    const lineSegs = session.overlay_line_segments(
      view.aspect,
      Boolean(view.showGalacticPlane),
      Boolean(view.showConstellations)
    );
    if (lineSegs && lineSegs.length >= 6) {
      arrowGeometry.setAttribute(
        "position",
        new THREE.Float32BufferAttribute(lineSegs, 3)
      );
      arrowMaterial.color.setHex(is3dInterior ? 0x34d399 : 0x60a5fa);
      arrowMaterial.opacity = is3dInterior ? 0.78 : 0.44;
      view.arrowStrokeStyle = is3dInterior
        ? "rgba(52, 211, 153, 0.78)"
        : "rgba(96, 165, 250, 0.44)";
      view.disableArrowBackfaceCull = is3dInterior;
      arrowLines.visible = true;
    } else {
      arrowLines.visible = false;
    }
  }

  view.paint();
}

function buildSunInfo(view) {
  const s = view.session;
  const defaultPrompt =
    view.aspect === "sky_plants"
      ? "2D Exoplanet Landscape: Left/Top = Daytime Sky & Home Sun, Right Horizon = Sunset & Twilight Glow over the Ocean, Foreground = Rolling Hills & Photosynthetic Plant Foliage. Click the sky, sunset, or plants to inspect their optical physics."
      : view.aspect === "galaxy"
      ? "3D Host Galaxy View: Shows the full 3D galaxy star cloud (tilted 38°), with a bright green 3D crosshair & orbit circle marking your Home Star's galactocentric location. Click or rotate to inspect."
      : view.aspect === "solar_system"
      ? "3D Planetary System View: Central Home Star with concentric 3D planet orbits (green target crosshair marks the simulated Home World; ringed planets show 3D ring halos). Click near any planet to inspect it."
      : view.aspect === "home_sun"
      ? "3D Home Sun Photosphere: Scaled to true solar radius (dashed blue circle = 1.00 R☉ Sol reference) in its true B-V blackbody color. Click the photosphere to inspect."
      : "Click any star on the celestial sphere to inspect its catalog ID, spectral class, apparent & absolute magnitude, and distance in light-years.";
  const selectedReport =
    view.selectedTileIdx >= 0
      ? s.inspect_aspect_tile(view.aspect || "night_sky", view.selectedTileIdx)
      : defaultPrompt;
  return {
    galaxyTypeName: s.galaxy_type_name(),
    vantageName: s.vantage_name(),
    starCount: s.star_count(),
    sunSpectralType: s.sun_spectral_type(),
    sunClassDesc: s.sun_class_description(),
    sunColorCss: rgbFloatToCss(s.sun_rgb(), 255, 255, 119),
    sunBvText: `${s.sun_bv_color() >= 0 ? "+" : ""}${s.sun_bv_color().toFixed(2)}`,
    sunTempText: `${Math.round(s.sun_temp_k())} K`,
    sunRadiusText: `${s.sun_radius_solar().toFixed(2)} R☉`,
    sunMassText: `${s.sun_mass_solar().toFixed(2)} M☉`,
    sunLumText: `${s.sun_luminosity_solar().toFixed(2)} L☉`,
    sunHzText: `${s.sun_habitable_au().toFixed(2)} AU (${Math.round(s.sun_orbital_period_days())}d yr)`,
    atmosphereName: s.atmosphere_name(),
    atmosphereFormula: s.atmosphere_formula(),
    skyColorCss: rgbFloatToCss(s.sky_zenith_rgb(), 41, 130, 240),
    skyHorizonCss: rgbFloatToCss(s.sky_horizon_rgb(), 138, 194, 250),
    skyColorName: s.sky_color_name(),
    skyMechanism: s.sky_mechanism(),
    sunsetColorCss: rgbFloatToCss(s.sunset_rgb(), 250, 97, 36),
    twilightColorCss: rgbFloatToCss(s.twilight_rgb(), 117, 51, 133),
    sunsetColorName: s.sunset_color_name(),
    sunsetMechanism: s.sunset_mechanism(),
    plantColorCss: rgbFloatToCss(s.plant_foliage_rgb(), 36, 138, 41),
    plantHighlightCss: rgbFloatToCss(s.plant_highlight_rgb(), 82, 189, 71),
    plantColorName: s.plant_color_name(),
    plantPigmentDesc: s.plant_pigment_desc(),
    planetsSummaryText: s.planets_summary_text(),
    summaryLine: s.summary_line(),
    selectedReport,
  };
}

export const mountSunView =
  (galaxyTypeId) =>
  (vantageId) =>
  (starFilter) =>
  (aspect) =>
  (showGalacticPlane) =>
  (showConstellations) =>
  (setSunInfo) =>
  () => {
    const canvas = document.getElementById("sphere-canvas");
    if (!canvas) {
      return () => {};
    }

    const view = createBaseSphereView(canvas, false);
    // Use unlit vertex colors in SunScreen so directional Phong lighting does not half-shade
    // the celestial sphere or distort true stellar B-V and atmospheric colors:
    view.material.dispose();
    view.material = new THREE.MeshBasicMaterial({
      side: THREE.FrontSide,
      vertexColors: true,
    });
    view.mesh.material = view.material;
    view.unlit = true;

    // Offscreen 2D canvas & full-viewport 2D quad for the `"sky_plants"` 2D Landscape Picture:
    const pic2dCanvas = document.createElement("canvas");
    pic2dCanvas.width = 460;
    pic2dCanvas.height = 460;
    const pic2dTexture = new THREE.CanvasTexture(pic2dCanvas);
    const quad2dGeom = new THREE.PlaneGeometry(2.2, 2.2);
    const quad2dMat = new THREE.MeshBasicMaterial({
      map: pic2dTexture,
      depthTest: false,
      depthWrite: false,
    });
    const quad2dMesh = new THREE.Mesh(quad2dGeom, quad2dMat);
    quad2dMesh.position.set(0, 0, 1.5);
    quad2dMesh.visible = false;
    view.scene.add(quad2dMesh);
    view.pic2dCanvas = pic2dCanvas;
    view.pic2dTexture = pic2dTexture;
    view.quad2dMesh = quad2dMesh;

    view.session = new WasmSunSession();
    view.session.generate_galaxy_and_sun(galaxyTypeId, vantageId, starFilter);
    view.aspect = aspect;
    view.showGalacticPlane = showGalacticPlane;
    view.showConstellations = showConstellations;
    view.selectedTileIdx = -1;
    view.setSunInfo = setSunInfo;

    const raycaster = new THREE.Raycaster();
    const pointerNdc = new THREE.Vector2();

    const handleCanvasClick = (ev) => {
      if (!view.session) return;
      const rect = canvas.getBoundingClientRect();
      const u = (ev.clientX - rect.left) / rect.width;
      const v = (ev.clientY - rect.top) / rect.height;

      pointerNdc.set(2.0 * u - 1.0, 1.0 - 2.0 * v);
      view.mesh.updateMatrixWorld(true);
      view.camera.updateMatrixWorld(true);
      raycaster.setFromCamera(pointerNdc, view.camera);
      const hits = raycaster.intersectObject(view.mesh, false);
      if (hits.length > 0 && hits[0].face && view.vertexTileIds) {
        view.selectedTileIdx = view.vertexTileIds[hits[0].face.a];
        updateSunDataBuffers(view, false);
        view.setSunInfo(buildSunInfo(view))();
      }
    };

    canvas.addEventListener("click", handleCanvasClick);

    activeView = view;
    updateSunDataBuffers(view, true);
    setSunInfo(buildSunInfo(view))();

    return () => {
      canvas.removeEventListener("click", handleCanvasClick);
      if (activeView === view) {
        activeView = null;
      }
      quad2dGeom.dispose();
      quad2dMat.dispose();
      pic2dTexture.dispose();
      view.dispose();
      view.session.free();
    };
  };

export const generateSunGalaxy =
  (galaxyTypeId) =>
  (vantageId) =>
  (starFilter) =>
  (setSunInfo) =>
  () => {
    if (!activeView || !activeView.session) return;
    activeView.session.generate_galaxy_and_sun(
      galaxyTypeId,
      vantageId,
      starFilter
    );
    activeView.selectedTileIdx = -1;
    updateSunDataBuffers(activeView, false);
    setSunInfo(buildSunInfo(activeView))();
  };

export const chooseNewSunInGalaxy = (starFilter) => (setSunInfo) => () => {
  if (!activeView || !activeView.session) return;
  activeView.session.choose_sun_in_current_galaxy(starFilter);
  activeView.selectedTileIdx = -1;
  updateSunDataBuffers(activeView, false);
  setSunInfo(buildSunInfo(activeView))();
};

export const loadHipparcosIntoSun = (setSunInfo) => () => {
  if (!activeView || !activeView.session) return;
  activeView.session.load_real_hipparcos_sol();
  activeView.selectedTileIdx = -1;
  updateSunDataBuffers(activeView, false);
  setSunInfo(buildSunInfo(activeView))();
};

export const setSunAtmosphere = (atmosphereId) => (setSunInfo) => () => {
  if (!activeView || !activeView.session) return;
  activeView.session.set_atmosphere(atmosphereId);
  updateSunDataBuffers(activeView, false);
  setSunInfo(buildSunInfo(activeView))();
};

export const setSunDisplayOptions =
  (aspect) =>
  (showGalacticPlane) =>
  (showConstellations) =>
  () => {
    if (!activeView || !activeView.session) return;
    activeView.aspect = aspect;
    activeView.showGalacticPlane = showGalacticPlane;
    activeView.showConstellations = showConstellations;
    updateSunDataBuffers(activeView, false);
    if (activeView.setSunInfo) {
      activeView.setSunInfo(buildSunInfo(activeView))();
    }
  };

export const rotateGrid = (deg) => () => {
  if (!activeView) return;
  const angleRad = (deg * Math.PI) / 180.0;
  activeView.rotationAngleRad = angleRad;
  activeView.mesh.setRotationFromAxisAngle(_yAxis, angleRad);
  activeView.paint();
};
