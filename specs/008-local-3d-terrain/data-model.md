# Data Model: Local 3D Hex-Prism Terrain & Underground Zoom

**Feature**: `008-local-3d-terrain` | **Date**: 2026-11-24

All simulation types live in the pure-Rust crate `crates/tec11-local-terrain/` with zero browser/WASM dependencies.

## 1. Anchor & Planetary Context

### `LocalZoomAnchor`
Immutable planetary reference extracted from `PlanetSnapshot` bytes (`TectonicsStage`, `PrehistoryStage`, or `HistoryStage`):
- `world_seed: u64` — deterministic seed derived from `PlanetSnapshot.rng_state_16b`.
- `stage: SnapshotStage` — `Tectonics | Prehistory | History`.
- `parent_tile_id: usize` — coarse tile index (Tectonics/Prehistory) or fine tile index (History).
- `parent_tile_radius_m: f64` — physical radius of the parent tile in meters (derived from `radius_km` and grid size).
- `anchor_normal: [f64; 3]` — unit vector on the sphere at the parent tile center.
- `tangent_east: [f64; 3]`, `tangent_north: [f64; 3]` — right-handed local tangent basis.
- `sea_level_m: f64` — eustatic sea level offset in meters (`0.0` in Tectonics/History, `eustatic_sea_level_km * 1000.0` in Prehistory).
- `has_atmosphere: bool`, `has_life: bool` — gates water, soil, karst, and flora (false on airless Hadean Tectonics tiles).
- `neighbors: Vec<NeighborTileSample>` — center tile + 5 (pentagon) or 6 (hexagon) 1-ring neighbor samples projected into local tangent `(x_m, y_m)` coordinates.

### `NeighborTileSample`
- `tile_id: usize`
- `local_xy_m: [f64; 2]` — position in anchor tangent plane (meters).
- `elevation_m: f32`
- `mountainosity: f32`
- `subduction: f32`
- `impact_scar: f32`
- `soil_depth_m: f32` — persisted physical soil depth (`TileGeology.soil_depth_m`, `[0.0, 25.0]`).
- `soil_fertility: f32` — persisted physical soil fertility (`TileGeology.soil_fertility`, `[0.45, 1.50]`).
- `is_glaciated: bool`, `glacier_cover: f32` — physical glacier state from `ClimateModel` mass balance.
- `annual_temp_c: f32`, `precip_norm: f32`, `tidal_range_m: f32`, `wind_vec: [f32; 2]`.
- `layers: Vec<RockLayer>` — full bedrock stratigraphic stack (`TileGeology.layers`).
- `occurrences: Vec<LayerMineralHint>` — mineralized beds/interfaces derived via `TileGeology::derive_layer_occurrences()`.

## 2. Progressive Drill-Down State

### `ZoomLevel` (`0..=3`)
- `level: u8` — `0` (tile scale, ~1.5–4 km/hex), `1` (district, ~250–500 m/hex), `2` (valley/ridge, ~25–50 m/hex), `3` (leaf 3D embark, configurable `0.5–1.0 m/hex`, default `1.0 m`).
- `center_xy_m: [f64; 2]` — current focus point in the anchor tangent plane (meters).
- `hex_spacing_m: f32` — horizontal center-to-center hex spacing at this level.
- `z_step_m: f32` — vertical z-level height (`1.0 m` at Level 3).

### `DrillDownPatch` (Levels 0–2)
Lightweight 2.5D hex-column patch rendered by the same WebGL hex-column renderer before reaching Level 3:
- `level: u8`, `hex_spacing_m: f32`, `radius_hex: i32` (default `24..32` → ~1,800–3,200 hex columns).
- `columns: Vec<PatchColumn>`:
  - `q: i32`, `r: i32` — axial hex coordinates relative to `center_xy_m`.
  - `local_xy_m: [f64; 2]` — exact world-tangent position for drilling into the next level.
  - `surface_elev_m: f32`
  - `water_elev_m: Option<f32>` — river or ocean surface elevation if wet.
  - `surface_material: MaterialId` — exposed rock, soil, loess, sand, ice, or water.
  - `soil_depth_m: f32`
  - `is_river: bool`

## 3. Continuous Stratigraphy & River Splines

### `CorrelatedStratum`
Unified bed correlated across `LocalZoomAnchor.neighbors`:
- `stratum_id: u16`
- `rock: Rock` — canonical rock properties (`kind`, `name`, `toughness`, `felsity`, `calcity`, `clasticity`, `bogginess`, `salinity`, `quartzity`, `sulfidity`).
- `thickness_by_neighbor_m: Vec<f32>` — thickness in meters at each neighbor (`0.0` where absent → tapers to pinch-out wedge).
- `mineral_veins: Vec<MineralVeinSpec>` — ore/mineral material IDs and probability density derived from `TileGeology` occurrences.

