# Contract: `tec11-wasm` & World UI Local 3D Terrain Modal

## 1. WASM Session (`crates/tec11-wasm/src/local_terrain_session.rs`)

Exposed via `#[wasm_bindgen]` as `LocalTerrainSession`:

```rust
#[wasm_bindgen]
pub struct LocalTerrainSession { /* owns LocalTerrainWorld, current zoom level stack, active Level 3 LocalTerrainVolume, cached HexMeshBuffers */ }

#[wasm_bindgen]
impl LocalTerrainSession {
    /// Construct from immutable PlanetSnapshot bytes + selected parent tile index
    /// (plus optional isolated-island elevation override & History geographic `(lat, lon)` center).
    #[wasm_bindgen(constructor)]
    pub fn new(
        snapshot_bytes: &[u8],
        parent_tile_id: usize,
        surface_elev_override_km: Option<f32>,
        target_lat_deg: Option<f32>,
        target_lon_deg: Option<f32>,
    ) -> Result<WasmLocalTerrainSession, JsValue>;

    /// Current zoom level (`0..=3`, where `3` is the ~1 m 3D hex-prism volume).
    pub fn zoom_level(&self) -> u8;

    /// Drill one level deeper centered on clicked column `(q, r)` from the current patch.
    pub fn drill_down(&mut self, q: i32, r: i32) -> bool;

    /// Step back up one zoom level (returns false if already at Level 0).
    pub fn step_up(&mut self) -> bool;

    /// Jump directly to Level 3 (~1 m) at the current center without stepping through intermediate clicks.
    pub fn jump_to_leaf(&mut self);

    /// Pan laterally in the local tangent plane at any zoom level (`0..=3`) by ~half the visible patch/embark width.
    pub fn pan_lateral(&mut self, dir_east: f32, dir_north: f32);

    /// Jump directly to a discovered in-tile Point of Interest by index (centers view at Level 2 or current leaf).
    pub fn jump_to_poi(&mut self, poi_index: usize) -> bool;

    /// Vertical Z bounds `[z_min, z_max]` of the active Level 3 volume (and current `z_slice`).
    pub fn z_min(&self) -> i32;
    pub fn z_max(&self) -> i32;
    pub fn z_slice(&self) -> i32;

    /// Update the active Z-level slice (Level 3) and rebuild exposed slice mesh (< 50 ms).
    pub fn set_z_slice(&mut self, z_max: i32);

    /// Shift the loaded Level 3 vertical Z-window deeper or shallower (e.g., -100 m / +100 m)
    /// to inspect deep crust strata and ore contacts down toward the Moho.
    pub fn shift_z_window(&mut self, delta_z: i32);

    /// Flat mesh arrays for the current zoom level / z_slice (Three.js BufferGeometry).
    pub fn mesh_positions(&self) -> Vec<f32>;
    pub fn mesh_normals(&self) -> Vec<f32>;
    pub fn mesh_colors(&self) -> Vec<f32>;

    /// Pick & inspect a hex prism / column at `(x_m, y_m, z_m)` hit by Three.js raycaster.
    /// Returns JSON `{ "q", "r", "z", "materialName", "rockKind", "depthBelowSurfaceM", "elevationM", "soilDepthM", "isFluid", "isOre", "summary" }`.
    pub fn inspect_hit_json(&mut self, x_m: f32, y_m: f32, z_m: f32) -> String;

    /// Status JSON `{ "zoomLevel", "hexSpacingM", "zMin", "zMax", "zSlice", "surfaceElevM", "canStepUp", "isLeaf", "title" }`.
    pub fn status_json(&self) -> String;
}
```

## 2. World UI Modal (`src/Tec11/App.purs` & `src/Tec11/App.js`)

- **Entry Point**: New `LocalTerrainModal` variant in `WorldModal` (`src/Tec11/App.purs`).
- **Button**: **"Local 3D Terrain"** button in the selected-tile action bar alongside **Geology**, **Sky**, and **Life**; **disabled unless World simulation is paused** (`FR-008`).
- **Isolation**: `mountWorldLocalTerrainModal` pulls immutable `PlanetSnapshot` bytes from the paused `WorldSession` (`session.to_snapshot_bytes()`) + selected tile ID, constructs `new LocalTerrainSession(bytes, tileId)`, and mounts a dedicated Three.js orbit/cutaway WebGL canvas.
- **Controls inside Modal**:
  - Zoom breadcrumb & scale readout (`Level 0: ~2.5 km/hex` → `Level 1: ~300 m/hex` → `Level 2: ~35 m/hex` → `Level 3: 1.0 m/hex`), **Step Up** button, and **Jump to 1 m 3D** shortcut.
  - **Click on any hex** at Levels 0–2 drills down into that hex; **Click on any hex** at Level 3 displays the inspected cell's rock/mineral/soil/depth readout.
  - **Z-Level Slicer & Depth Window** (visible at Level 3): Range slider + `-1 m` / `+1 m` step buttons scrubbing `z_slice` from sky down into bedrock, plus `-100 m` / `+100 m` **Depth Window** buttons to stream deeper crust layers down toward the Moho on thick, many-layered tiles.
  - **Orbit / Tilt / Zoom**: Left-drag orbit, wheel zoom, right-drag pan (matching existing Three.js modal conventions).
- **Cleanup (`FR-009`)**: Closing the modal calls the returned React `useEffect` disposer, which cancels `requestAnimationFrame`, disposes Three.js geometry/materials/renderer, calls `localSession.free()`, and leaves the base `WorldSession` completely untouched.
