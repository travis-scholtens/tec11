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
  WasmTidesSession,
  WasmWeatherSession,
  WasmWorldSession,
} from "tec11-wasm";
import {
  applyParsedTileToView,
  clearTileSkyAndVantageOnView,
  paintSparklineCanvas,
  registerSkyVantageHost,
  scheduleTileSkyAndVantagePaint,
} from "../Tec11.SkyVantage/foreign.js";

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
    const glaciation = glaciationSlider / 100.0;
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
  const glaciation = glaciationSlider / 100.0;
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
    stratigraphySummary: "",
    stratigraphyLines: [],
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
  } else if (view.inspectorMode === "tides" && view.setTidesInfo) {
    view.setTidesInfo(buildTidesInfo(view))();
  } else if (view.inspectorMode === "weather" && view.setWeatherInfo) {
    view.setWeatherInfo(buildWeatherInfo(view))();
  }
}

registerSkyVantageHost(
  () => activeView,
  (v) => refreshActiveInspectorView(v)
);

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
    clearTileSkyAndVantageOnView(view);
    return { stateView, tileView, languageDetail, nationDetail };
  }

  if (view.selectedTileIdx >= 0) {
    try {
      const parsedTile = JSON.parse(s.inspect_tile_structured_json(view.selectedTileIdx));
      const applied = applyParsedTileToView(view, parsedTile);
      if (applied) {
        tileView = applied;
      }
    } catch (_) {
      clearTileSkyAndVantageOnView(view);
    }
  } else {
    clearTileSkyAndVantageOnView(view);
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
  if (tileView.hasTile && view.tileSky) {
    scheduleTileSkyAndVantagePaint(view);
  }
  if (tileView.hasTile && typeof window !== "undefined") {
    window.requestAnimationFrame(() => paintStratigraphySideCanvas(view));
    setTimeout(() => paintStratigraphySideCanvas(view), 45);
    setTimeout(() => paintStratigraphySideCanvas(view), 120);
  }

  return { stateView, tileView, languageDetail, nationDetail };
}

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
    canvas.onmousemove = null;
    canvas.onmouseleave = null;
    const labelEl = document.getElementById("stratigraphy-hover-label");
    if (labelEl) {
      labelEl.textContent =
        "Click any tile on the globe to view its cross-section; hover a rock layer for composition & veins.";
    }
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
  const tSky = view.tileSky || null;
  const hasAtm = !tSky || tSky.hasAtmosphere !== false;
  const zRgb = tSky && Array.isArray(tSky.skyZenithRgb) ? tSky.skyZenithRgb : [0.16, 0.38, 0.82];
  const hRgb = tSky && Array.isArray(tSky.skyHorizonRgb) ? tSky.skyHorizonRgb : [0.48, 0.72, 0.96];
  const skyGrad = ctx.createLinearGradient(0, padTop, 0, Math.max(padTop + 20, Math.min(ySea, ySurfBase)));
  if (hasAtm) {
    skyGrad.addColorStop(
      0.0,
      `rgb(${Math.round(Math.min(255, (0.04 + 0.56 * zRgb[0]) * 255))},${Math.round(Math.min(255, (0.06 + 0.58 * zRgb[1]) * 255))},${Math.round(Math.min(255, (0.10 + 0.62 * zRgb[2]) * 255))})`
    );
    skyGrad.addColorStop(
      1.0,
      `rgb(${Math.round(Math.min(255, (0.08 + 0.68 * hRgb[0]) * 255))},${Math.round(Math.min(255, (0.10 + 0.70 * hRgb[1]) * 255))},${Math.round(Math.min(255, (0.14 + 0.72 * hRgb[2]) * 255))})`
    );
  } else {
    skyGrad.addColorStop(0.0, "#04070f");
    skyGrad.addColorStop(1.0, "#090d18");
  }
  ctx.fillStyle = skyGrad;
  ctx.fillRect(colLeft, padTop, colW, plotH);

  // If submarine (elevKm < 0) and planet has an atmosphere/hydrosphere, fill ocean water above the seafloor
  if (elevKm < 0.0 && hasAtm) {
    const oR = Math.round(Math.min(255, (0.03 + 0.42 * zRgb[0]) * 255));
    const oG = Math.round(Math.min(255, (0.08 + 0.52 * zRgb[1]) * 255));
    const oB = Math.round(Math.min(255, (0.16 + 0.62 * zRgb[2]) * 255));
    const oceanGrad = ctx.createLinearGradient(0, Math.max(padTop, ySea), 0, Math.min(cssH, ySurfBase + 12));
    oceanGrad.addColorStop(0.0, `rgba(${oR}, ${oG}, ${oB}, 0.78)`);
    oceanGrad.addColorStop(0.6, `rgba(${Math.round(oR * 0.68)}, ${Math.round(oG * 0.68)}, ${Math.round(oB * 0.74)}, 0.88)`);
    oceanGrad.addColorStop(1.0, `rgba(${Math.round(oR * 0.42)}, ${Math.round(oG * 0.44)}, ${Math.round(oB * 0.52)}, 0.95)`);
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
  for (let i = 0; i < strat.layers.length; i++) {
    const kl = strat.layers[i];
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
    view.dirtySnapshot = true;
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
    if (activeClimateSnapshotBytes) {
      view.session.load_snapshot_bytes(activeClimateSnapshotBytes);
    }
    view.dirtySnapshot = false;
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
      if (view.dirtySnapshot && view.session) {
        const bytes = view.session.encode_snapshot();
        if (bytes && bytes.length > 0) {
          activeClimateSnapshotBytes = bytes;
        }
      }
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
    activeView.dirtySnapshot = true;
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
  activeView.dirtySnapshot = true;
  updateWorldDataBuffers(activeView, false);
  setWorldInfo(buildWorldInfo(activeView))();
};

export const advanceWorldStage = (setWorldInfo) => () => {
  if (!activeView || !activeView.session) return;
  activeView.session.advance_stage();
  activeView.dirtySnapshot = true;
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
    activeView.dirtySnapshot = true;
    updateWorldDataBuffers(activeView, false);
    setWorldInfo(buildWorldInfo(activeView))();
  };

export const saveWorldPlanet = (setWorldInfo) => () => {
  if (!activeView || !activeView.session) return;
  const bytes = activeView.session.encode_snapshot();
  if (!bytes || bytes.length === 0) return;
  storeSavedSnapshotBytes(bytes);
  activeClimateSnapshotBytes = bytes;
  activeView.dirtySnapshot = false;
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
    activeView.dirtySnapshot = false;
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
      activeView.dirtySnapshot = false;
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

const MOON_ORBIT_LAYOUT = {
  W: 560,
  H: 460,
  cx: 280,
  cy: 226,
  rHelio: 124,
};

const MOON_TRAIL_PALETTE = [
  { fill: "#e2e8f0", stroke: "rgba(226, 232, 240, 0.72)", rgb: [226, 232, 240] },
  { fill: "#fcd34d", stroke: "rgba(252, 211, 77, 0.72)", rgb: [252, 211, 77] },
  { fill: "#67e8f9", stroke: "rgba(103, 232, 249, 0.72)", rgb: [103, 232, 249] },
  { fill: "#f9a8d4", stroke: "rgba(249, 168, 212, 0.72)", rgb: [249, 168, 212] },
];

const MOON_BG_STARS = (() => {
  const out = [];
  let s = 0x5eed1234;
  const next = () => {
    s = (Math.imul(s, 1664525) + 1013904223) >>> 0;
    return s / 4294967296.0;
  };
  for (let i = 0; i < 115; i++) {
    out.push({
      x: next() * MOON_ORBIT_LAYOUT.W,
      y: 36 + next() * (MOON_ORBIT_LAYOUT.H - 76),
      r: 0.45 + next() * 0.95,
      a: 0.16 + next() * 0.42,
      warm: next() < 0.35,
    });
  }
  return out;
})();

function moonDistRpToOrbitPx(distRp) {
  const clamped = Math.max(12.0, Math.min(410.0, distRp));
  return 15.0 + 41.0 * Math.pow((clamped - 10.0) / 400.0, 0.56);
}

function lunarPhaseNameFromElong(elongRad) {
  const norm = ((elongRad % (Math.PI * 2.0)) + Math.PI * 2.0) % (Math.PI * 2.0);
  const deg = (norm * 180.0) / Math.PI;
  if (deg < 22.5 || deg >= 337.5) return "New Moon";
  if (deg < 67.5) return "Waxing Crescent";
  if (deg < 112.5) return "First Quarter";
  if (deg < 157.5) return "Waxing Gibbous";
  if (deg < 202.5) return "Full Moon";
  if (deg < 247.5) return "Waning Gibbous";
  if (deg < 292.5) return "Third Quarter";
  return "Waning Crescent";
}

function ensureMoonOrbitalData(view) {
  if (view.orbitalSystemData) {
    return view.orbitalSystemData;
  }
  let parsed = {
    sun: {
      spectralType: "G2V",
      classDesc: "Yellow Main-Sequence Dwarf",
      tempK: 5778,
      massSolar: 1.0,
      radiusSolar: 1.0,
      luminositySolar: 1.0,
      orbitAu: 1.0,
      yearDays: 365.25,
      rgb: [1.0, 0.96, 0.84],
    },
    planet: {
      isCustom: false,
      geoticks: 0,
      landPct: 29,
      dayHours: 24.0,
      spin: 1.0,
      tiltDeg: 23.0,
      cells: 3,
      tidalStrength: 1.0,
      maxTidalRangeM: 4.5,
      coastalScourKm: 0.004,
      tidalSharePct: 35.0,
    },
    epochPct: 100,
    moonCount: 1,
    primaryMassLunar: 1.0,
    moons: [],
  };
  try {
    parsed = JSON.parse(view.session.orbital_system_json());
  } catch (_err) {
    // Use fallback above
  }
  view.orbitalSystemData = parsed;
  return parsed;
}

function computeMoonOrbitalFrame(orb, simDays) {
  const { cx, cy, rHelio } = MOON_ORBIT_LAYOUT;
  const yearDays = Math.max(8.0, orb.sun?.yearDays || 365.25);
  const thetaPlanet = ((simDays / yearDays) * Math.PI * 2.0) % (Math.PI * 2.0);
  const px = cx + rHelio * Math.cos(thetaPlanet);
  const py = cy - rHelio * Math.sin(thetaPlanet);
  const sunDirAngle = Math.atan2(cy - py, cx - px); // screen-space angle from planet -> sun

  // World-space angle from planet -> sun (with +y_world = -y_screen):
  const thetaSunFromPlanet = (thetaPlanet + Math.PI) % (Math.PI * 2.0);

  const moonsState = [];
  const moons = Array.isArray(orb.moons) ? orb.moons : [];
  for (let k = 0; k < moons.length; k++) {
    const m = moons[k];
    const period = Math.max(1.2, m.periodDays || 27.32);
    const phiWorld =
      ((simDays / period) * Math.PI * 2.0 + (m.phaseOffsetRad || 0.0)) % (Math.PI * 2.0);
    const rPx = moonDistRpToOrbitPx(m.distRp || 60.0);
    const mx = px + rPx * Math.cos(phiWorld);
    const my = py - rPx * Math.sin(phiWorld);
    // Elongation `0` = New Moon (between Planet and Sun), `PI` = Full Moon (opposite Sun):
    const elongRad =
      (((phiWorld - thetaSunFromPlanet) % (Math.PI * 2.0)) + Math.PI * 2.0) %
      (Math.PI * 2.0);
    const tideWeight =
      (m.massLunar || 1.0) * Math.pow(60.0 / Math.max(12.0, m.distRp || 60.0), 3.0);
    moonsState.push({
      meta: m,
      index: k,
      phiWorld,
      rPx,
      x: mx,
      y: my,
      elongRad,
      phaseName: lunarPhaseNameFromElong(elongRad),
      tideWeight,
    });
  }

  return {
    yearDays,
    thetaPlanet,
    px,
    py,
    sunDirAngle,
    thetaSunFromPlanet,
    moonsState,
  };
}

function formatMoonOrbitalSelectionReport(view) {
  const orb = ensureMoonOrbitalData(view);
  const frame = computeMoonOrbitalFrame(orb, view.orbitSimDays || 0.0);
  const sel = view.selectedOrbitBody || null;
  const sun = orb.sun;
  const pl = orb.planet;

  if (sel === "sun") {
    return (
      `Selected Body: Host Star (${sun.spectralType} — ${sun.classDesc})\n` +
      `Effective Temp: ${Math.round(sun.tempK)} K | Mass: ${sun.massSolar.toFixed(2)} M☉ | Radius: ${sun.radiusSolar.toFixed(2)} R☉ | Luminosity: ${sun.luminositySolar.toFixed(2)} L☉\n` +
      `Planet Heliocentric Orbit: ${sun.orbitAu.toFixed(2)} AU | Year Length: ${sun.yearDays.toFixed(1)} Earth days (${(sun.yearDays / Math.max(0.1, pl.dayHours / 24.0)).toFixed(0)} local solar days/yr)\n` +
      `Solar Tide Contribution: 0.15x Earth baseline (drives spring/neap modulation as moons cross syzygy & quadrature)`
    );
  }
  if (sel === "planet") {
    const pLabel = pl.isCustom
      ? `Loaded Planet (${pl.geoticks * 5} Myr, ${pl.landPct}% land)`
      : "Earth (ETOPO2)";
    return (
      `Selected Body: ${pLabel}\n` +
      `Rotation Day: ${pl.dayHours.toFixed(1)} h (${pl.cells} Hadley/Ferrel/Polar cells) | Axial Obliquity: ${pl.tiltDeg.toFixed(1)}°\n` +
      `Total Tidal Forcing: ${pl.tidalStrength.toFixed(2)}x Earth | Peak Coastal Range: ${pl.maxTidalRangeM.toFixed(1)} m\n` +
      `Coastal Tidal Scour: ${pl.coastalScourKm.toFixed(4)} km/tick (${pl.tidalSharePct.toFixed(1)}% of total coastal erosion)`
    );
  }
  if (typeof sel === "string" && sel.startsWith("moon_")) {
    const k = parseInt(sel.slice(5), 10);
    const ms = frame.moonsState[k];
    if (ms) {
      const m = ms.meta;
      const synodicDays =
        Math.abs(frame.yearDays - m.periodDays) > 0.05
          ? (frame.yearDays * m.periodDays) / Math.abs(frame.yearDays - m.periodDays)
          : m.periodDays;
      return (
        `Selected Body: Moon M${m.number} (${m.resonanceRatio}:1 Laplace Harmonic Orbit) — Current Phase: ${ms.phaseName}\n` +
        `Orbital Distance: ${m.distRp.toFixed(1)} Rp (Hadean birth ${m.initialDistRp.toFixed(1)} Rp → Mature target ${m.targetDistRp.toFixed(1)} Rp at Epoch ${orb.epochPct}%)\n` +
        `Sidereal Period: ${m.periodDays.toFixed(2)} Earth days | Synodic Phase Cycle: ${synodicDays.toFixed(2)} days | Mass: ${m.massLunar.toFixed(2)}x Lunar\n` +
        `Tidal Share: ${m.tideSharePct.toFixed(1)}% of lunar tidal forcing (scales as M / a³)`
      );
    }
  }
  return (
    "Live Sun–Planet–Moons Orrery: Watch the moons orbit the planet (forming braided epitrochoid trails, lit lunar phases, and live spring/neap ocean tidal bulges) while the planet orbits its Sun. " +
    "Adjust Harmonic Moons, Mass, Target Distance, or Hadean→Mature Epoch on the right, or click the Sun, Planet, or any Moon to inspect it."
  );
}

function drawLitBodyDisc(ctx, x, y, radius, sunAngleScreen, litCss, shadowCss) {
  // Base lit sphere:
  ctx.save();
  ctx.beginPath();
  ctx.arc(x, y, radius, 0, Math.PI * 2);
  ctx.fillStyle = litCss;
  ctx.fill();

  // Night-side hemisphere opposite `sunAngleScreen`:
  ctx.beginPath();
  ctx.arc(
    x,
    y,
    radius + 0.25,
    sunAngleScreen + Math.PI * 0.5,
    sunAngleScreen + Math.PI * 1.5
  );
  ctx.closePath();
  ctx.fillStyle = shadowCss;
  ctx.fill();
  ctx.restore();
}

function drawMoonOrbitalSystem(view) {
  const canvas = view.moonOrbitCanvas || document.getElementById("moon-orbit-canvas");
  if (!canvas || !view.session) return;

  const { W, H, cx, cy, rHelio } = MOON_ORBIT_LAYOUT;
  const dpr = Math.min(2.5, Math.max(1.0, window.devicePixelRatio || 1.0));
  const targetW = Math.round(W * dpr);
  const targetH = Math.round(H * dpr);
  if (canvas.width !== targetW || canvas.height !== targetH) {
    canvas.width = targetW;
    canvas.height = targetH;
  }

  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  ctx.save();
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

  const orb = ensureMoonOrbitalData(view);
  const simDays = view.orbitSimDays || 0.0;
  const frame = computeMoonOrbitalFrame(orb, simDays);
  view.lastOrbitFrame = frame;

  // 1. Deep-space radial gradient + subtle starfield:
  const bg = ctx.createRadialGradient(cx, cy, 12, cx, cy, W * 0.68);
  bg.addColorStop(0.0, "#0b132b");
  bg.addColorStop(0.55, "#060b19");
  bg.addColorStop(1.0, "#030610");
  ctx.fillStyle = bg;
  ctx.fillRect(0, 0, W, H);

  for (const st of MOON_BG_STARS) {
    ctx.fillStyle = st.warm
      ? `rgba(255, 236, 205, ${st.a})`
      : `rgba(210, 228, 255, ${st.a})`;
    ctx.beginPath();
    ctx.arc(st.x, st.y, st.r, 0, Math.PI * 2);
    ctx.fill();
  }

  // 2. Heliocentric Planet Orbit ring + Equinox/Solstice ticks + Planet wake arc:
  ctx.save();
  ctx.setLineDash([5, 5]);
  ctx.strokeStyle = "rgba(148, 163, 184, 0.28)";
  ctx.lineWidth = 1.1;
  ctx.beginPath();
  ctx.arc(cx, cy, rHelio, 0, Math.PI * 2);
  ctx.stroke();
  ctx.restore();

  ctx.font = "500 8.5px ui-monospace, SFMono-Regular, Menlo, monospace";
  ctx.fillStyle = "rgba(148, 163, 184, 0.50)";
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  const seasonLabels = [
    [0.0, "Vernal Eq", rHelio + 14, 0],
    [Math.PI * 0.5, "Solstice", 0, -(rHelio + 11)],
    [Math.PI, "Autumnal Eq", -(rHelio + 16), 0],
    [Math.PI * 1.5, "Solstice", 0, rHelio + 11],
  ];
  for (const [ang, lbl, ox, oy] of seasonLabels) {
    const tx = cx + rHelio * Math.cos(ang);
    const ty = cy - rHelio * Math.sin(ang);
    ctx.strokeStyle = "rgba(148, 163, 184, 0.35)";
    ctx.beginPath();
    ctx.arc(tx, ty, 2.0, 0, Math.PI * 2);
    ctx.stroke();
    if (orb.moons.length <= 2 || oy !== 0) {
      ctx.fillText(lbl, cx + ox, cy + oy);
    }
  }

  // Fading blue heliocentric wake arc behind the planet (~55° of its orbit):
  const wakeSegs = 28;
  const wakeSpan = 0.95;
  for (let i = 0; i < wakeSegs; i++) {
    const f0 = i / wakeSegs;
    const f1 = (i + 1) / wakeSegs;
    const a0 = frame.thetaPlanet - wakeSpan * (1.0 - f0);
    const a1 = frame.thetaPlanet - wakeSpan * (1.0 - f1);
    ctx.strokeStyle = `rgba(59, 130, 246, ${(0.05 + 0.42 * f1).toFixed(3)})`;
    ctx.lineWidth = 1.8;
    ctx.beginPath();
    ctx.moveTo(cx + rHelio * Math.cos(a0), cy - rHelio * Math.sin(a0));
    ctx.lineTo(cx + rHelio * Math.cos(a1), cy - rHelio * Math.sin(a1));
    ctx.stroke();
  }

  // 3. Central Home Star (Sun) with glowing corona in true spectral B-V color:
  const sRgb = orb.sun?.rgb || [1.0, 0.96, 0.84];
  const sr = Math.min(255, Math.round(sRgb[0] * 255));
  const sg = Math.min(255, Math.round(sRgb[1] * 255));
  const sb = Math.min(255, Math.round(sRgb[2] * 255));
  const sunRad = Math.max(14.0, Math.min(24.0, 18.0 * Math.sqrt(orb.sun?.radiusSolar || 1.0)));

  const corona = ctx.createRadialGradient(cx, cy, sunRad * 0.35, cx, cy, sunRad * 3.4);
  corona.addColorStop(0.0, `rgba(${sr}, ${sg}, ${sb}, 0.92)`);
  corona.addColorStop(0.35, `rgba(${sr}, ${sg}, ${sb}, 0.34)`);
  corona.addColorStop(1.0, `rgba(${sr}, ${sg}, ${sb}, 0.0)`);
  ctx.fillStyle = corona;
  ctx.beginPath();
  ctx.arc(cx, cy, sunRad * 3.4, 0, Math.PI * 2);
  ctx.fill();

  const sunCore = ctx.createRadialGradient(
    cx - sunRad * 0.25,
    cy - sunRad * 0.25,
    sunRad * 0.1,
    cx,
    cy,
    sunRad
  );
  sunCore.addColorStop(0.0, "#ffffff");
  sunCore.addColorStop(0.55, `rgb(${sr}, ${sg}, ${sb})`);
  sunCore.addColorStop(
    1.0,
    `rgb(${Math.round(sr * 0.78)}, ${Math.round(sg * 0.72)}, ${Math.round(sb * 0.65)})`
  );
  ctx.fillStyle = sunCore;
  ctx.beginPath();
  ctx.arc(cx, cy, sunRad, 0, Math.PI * 2);
  ctx.fill();

  if (view.selectedOrbitBody === "sun") {
    ctx.strokeStyle = "#34d399";
    ctx.lineWidth = 1.6;
    ctx.beginPath();
    ctx.arc(cx, cy, sunRad + 5.5, 0, Math.PI * 2);
    ctx.stroke();
  }

  ctx.font = "600 9.5px ui-monospace, SFMono-Regular, Menlo, monospace";
  ctx.fillStyle = "rgba(248, 250, 252, 0.85)";
  ctx.fillText(
    `${orb.sun.spectralType} (${orb.sun.orbitAu.toFixed(2)} AU)`,
    cx,
    cy + sunRad + 12
  );

  // 4. Braided Epitrochoid Trails of Each Moon Around the Sun + Planet:
  const trailSpanDays = Math.min(frame.yearDays * 0.26, 72.0);
  const trailSteps = 84;
  for (let k = 0; k < frame.moonsState.length; k++) {
    const ms = frame.moonsState[k];
    const pal = MOON_TRAIL_PALETTE[k % MOON_TRAIL_PALETTE.length];
    const period = Math.max(1.2, ms.meta.periodDays || 27.32);
    const phaseOff = ms.meta.phaseOffsetRad || 0.0;
    let prevX = 0;
    let prevY = 0;
    for (let s = 0; s <= trailSteps; s++) {
      const frac = s / trailSteps;
      const tDay = simDays - trailSpanDays * (1.0 - frac);
      const thP = (tDay / frame.yearDays) * Math.PI * 2.0;
      const pxt = cx + rHelio * Math.cos(thP);
      const pyt = cy - rHelio * Math.sin(thP);
      const phiM = (tDay / period) * Math.PI * 2.0 + phaseOff;
      const mxt = pxt + ms.rPx * Math.cos(phiM);
      const myt = pyt - ms.rPx * Math.sin(phiM);
      if (s > 0) {
        const alpha = 0.04 + 0.48 * Math.pow(frac, 1.5);
        ctx.strokeStyle = `rgba(${pal.rgb[0]}, ${pal.rgb[1]}, ${pal.rgb[2]}, ${alpha.toFixed(3)})`;
        ctx.lineWidth = 1.15;
        ctx.beginPath();
        ctx.moveTo(prevX, prevY);
        ctx.lineTo(mxt, myt);
        ctx.stroke();
      }
      prevX = mxt;
      prevY = myt;
    }
  }

  // 5. Instantaneous Concentric Lunar Orbit Rings Around Moving Planet:
  for (let k = 0; k < frame.moonsState.length; k++) {
    const ms = frame.moonsState[k];
    ctx.strokeStyle = "rgba(148, 163, 184, 0.24)";
    ctx.lineWidth = 0.85;
    ctx.beginPath();
    ctx.arc(frame.px, frame.py, ms.rPx, 0, Math.PI * 2);
    ctx.stroke();
  }

  // 6. Live Quadrupolar Ocean Tidal Bulge Envelope Around the Planet:
  const planetRad = 9.0;
  let maxBulgeP2 = 0.0;
  ctx.save();
  ctx.beginPath();
  const bulgeSegs = 56;
  for (let i = 0; i <= bulgeSegs; i++) {
    const aWorld = (i / bulgeSegs) * Math.PI * 2.0;
    const cosSun = Math.cos(aWorld - frame.thetaSunFromPlanet);
    let p2 = 0.18 * 0.5 * (3.0 * cosSun * cosSun - 1.0);
    for (const ms of frame.moonsState) {
      const cosM = Math.cos(aWorld - ms.phiWorld);
      p2 += Math.min(2.6, ms.tideWeight) * 0.5 * (3.0 * cosM * cosM - 1.0);
    }
    if (p2 > maxBulgeP2) maxBulgeP2 = p2;
    const bulgePx = Math.max(-1.8, Math.min(8.5, 2.2 + 3.4 * p2));
    const rEnv = planetRad + bulgePx;
    const bx = frame.px + rEnv * Math.cos(aWorld);
    const by = frame.py - rEnv * Math.sin(aWorld);
    if (i === 0) ctx.moveTo(bx, by);
    else ctx.lineTo(bx, by);
  }
  ctx.closePath();
  ctx.fillStyle = "rgba(56, 189, 248, 0.28)";
  ctx.strokeStyle = "rgba(125, 211, 252, 0.72)";
  ctx.lineWidth = 1.0;
  ctx.fill();
  ctx.stroke();
  ctx.restore();

  // 7. Moving Planet Disc (with Rotating Continent Spot, Axial Tilt Axis & Day/Night Terminator):
  const tiltRad = ((orb.planet?.tiltDeg || 23.0) * Math.PI) / 180.0;
  ctx.strokeStyle = "rgba(248, 250, 252, 0.52)";
  ctx.lineWidth = 1.0;
  ctx.beginPath();
  ctx.moveTo(
    frame.px - (planetRad + 4.5) * Math.sin(tiltRad),
    frame.py - (planetRad + 4.5) * Math.cos(tiltRad)
  );
  ctx.lineTo(
    frame.px + (planetRad + 4.5) * Math.sin(tiltRad),
    frame.py + (planetRad + 4.5) * Math.cos(tiltRad)
  );
  ctx.stroke();

  drawLitBodyDisc(
    ctx,
    frame.px,
    frame.py,
    planetRad,
    frame.sunDirAngle,
    "#2563eb",
    "rgba(4, 9, 24, 0.76)"
  );
  // Subtle rotating continent patch on the planet to convey spin rate (`24 / dayHours`):
  const spinPhase = simDays * (orb.planet?.spin || 1.0) * 0.85;
  const contOff = Math.sin(spinPhase) * (planetRad * 0.48);
  ctx.fillStyle = "rgba(52, 211, 153, 0.72)";
  ctx.beginPath();
  ctx.arc(frame.px + contOff, frame.py - 1.5, 2.7, 0, Math.PI * 2);
  ctx.fill();

  if (view.selectedOrbitBody === "planet") {
    ctx.strokeStyle = "#34d399";
    ctx.lineWidth = 1.6;
    ctx.beginPath();
    ctx.arc(frame.px, frame.py, planetRad + 6.5, 0, Math.PI * 2);
    ctx.stroke();
  }

  // 8. Moving Moons M1..M4 with Sunlit Lunar Phases:
  for (let k = 0; k < frame.moonsState.length; k++) {
    const ms = frame.moonsState[k];
    const pal = MOON_TRAIL_PALETTE[k % MOON_TRAIL_PALETTE.length];
    const mRad = Math.max(
      3.4,
      Math.min(6.4, 4.3 * Math.cbrt(Math.max(0.15, ms.meta.massLunar || 1.0)))
    );
    const moonToSunScreen = Math.atan2(cy - ms.y, cx - ms.x);
    drawLitBodyDisc(
      ctx,
      ms.x,
      ms.y,
      mRad,
      moonToSunScreen,
      pal.fill,
      "rgba(8, 12, 26, 0.82)"
    );
    ctx.strokeStyle = pal.stroke;
    ctx.lineWidth = 0.85;
    ctx.beginPath();
    ctx.arc(ms.x, ms.y, mRad, 0, Math.PI * 2);
    ctx.stroke();

    if (view.selectedOrbitBody === `moon_${k}`) {
      ctx.strokeStyle = "#34d399";
      ctx.lineWidth = 1.6;
      ctx.beginPath();
      ctx.arc(ms.x, ms.y, mRad + 4.5, 0, Math.PI * 2);
      ctx.stroke();
    }

    ctx.font = "600 8.5px ui-monospace, SFMono-Regular, Menlo, monospace";
    ctx.fillStyle = pal.fill;
    ctx.fillText(`M${ms.meta.number}`, ms.x + mRad + 7.5, ms.y - mRad - 2.0);
  }

  // 9. Top Status HUD Bar (Year/Day Clock + Live Spring/Neap Syzygy Indicator):
  const yrNum = Math.floor(simDays / frame.yearDays) + 1;
  const dayInYr = Math.floor(((simDays % frame.yearDays) + frame.yearDays) % frame.yearDays) + 1;

  let tideStateLabel = "Solar Tide Only (Moonless)";
  let tideBadgeColor = "#94a3b8";
  if (frame.moonsState.length > 0) {
    const m1 = frame.moonsState[0];
    const alignCos = Math.abs(Math.cos(m1.elongRad)); // 1.0 at New/Full syzygy, 0.0 at Quarter quadrature
    if (alignCos >= 0.91) {
      tideStateLabel = `Spring Tide (Syzygy · ${m1.phaseName})`;
      tideBadgeColor = "#fbbf24";
    } else if (alignCos <= 0.36) {
      tideStateLabel = `Neap Tide (Quadrature · ${m1.phaseName})`;
      tideBadgeColor = "#60a5fa";
    } else {
      tideStateLabel = `${m1.phaseName} Tide`;
      tideBadgeColor = "#cbd5e1";
    }
  }

  ctx.textAlign = "left";
  ctx.font = "600 11px ui-monospace, SFMono-Regular, Menlo, monospace";
  ctx.fillStyle = "#e2e8f0";
  ctx.fillText(
    `Year ${yrNum} · Day ${String(dayInYr).padStart(3, " ")}/${Math.round(frame.yearDays)}`,
    14,
    18
  );
  ctx.font = "500 9.5px ui-monospace, SFMono-Regular, Menlo, monospace";
  ctx.fillStyle = "#94a3b8";
  ctx.fillText(
    `Spin Day: ${orb.planet.dayHours.toFixed(1)}h · Tilt: ${orb.planet.tiltDeg.toFixed(1)}° · Epoch: ${orb.epochPct}%`,
    14,
    32
  );

  ctx.textAlign = "right";
  ctx.font = "600 10.5px Inter, system-ui, sans-serif";
  ctx.fillStyle = tideBadgeColor;
  ctx.fillText(tideStateLabel, W - 14, 18);
  ctx.font = "500 9.5px ui-monospace, SFMono-Regular, Menlo, monospace";
  ctx.fillStyle = "#7dd3fc";
  ctx.fillText(
    `Peak Range: ${orb.planet.maxTidalRangeM.toFixed(1)}m (${orb.planet.tidalStrength.toFixed(2)}x Earth)`,
    W - 14,
    32
  );

  // 10. Bottom Harmonic Resonance Legend Strip:
  ctx.fillStyle = "rgba(15, 23, 42, 0.80)";
  ctx.strokeStyle = "rgba(148, 163, 184, 0.22)";
  ctx.lineWidth = 1.0;
  ctx.beginPath();
  if (ctx.roundRect) {
    ctx.roundRect(12, H - 31, W - 24, 23, 6);
  } else {
    ctx.rect(12, H - 31, W - 24, 23);
  }
  ctx.fill();
  ctx.stroke();

  ctx.textAlign = "left";
  ctx.font = "500 9.5px ui-monospace, SFMono-Regular, Menlo, monospace";
  if (frame.moonsState.length === 0) {
    ctx.fillStyle = "#94a3b8";
    ctx.fillText(
      `0 Moons — Planet orbits ${orb.sun.spectralType} Sun (${Math.round(frame.yearDays)}d yr) with unbraked ${orb.planet.dayHours.toFixed(1)}h spin`,
      22,
      H - 19
    );
  } else {
    let lx = 20;
    const ly = H - 19;
    for (let k = 0; k < frame.moonsState.length; k++) {
      const ms = frame.moonsState[k];
      const pal = MOON_TRAIL_PALETTE[k % MOON_TRAIL_PALETTE.length];
      ctx.fillStyle = pal.fill;
      ctx.beginPath();
      ctx.arc(lx + 4, ly - 0.5, 3.4, 0, Math.PI * 2);
      ctx.fill();
      const entry = `M${ms.meta.number}(${ms.meta.resonanceRatio}:1): ${ms.meta.periodDays.toFixed(1)}d @ ${ms.meta.distRp.toFixed(0)}Rp`;
      ctx.fillText(entry, lx + 11, ly);
      lx += ctx.measureText(entry).width + 21;
    }
  }

  ctx.restore();
}

function stopMoonOrbitLoop(view) {
  if (view && view.orbitRafId !== null && view.orbitRafId !== undefined) {
    cancelAnimationFrame(view.orbitRafId);
    view.orbitRafId = null;
  }
  if (view) {
    view.orbitLastTs = null;
  }
}

function syncMoonOrbitLoop(view) {
  if (!view) return;
  if (view.aspect !== "orbit_system" || !view.orbitPlaying) {
    stopMoonOrbitLoop(view);
    if (view.aspect === "orbit_system") {
      drawMoonOrbitalSystem(view);
    }
    return;
  }
  if (view.orbitRafId !== null && view.orbitRafId !== undefined) {
    return;
  }
  const tick = (ts) => {
    if (activeView !== view || view.aspect !== "orbit_system" || !view.orbitPlaying) {
      view.orbitRafId = null;
      view.orbitLastTs = null;
      return;
    }
    if (view.orbitLastTs !== null && view.orbitLastTs !== undefined) {
      const dtSec = Math.min(0.08, Math.max(0.0, (ts - view.orbitLastTs) / 1000.0));
      // Base rate `18` Earth days / sec (~20s per Earth year, ~1.5s per lunar month at 1x):
      view.orbitSimDays = (view.orbitSimDays || 0.0) + dtSec * 18.0 * (view.orbitSpeed || 1.0);
    }
    view.orbitLastTs = ts;
    drawMoonOrbitalSystem(view);
    view.orbitRafId = requestAnimationFrame(tick);
  };
  view.orbitRafId = requestAnimationFrame(tick);
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
  view.orbitalSystemData = null;
  if (view.aspect === "orbit_system") {
    drawMoonOrbitalSystem(view);
    syncMoonOrbitLoop(view);
    return;
  }
  stopMoonOrbitLoop(view);
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
    view.aspect === "orbit_system"
      ? formatMoonOrbitalSelectionReport(view)
      : view.selectedTileIdx >= 0
      ? s.inspect_tile(view.selectedTileIdx)
      : "Click any coastal, ocean, or inland tile on the globe to inspect local tidal range (m), tidal scour vs. rainfall erosion rates, shelf deposition, and rock strata.";
  if (view.aspect !== "orbit_system") {
    scheduleStratigraphySidePaint(view, view.selectedTileIdx);
  }
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
    const moonOrbitCanvas = document.getElementById("moon-orbit-canvas");
    view.moonOrbitCanvas = moonOrbitCanvas;
    view.orbitalSystemData = null;
    view.orbitSimDays = 0.0;
    view.orbitPlaying = true;
    view.orbitSpeed = 1.0;
    view.orbitRafId = null;
    view.orbitLastTs = null;
    view.selectedOrbitBody = null;

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

    const handleOrbitCanvasClick = (ev) => {
      if (!view.session || !moonOrbitCanvas) return;
      const rect = moonOrbitCanvas.getBoundingClientRect();
      if (rect.width <= 0 || rect.height <= 0) return;
      const mx = ((ev.clientX - rect.left) / rect.width) * MOON_ORBIT_LAYOUT.W;
      const my = ((ev.clientY - rect.top) / rect.height) * MOON_ORBIT_LAYOUT.H;
      const orb = ensureMoonOrbitalData(view);
      const frame = view.lastOrbitFrame || computeMoonOrbitalFrame(orb, view.orbitSimDays || 0.0);

      let picked = null;
      let bestDist = 18.0;
      for (let k = 0; k < frame.moonsState.length; k++) {
        const ms = frame.moonsState[k];
        const d = Math.hypot(mx - ms.x, my - ms.y);
        if (d < bestDist) {
          bestDist = d;
          picked = `moon_${k}`;
        }
      }
      if (!picked && Math.hypot(mx - frame.px, my - frame.py) <= 18.0) {
        picked = "planet";
      }
      if (!picked && Math.hypot(mx - MOON_ORBIT_LAYOUT.cx, my - MOON_ORBIT_LAYOUT.cy) <= 28.0) {
        picked = "sun";
      }
      view.selectedOrbitBody = picked;
      drawMoonOrbitalSystem(view);
      view.setMoonInfo(buildMoonInfo(view))();
    };

    canvas.addEventListener("click", handleCanvasClick);
    if (moonOrbitCanvas) {
      moonOrbitCanvas.addEventListener("click", handleOrbitCanvasClick);
    }

    activeView = view;
    updateMoonDataBuffers(view, true);
    setMoonInfo(buildMoonInfo(view))();

    return () => {
      stopMoonOrbitLoop(view);
      canvas.removeEventListener("click", handleCanvasClick);
      if (moonOrbitCanvas) {
        moonOrbitCanvas.removeEventListener("click", handleOrbitCanvasClick);
      }
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
    if (activeView.setMoonInfo) {
      activeView.setMoonInfo(buildMoonInfo(activeView))();
    }
  };

export const setMoonOrbitAnimControls =
  (playing) =>
  (speed) =>
  () => {
    if (!activeView || !activeView.session) return;
    activeView.orbitPlaying = Boolean(playing);
    activeView.orbitSpeed = Math.max(0.1, speed || 1.0);
    syncMoonOrbitLoop(activeView);
    if (activeView.setMoonInfo) {
      activeView.setMoonInfo(buildMoonInfo(activeView))();
    }
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

const STAR_MAP_LAYOUT = {
  W: 760,
  H: 430,
  cxN: 196,
  cyN: 214,
  cxS: 564,
  cyS: 214,
  R: 154,
};

function buildHemisphereGlowCanvas(isNorth, meta) {
  const S = 112;
  const cnv = document.createElement("canvas");
  cnv.width = S;
  cnv.height = S;
  const ctx = cnv.getContext("2d");
  if (!ctx) return cnv;

  const img = ctx.createImageData(S, S);
  const data = img.data;
  const gp = meta.galacticPole || [-0.86767, -0.19808, 0.45598];
  const gc = meta.galacticCenterDir || [-0.05488, -0.87344, -0.48384];
  const mode = meta.galaxyBandMode || 0;
  const tint = meta.warmGlow ? [242, 198, 138] : [150, 188, 252];
  const half = S * 0.5;

  for (let py = 0; py < S; py++) {
    const ny = (py + 0.5 - half) / half;
    for (let px = 0; px < S; px++) {
      const nx = (px + 0.5 - half) / half;
      const rFrac = Math.hypot(nx, ny);
      const off = (py * S + px) * 4;
      if (rFrac > 1.03) {
        data[off + 3] = 0;
        continue;
      }
      const clampedR = Math.min(1.0, rFrac);
      const decDeg = isNorth ? 90.0 * (1.0 - clampedR) : -90.0 * (1.0 - clampedR);
      const raRad = isNorth ? Math.atan2(nx, -ny) : Math.atan2(-nx, -ny);
      const decRad = (decDeg * Math.PI) / 180.0;
      const cosD = Math.cos(decRad);
      const dx = cosD * Math.cos(raRad);
      const dy = cosD * Math.sin(raRad);
      const dz = Math.sin(decRad);

      const sinB = dx * gp[0] + dy * gp[1] + dz * gp[2];
      const cosCore = dx * gc[0] + dy * gc[1] + dz * gc[2];
      let glow = 0.0;
      if (mode === 1) {
        const diskHemi = Math.pow(Math.max(0.0, -sinB), 1.45) * (0.4 + 0.6 * Math.max(0.0, cosCore));
        glow = 0.24 * diskHemi;
      } else if (mode === 2) {
        const coreHemi = Math.pow(Math.max(0.0, cosCore + 0.35) / 1.35, 2.0);
        glow = 0.20 * coreHemi;
      } else {
        const band = Math.exp(-Math.pow(sinB / 0.19, 2.0));
        const bulge = Math.pow(Math.max(0.0, cosCore), 4.0) * Math.exp(-Math.pow(sinB / 0.33, 2.0));
        glow = 0.15 * band + 0.19 * bulge;
      }

      const limb = 0.03 * Math.pow(clampedR, 2.2);
      data[off] = Math.min(255, Math.round(5 + limb * 28 + glow * tint[0]));
      data[off + 1] = Math.min(255, Math.round(9 + limb * 42 + glow * tint[1]));
      data[off + 2] = Math.min(255, Math.round(22 + limb * 76 + glow * tint[2]));
      data[off + 3] = 255;
    }
  }
  ctx.putImageData(img, 0, 0);
  return cnv;
}

function sampleGreatCircleEquatorial(poleVec, refVec, count = 160) {
  const pLen = Math.hypot(poleVec[0], poleVec[1], poleVec[2]) || 1.0;
  const n = [poleVec[0] / pLen, poleVec[1] / pLen, poleVec[2] / pLen];
  let ref = refVec && refVec.length === 3 ? refVec : [0, 0, 1];
  const dotNR = n[0] * ref[0] + n[1] * ref[1] + n[2] * ref[2];
  let u = [ref[0] - dotNR * n[0], ref[1] - dotNR * n[1], ref[2] - dotNR * n[2]];
  let uLen = Math.hypot(u[0], u[1], u[2]);
  if (uLen < 1e-4) {
    const alt = Math.abs(n[2]) < 0.9 ? [0, 0, 1] : [1, 0, 0];
    u = [
      n[1] * alt[2] - n[2] * alt[1],
      n[2] * alt[0] - n[0] * alt[2],
      n[0] * alt[1] - n[1] * alt[0],
    ];
    uLen = Math.hypot(u[0], u[1], u[2]) || 1.0;
  }
  u = [u[0] / uLen, u[1] / uLen, u[2] / uLen];
  const v = [
    n[1] * u[2] - n[2] * u[1],
    n[2] * u[0] - n[0] * u[2],
    n[0] * u[1] - n[1] * u[0],
  ];
  const pts = [];
  for (let i = 0; i <= count; i++) {
    const a = (i / count) * Math.PI * 2.0;
    const ca = Math.cos(a);
    const sa = Math.sin(a);
    const x = ca * u[0] + sa * v[0];
    const y = ca * u[1] + sa * v[1];
    const z = Math.max(-1.0, Math.min(1.0, ca * u[2] + sa * v[2]));
    pts.push({
      raRad: Math.atan2(y, x),
      decDeg: (Math.asin(z) * 180.0) / Math.PI,
    });
  }
  return pts;
}

function ensureStarMapCache(view) {
  if (view.starMapCache) {
    return view.starMapCache;
  }
  const { session } = view;
  const starsRaw = session.star_map_stars_data();
  const edgesRaw = session.star_map_constellation_edges();
  let meta = {
    isRealHipparcos: false,
    starCount: 0,
    constellationCount: 0,
    galaxyBandMode: 0,
    warmGlow: false,
    galacticPole: [-0.86767, -0.19808, 0.45598],
    galacticCenterDir: [-0.05488, -0.87344, -0.48384],
    eclipticPole: [0.0, -0.398, 0.917],
    planets: [],
    constellations: [],
  };
  try {
    meta = JSON.parse(session.star_map_metadata_json());
  } catch (_err) {
    // Fallback metadata above
  }

  const starCount = Math.floor(starsRaw.length / 6);
  const drawOrder = new Uint32Array(starCount);
  for (let i = 0; i < starCount; i++) {
    drawOrder[i] = i;
  }
  // Sort faintest (highest numeric magnitude) first so bright stars paint crisply on top:
  const orderArr = Array.from(drawOrder);
  orderArr.sort((a, b) => starsRaw[b * 6 + 2] - starsRaw[a * 6 + 2]);

  const galCurve = sampleGreatCircleEquatorial(meta.galacticPole, meta.galacticCenterDir, 180);
  const eclCurve = sampleGreatCircleEquatorial(meta.eclipticPole, [1, 0, 0], 180);
  const gc = meta.galacticCenterDir || [-0.055, -0.873, -0.484];
  const gcDecDeg = (Math.asin(Math.max(-1.0, Math.min(1.0, gc[2]))) * 180.0) / Math.PI;
  const gcRaRad = Math.atan2(gc[1], gc[0]);

  const cache = {
    starsRaw,
    edgesRaw,
    meta,
    starCount,
    orderArr,
    galCurve,
    eclCurve,
    gcRaRad,
    gcDecDeg,
    northGlow: buildHemisphereGlowCanvas(true, meta),
    southGlow: buildHemisphereGlowCanvas(false, meta),
  };
  view.starMapCache = cache;
  return cache;
}

function projectToHemisphere(raRad, decDeg, rotRad, isNorth, cx, cy, R) {
  const rFrac = isNorth ? (90.0 - decDeg) / 90.0 : (90.0 + decDeg) / 90.0;
  const phi = raRad + rotRad;
  const sp = Math.sin(phi);
  const cp = Math.cos(phi);
  return {
    x: isNorth ? cx + R * rFrac * sp : cx - R * rFrac * sp,
    y: cy - R * rFrac * cp,
    rFrac,
  };
}

function drawDualHemisphereStarMap(view) {
  const canvas = view.starMapCanvas || document.getElementById("sun-starmap-canvas");
  if (!canvas || !view.session) return;

  const { W, H, cxN, cyN, cxS, cyS, R } = STAR_MAP_LAYOUT;
  const dpr = Math.min(2.5, Math.max(1.0, window.devicePixelRatio || 1.0));
  const targetW = Math.round(W * dpr);
  const targetH = Math.round(H * dpr);
  if (canvas.width !== targetW || canvas.height !== targetH) {
    canvas.width = targetW;
    canvas.height = targetH;
  }

  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  ctx.save();
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

  const cache = ensureStarMapCache(view);
  const { starsRaw, edgesRaw, meta, orderArr, galCurve, eclCurve } = cache;
  const rotRad = view.rotationAngleRad || 0.0;
  const selStarIdx = typeof view.selectedStarIdx === "number" ? view.selectedStarIdx : -1;
  const selConstIdx =
    selStarIdx >= 0 ? view.session.star_constellation_index(selStarIdx) : -1;

  // 1. Deep midnight celestial atlas backdrop:
  const bgGrad = ctx.createLinearGradient(0, 0, 0, H);
  bgGrad.addColorStop(0.0, "#050814");
  bgGrad.addColorStop(1.0, "#080d1d");
  ctx.fillStyle = bgGrad;
  ctx.fillRect(0, 0, W, H);

  // Title headers above Northern & Southern discs:
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.font = "700 12px Inter, system-ui, sans-serif";
  ctx.fillStyle = "#e2e8f0";
  ctx.fillText("NORTHERN CELESTIAL HEMISPHERE", cxN, 17);
  ctx.fillText("SOUTHERN CELESTIAL HEMISPHERE", cxS, 17);
  ctx.font = "500 10px ui-monospace, SFMono-Regular, Menlo, monospace";
  ctx.fillStyle = "#94a3b8";
  ctx.fillText("Pole +90° (Center) · Equator 0° (Rim)", cxN, 32);
  ctx.fillText("Pole −90° (Center) · Equator 0° (Rim)", cxS, 32);

  const drawSingleHemisphere = (isNorth, cx, cy, glowCanvas) => {
    // Outer graduation bezel ring:
    ctx.save();
    ctx.beginPath();
    ctx.arc(cx, cy, R + 15, 0, Math.PI * 2);
    ctx.fillStyle = "rgba(15, 23, 42, 0.72)";
    ctx.fill();
    ctx.strokeStyle = "rgba(148, 163, 184, 0.28)";
    ctx.lineWidth = 1.0;
    ctx.stroke();
    ctx.restore();

    // Clipped celestial hemisphere interior (`r <= R`):
    ctx.save();
    ctx.beginPath();
    ctx.arc(cx, cy, R, 0, Math.PI * 2);
    ctx.clip();

    // Rotated diffuse Milky Way / host-galaxy glow texture:
    ctx.save();
    ctx.translate(cx, cy);
    ctx.rotate(isNorth ? rotRad : -rotRad);
    ctx.drawImage(glowCanvas, -R - 2, -R - 2, (R + 2) * 2, (R + 2) * 2);
    ctx.restore();

    // Subtle Right Ascension hour spokes (every 2h = 30°) & Declination circles (±60°, ±30°):
    ctx.strokeStyle = "rgba(148, 163, 184, 0.14)";
    ctx.lineWidth = 0.75;
    for (let h = 0; h < 24; h += 2) {
      const ra = (h * 15.0 * Math.PI) / 180.0;
      const pOuter = projectToHemisphere(ra, 0.0, rotRad, isNorth, cx, cy, R);
      const pInner = projectToHemisphere(
        ra,
        isNorth ? 80.0 : -80.0,
        rotRad,
        isNorth,
        cx,
        cy,
        R
      );
      ctx.beginPath();
      ctx.moveTo(pInner.x, pInner.y);
      ctx.lineTo(pOuter.x, pOuter.y);
      ctx.stroke();
    }

    ctx.setLineDash([3, 3]);
    ctx.strokeStyle = "rgba(148, 163, 184, 0.19)";
    for (const rRing of [R / 3.0, (2.0 * R) / 3.0]) {
      ctx.beginPath();
      ctx.arc(cx, cy, rRing, 0, Math.PI * 2);
      ctx.stroke();
    }
    ctx.setLineDash([]);

    // Polar crosshair at +90° / -90°:
    ctx.strokeStyle = "rgba(148, 163, 184, 0.45)";
    ctx.lineWidth = 0.9;
    ctx.beginPath();
    ctx.moveTo(cx - 4.5, cy);
    ctx.lineTo(cx + 4.5, cy);
    ctx.moveTo(cx, cy - 4.5);
    ctx.lineTo(cx, cy + 4.5);
    ctx.stroke();

    // Helper to stroke great-circle curves (Galactic Equator & Ecliptic):
    const strokeGreatCircle = (pts, strokeStyle, lineWidth, dash) => {
      ctx.save();
      ctx.strokeStyle = strokeStyle;
      ctx.lineWidth = lineWidth;
      if (dash) ctx.setLineDash(dash);
      ctx.beginPath();
      let pen = false;
      let prevX = 0;
      let prevY = 0;
      for (let i = 0; i < pts.length; i++) {
        const pt = pts[i];
        const inHemi = isNorth ? pt.decDeg >= -6.0 : pt.decDeg <= 6.0;
        if (!inHemi) {
          pen = false;
          continue;
        }
        const pr = projectToHemisphere(pt.raRad, pt.decDeg, rotRad, isNorth, cx, cy, R);
        if (!pen || Math.hypot(pr.x - prevX, pr.y - prevY) > R * 0.45) {
          ctx.moveTo(pr.x, pr.y);
          pen = true;
        } else {
          ctx.lineTo(pr.x, pr.y);
        }
        prevX = pr.x;
        prevY = pr.y;
      }
      ctx.stroke();
      ctx.restore();
    };

    // Galactic Equator curve + Galactic Center diamond marker:
    if (view.showGalacticPlane) {
      strokeGreatCircle(galCurve, "rgba(56, 189, 248, 0.42)", 1.15, [5, 4]);
      const gcInHemi = isNorth ? cache.gcDecDeg >= 0.0 : cache.gcDecDeg < 0.0;
      if (gcInHemi) {
        const gcp = projectToHemisphere(
          cache.gcRaRad,
          cache.gcDecDeg,
          rotRad,
          isNorth,
          cx,
          cy,
          R
        );
        ctx.save();
        ctx.strokeStyle = "rgba(56, 189, 248, 0.85)";
        ctx.fillStyle = "rgba(14, 116, 144, 0.45)";
        ctx.lineWidth = 1.1;
        ctx.beginPath();
        ctx.moveTo(gcp.x, gcp.y - 4.5);
        ctx.lineTo(gcp.x + 4.5, gcp.y);
        ctx.lineTo(gcp.x, gcp.y + 4.5);
        ctx.lineTo(gcp.x - 4.5, gcp.y);
        ctx.closePath();
        ctx.fill();
        ctx.stroke();
        ctx.font = "600 8.5px ui-monospace, SFMono-Regular, Menlo, monospace";
        ctx.fillStyle = "rgba(125, 211, 252, 0.9)";
        ctx.fillText("GC", gcp.x + 11, gcp.y - 5);
        ctx.restore();
      }
    }

    // Ecliptic curve (zodiac path of Sun & planets):
    strokeGreatCircle(eclCurve, "rgba(251, 191, 36, 0.34)", 1.0, [3, 4]);

    // Constellation asterism lines (clipping cleanly across the 0° equator rim):
    if (view.showConstellations && edgesRaw.length >= 3) {
      const edgeCount = Math.floor(edgesRaw.length / 3);
      ctx.lineCap = "round";
      for (let pass = 0; pass < 2; pass++) {
        for (let e = 0; e < edgeCount; e++) {
          const ia = edgesRaw[e * 3];
          const ib = edgesRaw[e * 3 + 1];
          const cIdx = edgesRaw[e * 3 + 2];
          const isSelConst = cIdx === selConstIdx;
          if ((pass === 0 && isSelConst) || (pass === 1 && !isSelConst)) {
            continue;
          }
          const raA = starsRaw[ia * 6];
          const decA = starsRaw[ia * 6 + 1];
          const raB = starsRaw[ib * 6];
          const decB = starsRaw[ib * 6 + 1];
          const visible = isNorth
            ? decA >= -14.0 && decB >= -14.0 && (decA >= 0.0 || decB >= 0.0)
            : decA <= 14.0 && decB <= 14.0 && (decA <= 0.0 || decB <= 0.0);
          if (!visible) continue;

          const pA = projectToHemisphere(raA, decA, rotRad, isNorth, cx, cy, R);
          const pB = projectToHemisphere(raB, decB, rotRad, isNorth, cx, cy, R);
          if (Math.hypot(pA.x - pB.x, pA.y - pB.y) > R * 0.72) continue;

          ctx.strokeStyle = isSelConst
            ? "rgba(52, 211, 153, 0.95)"
            : "rgba(96, 165, 250, 0.58)";
          ctx.lineWidth = isSelConst ? 2.1 : 1.25;
          ctx.beginPath();
          ctx.moveTo(pA.x, pA.y);
          ctx.lineTo(pB.x, pB.y);
          ctx.stroke();
        }
      }
    }

    // All naked-eye stars in this hemisphere (faintest -> brightest):
    for (let k = 0; k < orderArr.length; k++) {
      const idx = orderArr[k];
      const base = idx * 6;
      const decDeg = starsRaw[base + 1];
      if (isNorth ? decDeg < -0.6 : decDeg > 0.6) {
        continue;
      }
      const raRad = starsRaw[base];
      const mag = starsRaw[base + 2];
      const r255 = Math.min(255, Math.round(starsRaw[base + 3] * 255));
      const g255 = Math.min(255, Math.round(starsRaw[base + 4] * 255));
      const b255 = Math.min(255, Math.round(starsRaw[base + 5] * 255));
      const pt = projectToHemisphere(raRad, decDeg, rotRad, isNorth, cx, cy, R);

      const radius =
        mag <= 0.5
          ? 3.35 - 0.35 * mag
          : mag <= 2.5
          ? 2.65 - 0.42 * (mag - 0.5)
          : Math.max(0.58, 1.81 - 0.34 * (mag - 2.5));
      const alpha =
        mag <= 2.5
          ? 0.98
          : Math.max(0.28, Math.min(0.92, 0.92 - 0.175 * (mag - 2.5)));

      if (mag <= 1.85) {
        const glowR = radius * 3.1;
        const grad = ctx.createRadialGradient(pt.x, pt.y, radius * 0.25, pt.x, pt.y, glowR);
        grad.addColorStop(0.0, `rgba(${r255}, ${g255}, ${b255}, 0.55)`);
        grad.addColorStop(1.0, `rgba(${r255}, ${g255}, ${b255}, 0.0)`);
        ctx.fillStyle = grad;
        ctx.beginPath();
        ctx.arc(pt.x, pt.y, glowR, 0, Math.PI * 2);
        ctx.fill();
      }

      ctx.fillStyle = `rgba(${r255}, ${g255}, ${b255}, ${alpha})`;
      ctx.beginPath();
      ctx.arc(pt.x, pt.y, radius, 0, Math.PI * 2);
      ctx.fill();

      if (mag <= 0.65) {
        const spk = radius * 2.35;
        ctx.strokeStyle = `rgba(${r255}, ${g255}, ${b255}, 0.55)`;
        ctx.lineWidth = 0.75;
        ctx.beginPath();
        ctx.moveTo(pt.x - spk, pt.y);
        ctx.lineTo(pt.x + spk, pt.y);
        ctx.moveTo(pt.x, pt.y - spk);
        ctx.lineTo(pt.x, pt.y + spk);
        ctx.stroke();
      }
    }

    // Naked-eye Solar System planets along the ecliptic:
    if (Array.isArray(meta.planets)) {
      for (const pl of meta.planets) {
        const inHemi = isNorth ? pl.decDeg >= 0.0 : pl.decDeg < 0.0;
        if (!inHemi) continue;
        const pp = projectToHemisphere(pl.raRad, pl.decDeg, rotRad, isNorth, cx, cy, R);
        const pr = Math.round((pl.rgb?.[0] ?? 0.95) * 255);
        const pg = Math.round((pl.rgb?.[1] ?? 0.82) * 255);
        const pb = Math.round((pl.rgb?.[2] ?? 0.58) * 255);
        ctx.fillStyle = `rgb(${pr}, ${pg}, ${pb})`;
        ctx.strokeStyle = "rgba(251, 191, 36, 0.88)";
        ctx.lineWidth = 1.1;
        ctx.beginPath();
        ctx.arc(pp.x, pp.y, 3.1, 0, Math.PI * 2);
        ctx.fill();
        ctx.stroke();
        ctx.font = "600 8.5px ui-monospace, SFMono-Regular, Menlo, monospace";
        ctx.fillStyle = "rgba(253, 230, 138, 0.92)";
        ctx.fillText(`P${pl.number}`, pp.x + 9, pp.y - 5);
      }
    }

    // Constellation labels (`C1..CN`):
    if (view.showConstellations && Array.isArray(meta.constellations)) {
      ctx.font = "700 9.5px ui-monospace, SFMono-Regular, Menlo, monospace";
      for (const c of meta.constellations) {
        const inHemi = isNorth ? c.decDeg >= 0.0 : c.decDeg < 0.0;
        if (!inHemi) continue;
        const cp = projectToHemisphere(c.raRad, c.decDeg, rotRad, isNorth, cx, cy, R);
        const lx = Math.max(cx - R + 16, Math.min(cx + R - 16, cp.x));
        const ly = Math.max(cy - R + 12, Math.min(cy + R - 12, cp.y - 8));
        const isSel = c.index === selConstIdx;
        const label = `C${c.number}`;
        ctx.fillStyle = isSel ? "rgba(6, 78, 59, 0.85)" : "rgba(10, 16, 34, 0.72)";
        const tw = ctx.measureText(label).width + 6;
        ctx.fillRect(lx - tw * 0.5, ly - 6, tw, 12);
        ctx.fillStyle = isSel ? "#6ee7b7" : "rgba(147, 197, 253, 0.92)";
        ctx.fillText(label, lx, ly + 0.5);
      }
    }

    // Declination tick labels (+60°, +30° / -60°, -30°):
    ctx.font = "500 8.5px ui-monospace, SFMono-Regular, Menlo, monospace";
    ctx.fillStyle = "rgba(148, 163, 184, 0.62)";
    ctx.fillText(isNorth ? "+60°" : "−60°", cx, cy - R / 3.0 + 8);
    ctx.fillText(isNorth ? "+30°" : "−30°", cx, cy - (2.0 * R) / 3.0 + 8);

    // Selected star reticle highlight:
    if (selStarIdx >= 0 && selStarIdx * 6 + 5 < starsRaw.length) {
      const sRa = starsRaw[selStarIdx * 6];
      const sDec = starsRaw[selStarIdx * 6 + 1];
      if (isNorth ? sDec >= -2.0 : sDec <= 2.0) {
        const sp = projectToHemisphere(sRa, sDec, rotRad, isNorth, cx, cy, R);
        ctx.strokeStyle = "#34d399";
        ctx.lineWidth = 1.6;
        ctx.beginPath();
        ctx.arc(sp.x, sp.y, 7.5, 0, Math.PI * 2);
        ctx.stroke();
        ctx.beginPath();
        ctx.moveTo(sp.x - 11, sp.y);
        ctx.lineTo(sp.x - 5, sp.y);
        ctx.moveTo(sp.x + 5, sp.y);
        ctx.lineTo(sp.x + 11, sp.y);
        ctx.moveTo(sp.x, sp.y - 11);
        ctx.lineTo(sp.x, sp.y - 5);
        ctx.moveTo(sp.x, sp.y + 5);
        ctx.lineTo(sp.x, sp.y + 11);
        ctx.stroke();
      }
    }

    ctx.restore(); // End hemisphere clip

    // Crisp Celestial Equator rim & Right Ascension hour ticks/labels around bezel:
    ctx.strokeStyle = "rgba(148, 163, 184, 0.68)";
    ctx.lineWidth = 1.3;
    ctx.beginPath();
    ctx.arc(cx, cy, R, 0, Math.PI * 2);
    ctx.stroke();

    ctx.font = "600 9px ui-monospace, SFMono-Regular, Menlo, monospace";
    ctx.fillStyle = "#cbd5e1";
    for (let h = 0; h < 24; h++) {
      const ra = (h * 15.0 * Math.PI) / 180.0;
      const p0 = projectToHemisphere(ra, 0.0, rotRad, isNorth, cx, cy, R);
      const tickLen = h % 2 === 0 ? 4.5 : 2.5;
      const p1 = projectToHemisphere(ra, 0.0, rotRad, isNorth, cx, cy, R + tickLen);
      ctx.strokeStyle =
        h % 2 === 0 ? "rgba(203, 213, 225, 0.65)" : "rgba(148, 163, 184, 0.35)";
      ctx.lineWidth = 1.0;
      ctx.beginPath();
      ctx.moveTo(p0.x, p0.y);
      ctx.lineTo(p1.x, p1.y);
      ctx.stroke();

      if (h % 2 === 0) {
        const pl = projectToHemisphere(ra, 0.0, rotRad, isNorth, cx, cy, R + 10.5);
        ctx.fillText(`${h}h`, pl.x, pl.y);
      }
    }
  };

  drawSingleHemisphere(true, cxN, cyN, cache.northGlow);
  drawSingleHemisphere(false, cxS, cyS, cache.southGlow);

  // Bottom Legend Bar (`y = 398..424`):
  ctx.fillStyle = "rgba(15, 23, 42, 0.78)";
  ctx.strokeStyle = "rgba(148, 163, 184, 0.22)";
  ctx.lineWidth = 1.0;
  ctx.beginPath();
  if (ctx.roundRect) {
    ctx.roundRect(18, H - 32, W - 36, 24, 6);
  } else {
    ctx.rect(18, H - 32, W - 36, 24);
  }
  ctx.fill();
  ctx.stroke();

  ctx.textAlign = "left";
  ctx.font = "500 10px Inter, system-ui, sans-serif";
  let lx = 30;
  const ly = H - 20;
  ctx.fillStyle = "#94a3b8";
  ctx.fillText("Mag:", lx, ly);
  lx += 30;
  for (const [mLabel, rad] of [
    ["0", 3.3],
    ["2", 2.0],
    ["4", 1.25],
    ["6", 0.7],
  ]) {
    ctx.fillStyle = "#f8fafc";
    ctx.beginPath();
    ctx.arc(lx + rad, ly - 0.5, rad, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = "#cbd5e1";
    ctx.fillText(mLabel, lx + rad * 2 + 3, ly);
    lx += rad * 2 + 17;
  }

  lx += 10;
  ctx.strokeStyle = "rgba(96, 165, 250, 0.85)";
  ctx.lineWidth = 1.6;
  ctx.beginPath();
  ctx.moveTo(lx, ly);
  ctx.lineTo(lx + 16, ly);
  ctx.stroke();
  ctx.fillStyle = "#cbd5e1";
  ctx.fillText(`Constellations (${meta.constellationCount || 0})`, lx + 21, ly);
  lx += 132;

  ctx.save();
  ctx.setLineDash([4, 3]);
  ctx.strokeStyle = "rgba(56, 189, 248, 0.85)";
  ctx.beginPath();
  ctx.moveTo(lx, ly);
  ctx.lineTo(lx + 16, ly);
  ctx.stroke();
  ctx.restore();
  ctx.fillText("Galactic Plane", lx + 21, ly);
  lx += 108;

  ctx.save();
  ctx.setLineDash([3, 3]);
  ctx.strokeStyle = "rgba(251, 191, 36, 0.85)";
  ctx.beginPath();
  ctx.moveTo(lx, ly);
  ctx.lineTo(lx + 16, ly);
  ctx.stroke();
  ctx.restore();
  ctx.fillText("Ecliptic & Planets", lx + 21, ly);

  ctx.textAlign = "right";
  ctx.fillStyle = "#93c5fd";
  ctx.fillText(
    meta.isRealHipparcos
      ? `Sol · Hipparcos (${meta.starCount} stars)`
      : `${view.session.galaxy_type_name()} (${meta.starCount} stars)`,
    W - 28,
    ly
  );

  ctx.restore();
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

  if (view.aspect === "star_map") {
    drawDualHemisphereStarMap(view);
    return;
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
    view.aspect === "star_map"
      ? "2D Dual-Hemisphere Star Map: Left = Northern Celestial Hemisphere (+90° pole at center, 0° equator at rim), Right = Southern Celestial Hemisphere (−90° pole at center, 0° equator at rim). Drag the rotation slider to spin the planisphere dials, or click any star/constellation to inspect it."
      : view.aspect === "sky_plants"
      ? "2D Exoplanet Landscape: Left/Top = Daytime Sky & Home Sun, Right Horizon = Sunset & Twilight Glow over the Ocean, Foreground = Rolling Hills & Photosynthetic Plant Foliage. Click the sky, sunset, or plants to inspect their optical physics."
      : view.aspect === "galaxy"
      ? "3D Host Galaxy View: Shows the full 3D galaxy star cloud (tilted 38°), with a bright green 3D crosshair & orbit circle marking your Home Star's galactocentric location. Click or rotate to inspect."
      : view.aspect === "solar_system"
      ? "3D Planetary System View: Central Home Star with concentric 3D planet orbits (green target crosshair marks the simulated Home World; ringed planets show 3D ring halos). Click near any planet to inspect it."
      : view.aspect === "home_sun"
      ? "3D Home Sun Photosphere: Scaled to true solar radius (dashed blue circle = 1.00 R☉ Sol reference) in its true B-V blackbody color. Click the photosphere to inspect."
      : "Click any star on the celestial sphere to inspect its catalog ID, spectral class, apparent & absolute magnitude, and distance in light-years.";
  const selectedReport =
    view.aspect === "star_map"
      ? typeof view.selectedStarIdx === "number" && view.selectedStarIdx >= 0
        ? s.inspect_star_index(view.selectedStarIdx)
        : defaultPrompt
      : view.selectedTileIdx >= 0
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

    const starMapCanvas = document.getElementById("sun-starmap-canvas");
    view.starMapCanvas = starMapCanvas;
    view.starMapCache = null;
    view.selectedStarIdx = -1;
    const basePaint = view.paint.bind(view);
    view.paint = function () {
      if (this.aspect === "star_map") {
        drawDualHemisphereStarMap(this);
      } else {
        basePaint();
      }
    };

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

    const handleStarMapClick = (ev) => {
      if (!view.session || !starMapCanvas) return;
      const rect = starMapCanvas.getBoundingClientRect();
      if (rect.width <= 0 || rect.height <= 0) return;
      const mx = ((ev.clientX - rect.left) / rect.width) * STAR_MAP_LAYOUT.W;
      const my = ((ev.clientY - rect.top) / rect.height) * STAR_MAP_LAYOUT.H;
      const { cxN, cyN, cxS, cyS, R } = STAR_MAP_LAYOUT;
      const rotRad = view.rotationAngleRad || 0.0;

      const dxN = mx - cxN;
      const dyN = my - cyN;
      const distN = Math.hypot(dxN, dyN);
      const dxS = mx - cxS;
      const dyS = my - cyS;
      const distS = Math.hypot(dxS, dyS);

      let raRad = null;
      let decDeg = null;
      if (distN <= R + 5) {
        const rFrac = Math.min(1.0, distN / R);
        decDeg = 90.0 * (1.0 - rFrac);
        const phi = Math.atan2(dxN, -dyN);
        raRad = ((phi - rotRad) % (Math.PI * 2.0) + Math.PI * 2.0) % (Math.PI * 2.0);
      } else if (distS <= R + 5) {
        const rFrac = Math.min(1.0, distS / R);
        decDeg = -90.0 * (1.0 - rFrac);
        const phi = Math.atan2(-dxS, -dyS);
        raRad = ((phi - rotRad) % (Math.PI * 2.0) + Math.PI * 2.0) % (Math.PI * 2.0);
      }

      if (raRad !== null && decDeg !== null) {
        const hitIdx = view.session.find_nearest_star_ra_dec(raRad, decDeg);
        if (hitIdx >= 0) {
          view.selectedStarIdx = hitIdx;
          drawDualHemisphereStarMap(view);
          view.setSunInfo(buildSunInfo(view))();
        }
      }
    };

    canvas.addEventListener("click", handleCanvasClick);
    if (starMapCanvas) {
      starMapCanvas.addEventListener("click", handleStarMapClick);
    }

    activeView = view;
    updateSunDataBuffers(view, true);
    setSunInfo(buildSunInfo(view))();

    return () => {
      canvas.removeEventListener("click", handleCanvasClick);
      if (starMapCanvas) {
        starMapCanvas.removeEventListener("click", handleStarMapClick);
      }
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
    activeView.selectedStarIdx = -1;
    activeView.starMapCache = null;
    updateSunDataBuffers(activeView, false);
    setSunInfo(buildSunInfo(activeView))();
  };

export const chooseNewSunInGalaxy = (starFilter) => (setSunInfo) => () => {
  if (!activeView || !activeView.session) return;
  activeView.session.choose_sun_in_current_galaxy(starFilter);
  activeView.selectedTileIdx = -1;
  activeView.selectedStarIdx = -1;
  activeView.starMapCache = null;
  updateSunDataBuffers(activeView, false);
  setSunInfo(buildSunInfo(activeView))();
};

export const loadHipparcosIntoSun = (setSunInfo) => () => {
  if (!activeView || !activeView.session) return;
  activeView.session.load_real_hipparcos_sol();
  activeView.selectedTileIdx = -1;
  activeView.selectedStarIdx = -1;
  activeView.starMapCache = null;
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

// ============================================================================
// Mode 19: Dynamic Astronomical Tides on Full-Sphere Coarse+2 Grid (`TidesScreen`)
// ============================================================================

function expandFineTileColorsToVertexAttr(view, tileColors) {
  const ids = view.vertexTileIds;
  if (!ids || !tileColors) return;
  const needed = ids.length * 3;
  if (!view.vertexColorBuffer || view.vertexColorBuffer.length !== needed) {
    view.vertexColorBuffer = new Float32Array(needed);
    view.vertexColorAttr = new THREE.BufferAttribute(view.vertexColorBuffer, 3);
    view.geometry.setAttribute("color", view.vertexColorAttr);
  }
  const buf = view.vertexColorBuffer;
  for (let i = 0; i < ids.length; i++) {
    const src = ids[i] * 3;
    const dst = i * 3;
    buf[dst] = tileColors[src];
    buf[dst + 1] = tileColors[src + 1];
    buf[dst + 2] = tileColors[src + 2];
  }
  view.vertexColorAttr.needsUpdate = true;
}

function updateTidesDataBuffers(view, rebuildGeometry = false) {
  const { session, geometry } = view;
  if (!session) return;
  if (rebuildGeometry) {
    const positions = session.mesh_positions();
    const normals = session.mesh_normals();
    view.vertexTileIds = session.vertex_tile_ids();
    view.vertexColorBuffer = null;
    view.vertexColorAttr = null;
    geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
    geometry.setAttribute("normal", new THREE.Float32BufferAttribute(normals, 3));
    geometry.computeBoundingSphere();
  }

  const tileColors = session.tile_colors_rgb(view.aspect || "live_eta", view.selectedTileIdx);
  expandFineTileColorsToVertexAttr(view, tileColors);

  if (view.showArrows) {
    const arrowPos = session.tidal_arrow_positions();
    view.arrowLines.visible = arrowPos.length > 0;
    view.arrowGeometry.setAttribute("position", new THREE.Float32BufferAttribute(arrowPos, 3));
  } else {
    view.arrowLines.visible = false;
  }
  view.paint();
}

function buildTidesInfo(view) {
  const s = view.session;
  const bundle = buildHistoryInspectorBundle(view);
  let sparkCurrentLabel = "η = +0.00 m";
  let sparkSubtitle = "Step +1h / Play to record 48h tide gauge";
  let sparkMinLabel = "Low: 0.00 m";
  let sparkMaxLabel = "High: 0.00 m";
  let selectedReport =
    "Click any fine ocean, continental shelf, or coastal land tile on the globe to inspect dynamic Laplace tides, 48-hour tide gauge, amphidromic range, coastal wetting/drying, and synced local sky & 100° horizon vantage.";

  if (view.selectedTileIdx >= 0 && bundle.tileView.hasTile) {
    const tv = bundle.tileView;
    const floraLine = tv.flora && tv.flora.length > 0 ? `\nNative Flora: ${tv.flora.join(", ")}` : "";
    const faunaLine = tv.fauna && tv.fauna.length > 0 ? `\nNative Fauna: ${tv.fauna.join(", ")}` : "";
    selectedReport = `${tv.title} (${tv.skyLatLonSummary})\n${tv.locationSummary}\n${tv.populationSummary} | ${tv.technology}${floraLine}${faunaLine}`;
    try {
      const parsedSpark = JSON.parse(s.tile_tide_series_json(view.selectedTileIdx));
      sparkCurrentLabel = parsedSpark.currentLabel || sparkCurrentLabel;
      sparkSubtitle = parsedSpark.subtitle || sparkSubtitle;
      sparkMinLabel = parsedSpark.minLabel || sparkMinLabel;
      sparkMaxLabel = parsedSpark.maxLabel || sparkMaxLabel;
      if (typeof window !== "undefined") {
        const series = parsedSpark.series || [];
        window.requestAnimationFrame(() =>
          paintSparklineCanvas("tides-sparkline-canvas", series, { windowHours: 48 })
        );
        setTimeout(
          () => paintSparklineCanvas("tides-sparkline-canvas", series, { windowHours: 48 }),
          45
        );
      }
    } catch (_) {}
  } else {
    scheduleStratigraphySidePaint(view, -1);
  }

  return {
    planetName: view.loadedFileName ? `File: ${view.loadedFileName}` : s.planet_name(),
    coarseGridSize: s.coarse_grid_size(),
    fineGridSize: s.fine_grid_size(),
    tileCount: s.tile_count(),
    elapsedHours: s.elapsed_hours(),
    dayOfYear: s.sim_day_of_year(),
    daysPerYear: s.days_per_year(),
    clockLabel: s.sim_clock_label(),
    moonCount: s.moon_count(),
    moonMassTenths: Math.max(1, Math.round(s.moon_mass() * 10.0)),
    moonDistRadii: Math.max(20, Math.round(s.moon_dist_radii())),
    maxTideText: `${s.max_tide_m().toFixed(2)} m`,
    maxRangeText: `${s.max_tidal_range_m().toFixed(2)} m`,
    maxCurrentText: `${s.max_current_mps().toFixed(2)} m/s`,
    inundatedTiles: s.inundated_land_tiles(),
    isRunning: view.timerId !== null,
    hasSavedPlanet: getSavedSnapshotBytes() !== null,
    sparkCurrentLabel,
    sparkSubtitle,
    sparkMinLabel,
    sparkMaxLabel,
    selectedReport,
    tileView: bundle.tileView,
  };
}

function stopTidesTimer(view) {
  if (view && view.timerId !== null) {
    clearInterval(view.timerId);
    view.timerId = null;
  }
}

export const mountTidesView =
  (moonCount) =>
  (moonMassTenths) =>
  (moonDistRadii) =>
  (dayOfYear) =>
  (aspect) =>
  (showArrows) =>
  (setTidesInfo) =>
  () => {
    const canvas = document.getElementById("sphere-canvas");
    if (!canvas) {
      return () => {};
    }

    const view = createBaseSphereView(canvas, false);
    view.material.dispose();
    view.material = new THREE.MeshBasicMaterial({
      side: THREE.FrontSide,
      vertexColors: true,
    });
    view.mesh.material = view.material;
    view.unlit = true;
    view.arrowStrokeStyle = "rgba(224, 242, 254, 0.85)";

    if (activeClimateSnapshotBytes) {
      view.session = WasmTidesSession.from_snapshot_or_earth(activeClimateSnapshotBytes);
      view.session.set_day_of_year(dayOfYear);
    } else {
      view.session = new WasmTidesSession();
      view.session.set_day_of_year(dayOfYear);
      view.session.set_moon_params(moonCount, moonMassTenths / 10.0, moonDistRadii);
    }

    view.aspect = aspect;
    view.showArrows = showArrows;
    view.selectedTileIdx = -1;
    view.inspectorMode = "tides";
    view.timerId = null;
    view.loadedFileName = null;
    view.setTidesInfo = setTidesInfo;

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
        updateTidesDataBuffers(view, false);
        view.setTidesInfo(buildTidesInfo(view))();
      }
    };

    canvas.addEventListener("click", handleCanvasClick);

    activeView = view;
    updateTidesDataBuffers(view, true);
    setTidesInfo(buildTidesInfo(view))();

    return () => {
      stopTidesTimer(view);
      canvas.removeEventListener("click", handleCanvasClick);
      if (activeView === view) {
        activeView = null;
      }
      view.dispose();
      view.session.free();
    };
  };

export const stepTidesHours = (hours) => (setTidesInfo) => () => {
  if (!activeView || !activeView.session) return;
  activeView.session.step_hours(Math.max(1, hours | 0));
  updateTidesDataBuffers(activeView, false);
  setTidesInfo(buildTidesInfo(activeView))();
};

export const toggleTidesRunning = (setTidesInfo) => () => {
  if (!activeView || !activeView.session) return;
  if (activeView.timerId !== null) {
    stopTidesTimer(activeView);
  } else {
    const viewRef = activeView;
    viewRef.timerId = setInterval(() => {
      if (activeView !== viewRef || !viewRef.session) {
        stopTidesTimer(viewRef);
        return;
      }
      viewRef.session.step_hours(1);
      updateTidesDataBuffers(viewRef, false);
      setTidesInfo(buildTidesInfo(viewRef))();
    }, 150);
  }
  setTidesInfo(buildTidesInfo(activeView))();
};

export const resetTidesSim = (setTidesInfo) => () => {
  if (!activeView || !activeView.session) return;
  stopTidesTimer(activeView);
  activeView.session.reset_tides();
  updateTidesDataBuffers(activeView, false);
  setTidesInfo(buildTidesInfo(activeView))();
};

export const setTidesDayOfYear = (day) => (setTidesInfo) => () => {
  if (!activeView || !activeView.session) return;
  activeView.session.set_day_of_year(Math.max(0, day | 0));
  updateTidesDataBuffers(activeView, false);
  setTidesInfo(buildTidesInfo(activeView))();
};

export const setTidesMoonParams =
  (count) => (massTenths) => (distRadii) => (setTidesInfo) => () => {
    if (!activeView || !activeView.session) return;
    activeView.session.set_moon_params(
      Math.max(0, count | 0),
      Math.max(0.1, massTenths / 10.0),
      Math.max(20.0, distRadii)
    );
    updateTidesDataBuffers(activeView, false);
    setTidesInfo(buildTidesInfo(activeView))();
  };

export const setTidesDisplayOptions = (aspect) => (showArrows) => () => {
  if (!activeView || !activeView.session) return;
  activeView.aspect = aspect;
  activeView.showArrows = showArrows;
  updateTidesDataBuffers(activeView, false);
};

export const loadEarthIntoTides = (setTidesInfo) => () => {
  if (!activeView || !activeView.session) return;
  stopTidesTimer(activeView);
  activeClimateSnapshotBytes = null;
  activeView.loadedFileName = null;
  activeView.selectedTileIdx = -1;
  activeView.session.load_earth();
  updateTidesDataBuffers(activeView, true);
  setTidesInfo(buildTidesInfo(activeView))();
};

export const loadSavedPlanetIntoTides = (setTidesInfo) => () => {
  if (!activeView || !activeView.session) return;
  const saved = getSavedSnapshotBytes();
  if (!saved) return;
  stopTidesTimer(activeView);
  if (activeView.session.load_snapshot_bytes(saved)) {
    activeClimateSnapshotBytes = saved;
    activeView.loadedFileName = null;
    activeView.selectedTileIdx = -1;
    updateTidesDataBuffers(activeView, true);
    setTidesInfo(buildTidesInfo(activeView))();
  }
};

export const loadFilePlanetIntoTides = (setTidesInfo) => () => {
  if (!activeView || !activeView.session) return;
  pickBinarySnapshotFile((bytes, filename) => {
    if (!activeView || !activeView.session) return;
    stopTidesTimer(activeView);
    if (activeView.session.load_snapshot_bytes(bytes)) {
      storeSavedSnapshotBytes(bytes);
      activeClimateSnapshotBytes = bytes;
      activeView.loadedFileName = filename || null;
      activeView.selectedTileIdx = -1;
      updateTidesDataBuffers(activeView, true);
      setTidesInfo(buildTidesInfo(activeView))();
    }
  });
};

// ============================================================================
// Mode 20: Global Coupled Weather & Storm Surge on Full-Sphere Coarse+2 Grid (`WeatherScreen`)
// ============================================================================

function applyWeatherArrowColor(view, arrowMode) {
  if (arrowMode === "jet_stream") {
    view.arrowMaterial.color.setHex(0xfde047);
    view.arrowStrokeStyle = "rgba(253, 224, 71, 0.90)";
  } else if (arrowMode === "tidal_current") {
    view.arrowMaterial.color.setHex(0x67e8f9);
    view.arrowStrokeStyle = "rgba(103, 232, 249, 0.88)";
  } else {
    view.arrowMaterial.color.setHex(0xe0f2fe);
    view.arrowStrokeStyle = "rgba(224, 242, 254, 0.84)";
  }
}

function updateWeatherDataBuffers(view, rebuildGeometry = false) {
  const { session, geometry } = view;
  if (!session) return;
  if (rebuildGeometry) {
    const positions = session.mesh_positions();
    const normals = session.mesh_normals();
    view.vertexTileIds = session.vertex_tile_ids();
    view.vertexColorBuffer = null;
    view.vertexColorAttr = null;
    geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
    geometry.setAttribute("normal", new THREE.Float32BufferAttribute(normals, 3));
    geometry.computeBoundingSphere();
  }

  const tileColors = session.tile_colors_rgb(view.aspect || "satellite", view.selectedTileIdx);
  expandFineTileColorsToVertexAttr(view, tileColors);

  const arrowMode = view.arrowMode || "none";
  if (arrowMode !== "none") {
    applyWeatherArrowColor(view, arrowMode);
    const arrowPos = session.weather_arrow_positions(arrowMode);
    view.arrowLines.visible = arrowPos.length > 0;
    const existingArrowAttr = view.arrowGeometry.getAttribute("position");
    if (existingArrowAttr && existingArrowAttr.array.length === arrowPos.length) {
      existingArrowAttr.array.set(arrowPos);
      existingArrowAttr.needsUpdate = true;
    } else {
      view.arrowGeometry.setAttribute("position", new THREE.Float32BufferAttribute(arrowPos, 3));
    }
  } else {
    view.arrowLines.visible = false;
  }
  view.paint();
}

function buildWeatherInfo(view) {
  const s = view.session;
  const bundle = buildHistoryInspectorBundle(view);
  let sparkCurrentLabel = "+15.0 °C · 1013 hPa · 0.00 mm/h";
  let sparkSubtitle = "Step +1h / Play to record 48h meteogram";
  let sparkMinLabel = "Low: +15.0 °C";
  let sparkMaxLabel = "High: +15.0 °C";
  let selectedReport =
    "Click any fine tile on the globe to inspect live temperature, sea-level pressure, surface & 300 hPa jet winds, cloud decks, rain/snowfall, snowpack, river discharge, coastal storm surge, 48-hour meteogram, and synced local sky & 100° horizon vantage.";

  if (view.selectedTileIdx >= 0 && bundle.tileView.hasTile) {
    const tv = bundle.tileView;
    const floraLine = tv.flora && tv.flora.length > 0 ? `\nNative Flora: ${tv.flora.join(", ")}` : "";
    const faunaLine = tv.fauna && tv.fauna.length > 0 ? `\nNative Fauna: ${tv.fauna.join(", ")}` : "";
    selectedReport = `${tv.title} (${tv.skyLatLonSummary})\n${tv.locationSummary}\n${tv.populationSummary} | ${tv.technology}${floraLine}${faunaLine}`;
    try {
      const parsedSpark = JSON.parse(s.tile_meteogram_json(view.selectedTileIdx));
      sparkCurrentLabel = parsedSpark.currentLabel || sparkCurrentLabel;
      sparkSubtitle = parsedSpark.subtitle || sparkSubtitle;
      sparkMinLabel = parsedSpark.minLabel || sparkMinLabel;
      sparkMaxLabel = parsedSpark.maxLabel || sparkMaxLabel;
      if (typeof window !== "undefined") {
        const series = parsedSpark.series || [];
        const legend = parsedSpark.legend || [];
        window.requestAnimationFrame(() =>
          paintSparklineCanvas("weather-meteogram-canvas", series, { windowHours: 48, legend })
        );
        setTimeout(
          () => paintSparklineCanvas("weather-meteogram-canvas", series, { windowHours: 48, legend }),
          45
        );
      }
    } catch (_) {}
  } else {
    scheduleStratigraphySidePaint(view, -1);
  }

  const meanT = s.global_mean_temp_c();
  const minP = s.min_slp_hpa();
  const maxP = s.max_slp_hpa();

  return {
    planetName: view.loadedFileName ? `File: ${view.loadedFileName}` : s.planet_name(),
    coarseGridSize: s.coarse_grid_size(),
    fineGridSize: s.fine_grid_size(),
    tileCount: s.tile_count(),
    elapsedHours: s.elapsed_hours(),
    dayOfYear: s.sim_day_of_year(),
    daysPerYear: s.days_per_year(),
    clockLabel: s.sim_clock_label(),
    atmosphereId: s.atmosphere_id(),
    atmosphereName: s.atmosphere_name(),
    glaciationPct: Math.max(0, Math.min(100, Math.round(s.glaciation() * 100.0))),
    moonCount: s.moon_count(),
    moonMassTenths: Math.max(1, Math.round(s.moon_mass() * 10.0)),
    moonDistRadii: Math.max(20, Math.round(s.moon_dist_radii())),
    meanTempText: `${meanT >= 0 ? "+" : ""}${meanT.toFixed(1)} °C`,
    slpRangeText: `${Math.round(minP)}..${Math.round(maxP)} hPa`,
    maxWindText: `${s.max_wind_mps().toFixed(1)} m/s`,
    maxJetText: `${s.max_jet_mps().toFixed(1)} m/s`,
    maxPrecipText: `${s.max_precip_mm_hr().toFixed(2)} mm/h`,
    maxSurgeText: `${s.max_storm_surge_m().toFixed(2)} m`,
    inundatedTiles: s.inundated_land_tiles(),
    isRunning: view.timerId !== null,
    hasSavedPlanet: getSavedSnapshotBytes() !== null,
    sparkCurrentLabel,
    sparkSubtitle,
    sparkMinLabel,
    sparkMaxLabel,
    selectedReport,
    tileView: bundle.tileView,
  };
}

function stopWeatherTimer(view) {
  if (view && view.timerId !== null) {
    clearTimeout(view.timerId);
    view.timerId = null;
  }
}

export const mountWeatherView =
  (atmId) =>
  (glacPct) =>
  (moonCount) =>
  (moonMassTenths) =>
  (moonDistRadii) =>
  (dayOfYear) =>
  (aspect) =>
  (arrowMode) =>
  (setWeatherInfo) =>
  () => {
    const canvas = document.getElementById("sphere-canvas");
    if (!canvas) {
      return () => {};
    }

    const view = createBaseSphereView(canvas, false);
    view.material.dispose();
    view.material = new THREE.MeshBasicMaterial({
      side: THREE.FrontSide,
      vertexColors: true,
    });
    view.mesh.material = view.material;
    view.unlit = true;

    if (activeClimateSnapshotBytes) {
      view.session = WasmWeatherSession.from_snapshot_or_earth(activeClimateSnapshotBytes);
      view.session.set_day_of_year(dayOfYear);
    } else {
      view.session = new WasmWeatherSession();
      view.session.set_day_of_year(dayOfYear);
      view.session.set_environment_params(
        Math.max(0, atmId | 0),
        Math.max(0.0, Math.min(1.0, glacPct / 100.0)),
        Math.max(0, moonCount | 0),
        Math.max(0.1, moonMassTenths / 10.0),
        Math.max(20.0, moonDistRadii)
      );
    }

    view.aspect = aspect;
    view.arrowMode = arrowMode;
    view.selectedTileIdx = -1;
    view.inspectorMode = "weather";
    view.timerId = null;
    view.loadedFileName = null;
    view.setWeatherInfo = setWeatherInfo;

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
        updateWeatherDataBuffers(view, false);
        view.setWeatherInfo(buildWeatherInfo(view))();
      }
    };

    canvas.addEventListener("click", handleCanvasClick);

    activeView = view;
    updateWeatherDataBuffers(view, true);
    setWeatherInfo(buildWeatherInfo(view))();

    return () => {
      stopWeatherTimer(view);
      canvas.removeEventListener("click", handleCanvasClick);
      if (activeView === view) {
        activeView = null;
      }
      view.dispose();
      view.session.free();
    };
  };

export const stepWeatherHours = (hours) => (setWeatherInfo) => () => {
  if (!activeView || !activeView.session) return;
  activeView.session.step_hours(Math.max(1, hours | 0));
  updateWeatherDataBuffers(activeView, false);
  setWeatherInfo(buildWeatherInfo(activeView))();
};

export const toggleWeatherRunning = (setWeatherInfo) => () => {
  if (!activeView || !activeView.session) return;
  if (activeView.timerId !== null) {
    stopWeatherTimer(activeView);
  } else {
    const viewRef = activeView;
    const scheduleNextTick = (delayMs) => {
      viewRef.timerId = setTimeout(() => {
        if (activeView !== viewRef || !viewRef.session) {
          stopWeatherTimer(viewRef);
          return;
        }
        const t0 = typeof performance !== "undefined" ? performance.now() : Date.now();
        viewRef.session.step_hours(1);
        updateWeatherDataBuffers(viewRef, false);
        setWeatherInfo(buildWeatherInfo(viewRef))();
        const elapsed = (typeof performance !== "undefined" ? performance.now() : Date.now()) - t0;
        if (activeView === viewRef && viewRef.timerId !== null) {
          scheduleNextTick(Math.max(16, Math.round(180 - elapsed)));
        }
      }, delayMs);
    };
    scheduleNextTick(16);
  }
  setWeatherInfo(buildWeatherInfo(activeView))();
};

export const resetWeatherSim = (setWeatherInfo) => () => {
  if (!activeView || !activeView.session) return;
  stopWeatherTimer(activeView);
  activeView.session.reset_weather();
  updateWeatherDataBuffers(activeView, false);
  setWeatherInfo(buildWeatherInfo(activeView))();
};

export const setWeatherDayOfYear = (day) => (setWeatherInfo) => () => {
  if (!activeView || !activeView.session) return;
  activeView.session.set_day_of_year(Math.max(0, day | 0));
  updateWeatherDataBuffers(activeView, false);
  setWeatherInfo(buildWeatherInfo(activeView))();
};

export const setWeatherEnvParams =
  (atmId) =>
  (glacPct) =>
  (moonCount) =>
  (moonMassTenths) =>
  (moonDistRadii) =>
  (setWeatherInfo) =>
  () => {
    if (!activeView || !activeView.session) return;
    activeView.session.set_environment_params(
      Math.max(0, atmId | 0),
      Math.max(0.0, Math.min(1.0, glacPct / 100.0)),
      Math.max(0, moonCount | 0),
      Math.max(0.1, moonMassTenths / 10.0),
      Math.max(20.0, moonDistRadii)
    );
    updateWeatherDataBuffers(activeView, false);
    setWeatherInfo(buildWeatherInfo(activeView))();
  };

export const setWeatherDisplayOptions = (aspect) => (arrowMode) => () => {
  if (!activeView || !activeView.session) return;
  activeView.aspect = aspect;
  activeView.arrowMode = arrowMode;
  updateWeatherDataBuffers(activeView, false);
};

export const loadEarthIntoWeather = (setWeatherInfo) => () => {
  if (!activeView || !activeView.session) return;
  stopWeatherTimer(activeView);
  activeClimateSnapshotBytes = null;
  activeView.loadedFileName = null;
  activeView.selectedTileIdx = -1;
  activeView.session.load_earth();
  updateWeatherDataBuffers(activeView, true);
  setWeatherInfo(buildWeatherInfo(activeView))();
};

export const loadSavedPlanetIntoWeather = (setWeatherInfo) => () => {
  if (!activeView || !activeView.session) return;
  const saved = getSavedSnapshotBytes();
  if (!saved) return;
  stopWeatherTimer(activeView);
  if (activeView.session.load_snapshot_bytes(saved)) {
    activeClimateSnapshotBytes = saved;
    activeView.loadedFileName = null;
    activeView.selectedTileIdx = -1;
    updateWeatherDataBuffers(activeView, true);
    setWeatherInfo(buildWeatherInfo(activeView))();
  }
};

export const loadFilePlanetIntoWeather = (setWeatherInfo) => () => {
  if (!activeView || !activeView.session) return;
  pickBinarySnapshotFile((bytes, filename) => {
    if (!activeView || !activeView.session) return;
    stopWeatherTimer(activeView);
    if (activeView.session.load_snapshot_bytes(bytes)) {
      storeSavedSnapshotBytes(bytes);
      activeClimateSnapshotBytes = bytes;
      activeView.loadedFileName = filename || null;
      activeView.selectedTileIdx = -1;
      updateWeatherDataBuffers(activeView, true);
      setWeatherInfo(buildWeatherInfo(activeView))();
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