### `RiverSplineSegment`
Deterministically subdivided river reach:
- `p0_m: [f64; 2]`, `p1_m: [f64; 2]` — tangent-plane endpoints.
- `z_water_0_m: f32`, `z_water_1_m: f32` — **constraint**: `z_water_1_m <= z_water_0_m` (strictly non-increasing downstream, SC-005).
- `channel_width_m: f32` — active wetted channel width (`>= 2.0 m`).
- `channel_depth_m: f32` — depth below `z_water` (`>= 0.5 m`).
- `valley_half_width_m: f32` — floodplain/canyon blending envelope (`>= channel_width_m`).
- `discharge: f32` — relative flow magnitude from parent river routing.

## 4. Leaf 3D Hex-Prism Volume & Chunks (Level 3)

### `MaterialId(pub u16)`
Canonical material palette covering:
- `0`: `Air`
- `1..=15`: Fluids & surficial mantles (`Water`, `OceanWater`, `Ice`, `Snow`, `Topsoil`, `LoessTill`, `AlluvialSilt`, `RiverGravel`, `Sand`, `Peat`, `Talus`, `SaltCrust`)
- `16..=63`: Exact Tec11 `Rock` types (Igneous: `Basalt`, `Andesite`, `Rhyolite`, `Gabbro`, `Diorite`, `Granite`, `Kimberlite`; Sedimentary: `Claystone`, `Siltstone`, `Sandstone`, `Conglomerate`, `Limestone`, `Chalk`, `ImpactBreccia`; Metamorphic: `Slate`, `Phyllite`, `Schist`, `Greenschist`, `Amphibolite`, `Eclogite`, `Gneiss`, `Granulite`, `Marble`, `Quartzite`, `Hornfels`, `Blueschist`)
- `64..=95`: Discrete ore/mineral veins (`NativeCopper`, `IronOre`, `GoldVein`, `SilverVein`, `TinOre`, `CoalSeam`, `QuartzVein`, `GemstoneVein`, `SulfurDeposit`, `KimberliteDiamond`)

### `HexPrismCell` (`#[repr(C)]`, 4 bytes)
- `material: MaterialId` (`u16`)
- `fluid: u8` — `0 = dry`, `1..=7 = water depth/saturation`, `8 = lava/magma` (future-proof)
- `flags: u8` — bitflags: `EXPOSED_SURFACE`, `EXCAVATED_BY_DELTA`, `FLORA_GRASS`, `FLORA_SHRUB`, `FLORA_TREE`, `NATURAL_CAVE`

### `ChunkCoord`
- `cq: i32`, `cr: i32`, `cz: i32` — chunk indices where each chunk spans `CHUNK_Q = 16`, `CHUNK_R = 16`, `CHUNK_Z = 32` (`8,192` cells).

### `HexChunk`
- `coord: ChunkCoord`
- `storage: ChunkStorage` — either `Uniform(HexPrismCell)` (4 bytes for pure air/deep bedrock), `Rle(Vec<(u16, HexPrismCell)>)`, or `Dense(Box<[HexPrismCell; 8192]>)` (32 KB uncompressed, promoted only when mutated or high-entropy).
- `dirty_mesh: bool`

### `DeltaOverlay`
Sparse edit layer persisted separately from `.tec11`:
- `version: u16` (`1`)
- `anchor_hash: u64` — validates delta matches the target `LocalZoomAnchor`.
- `chunks: BTreeMap<ChunkCoord, BTreeMap<u16, HexPrismCell>>` — maps `(cq, cr, cz)` → local cell index (`0..8192`) → overridden `HexPrismCell`.
- Binary encoding: `[b"T11D", version: u16, anchor_hash: u64, chunk_count: u32, (cq: i32, cr: i32, cz: i32, cell_count: u16, (idx: u16, material: u16, fluid: u8, flags: u8)*)*]`.

### `LocalTerrainVolume`
- `anchor: LocalZoomAnchor`
- `config: LocalVolumeConfig` (`hex_spacing_m: f32`, `z_step_m: f32`, `embark_radius_chunks: i32`)
- `stratigraphy: RegionalStratigraphyField`
- `rivers: Vec<RiverSplineSegment>`
- `chunks: HashMap<ChunkCoord, HexChunk>`
- `delta: DeltaOverlay`
