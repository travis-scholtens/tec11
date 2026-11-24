# Contract: `tec11-local-terrain` Pure-Rust Crate API

**Crate**: `crates/tec11-local-terrain` (`//crates/tec11-local-terrain:tec11_local_terrain`)
**Dependencies**: `tec11-proto`, `tec11-math-grid`, `tec11-climate`, `tec11-geology` (strictly zero browser/WASM dependencies).

## 1. Anchor & Snapshot Loading

```rust
pub struct LocalVolumeConfig {
    /// Horizontal center-to-center hex spacing at Level 3 leaf scale (default: 1.0 m, range 0.5..=2.0).
    pub hex_spacing_m: f32,
    /// Vertical z-level height at Level 3 leaf scale (default: 1.0 m).
    pub z_step_m: f32,
}

impl LocalZoomAnchor {
    /// Load an immutable anchor from `.tec11` PlanetSnapshot bytes (Tectonics, Prehistory, or History).
    /// Does not mutate any external simulation.
    pub fn from_snapshot_bytes(
        snapshot_bytes: &[u8],
        parent_tile_id: usize,
    ) -> Result<Self, LocalTerrainError>;

    /// Deterministic hash identifying this `(world_seed, stage, parent_tile_id)`.
    pub fn anchor_hash(&self) -> u64;
}
```

## 2. Progressive Drill-Down (Levels 0–2)

```rust
impl LocalTerrainWorld {
    pub fn new(anchor: LocalZoomAnchor, config: LocalVolumeConfig) -> Self;

    /// Generate a 2.5D hex-column patch at zoom level `level` (0..=2) centered at `center_xy_m`.
    /// Clicking column `(q, r)` yields `col.local_xy_m` as the center for `level + 1`.
    pub fn generate_drilldown_patch(
        &self,
        level: u8,
        center_xy_m: [f64; 2],
        radius_hex: i32,
    ) -> DrillDownPatch;

    /// Discover notable physical features present inside this tile's neighborhood
    /// (River/Gorge, Highest Peak/Cliff, Coastline, Karst/Caverns, Ore Outcrop, Impact Crater).
    pub fn find_points_of_interest(&self) -> Vec<LocalPointOfInterest>;
}
```

## 3. Leaf 3D Chunk Streaming & Cell Read/Write (Level 3)

```rust
impl LocalTerrainVolume {
    /// Create a Level 3 3D hex-prism volume centered around `center_xy_m` (meters in anchor tangent frame).
    /// Automatically centers the vertical Z window around local surface elevation.
    pub fn new(
        world: &LocalTerrainWorld,
        center_xy_m: [f64; 2],
    ) -> Self;

    /// Stream (generate on demand and cache) a 16×16×32 chunk, applying any `DeltaOverlay` edits.
    pub fn get_or_generate_chunk(&mut self, coord: ChunkCoord) -> &HexChunk;

    /// Query a single hex prism at global local-volume axial coordinates `(q, r, z)`.
    pub fn get_cell(&mut self, q: i32, r: i32, z: i32) -> HexPrismCell;

    /// Mutate a single hex prism (e.g., digging or placing stone). Recorded in `DeltaOverlay`.
    pub fn set_cell(&mut self, q: i32, r: i32, z: i32, cell: HexPrismCell);

    /// Surface elevation (meters) and column inspection report at `(q, r)`.
    pub fn inspect_column(&mut self, q: i32, r: i32, z: i32) -> CellInspection;

    /// Export only modified cells as a compact versioned binary delta.
    pub fn export_delta_bytes(&self) -> Vec<u8>;

    /// Load and apply a previously exported binary delta (validates `anchor_hash`).
    pub fn import_delta_bytes(&mut self, bytes: &[u8]) -> Result<(), LocalTerrainError>;

    /// Approximate heap memory usage in bytes across cached chunks + delta.
    pub fn estimated_memory_bytes(&self) -> usize;
}
```

## 4. Mesher Contract

```rust
pub struct HexMeshBuffers {
    /// Interleaved or separate flat arrays for zero-copy WASM transfer.
    pub positions: Vec<f32>,  // 3 floats per vertex (x, y, z in meters)
    pub normals: Vec<f32>,    // 3 floats per vertex
    pub colors: Vec<f32>,     // 3 floats per vertex (linear RGB)
    pub cell_ids: Vec<i32>,   // 3 ints per triangle or per face for click-to-inspect picking (q, r, z)
}

pub struct LocalTerrainMesher;

impl LocalTerrainMesher {
    /// Build face-culled hex-prism mesh for a Level 0–2 `DrillDownPatch`.
    pub fn build_patch_mesh(patch: &DrillDownPatch) -> HexMeshBuffers;

    /// Build face-culled 3D cutaway hex-prism mesh for Level 3 chunks clipped at `z <= z_max`.
    /// Exposed top faces at `z == z_max` are always emitted cleanly for Z-level slicing.
    pub fn build_volume_mesh(
        volume: &mut LocalTerrainVolume,
        q_range: std::ops::Range<i32>,
        r_range: std::ops::Range<i32>,
        z_min: i32,
        z_max: i32,
    ) -> HexMeshBuffers;
}
```
