# Data Model: Volcanoes, Calderas, and 3D Magmatic Plumbing (Spec 009)

## Entities in `crates/tec11-local-terrain/src/volcanoes.rs`

### `VolcanoKind`
```rust
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum VolcanoKind {
    Stratovolcano,
    ShieldVolcano,
    ResurgentCaldera,
    VolcanicNeck,
}
```

### `ParasiticCone`
Small flank cinder/scoria cone associated with a `ShieldVolcano` or `Stratovolcano` (`FR-003`).
- `center_xy_m: [f64; 2]` — Anchor tangent-plane position.
- `radius_m: f32` — Basal radius (`200.0..500.0 m`, i.e., `400–1,000 m` diameter).
- `height_m: f32` — Peak height (`80.0..260.0 m`).
- `crater_radius_m: f32` — Summit crater radius (`0.18..0.28 * radius_m`).

### `VolcanicEdifice`
Deterministic, globally anchor-invariant volcanic landform (`FR-001`, `FR-002`).
- `tile_id: usize` — Originating coarse tile ID.
- `kind: VolcanoKind`
- `center_xy_m: [f64; 2]` — Projected from canonical 3D sphere `pos3` into active anchor tangent plane.
- `magma_material: MaterialId` — Extrusive/intrusive igneous material (`BASALT`, `ANDESITE`, `RHYOLITE`, `GABBRO`, `DIORITE`, `GRANITE`).
- `pluton_material: MaterialId` — Deep conduit/dike intrusive equivalent (`GABBRO`, `DIORITE`, `GRANITE`).
- `base_radius_m: f32` — Basal edifice radius (`600..48,000 m` by kind).
- `peak_height_m: f32` — Edifice relief above regional base (`220..2,900 m`).
- `caldera_radius_m: f32` — Summit crater or collapse caldera radius (`0.0` for pure eroded neck).
- `caldera_depth_m: f32` — Depth of summit depression below rim (`40..750 m`).
- `resurgent_radius_m: f32` — Central resurgent dome radius (`0.0` unless `ResurgentCaldera` or large collapse caldera).
- `resurgent_height_m: f32` — Height of central resurgent dome above caldera floor.
- `conduit_radius_m: f32` — Subsurface vertical feeder pipe radius (`18.0..65.0 m`).
- `dike_angles: [f32; 4]` — Radiating dike azimuths (radians) for Level 3 cross-cutting & neck ridges.
- `dike_count: u8` — Number of active radial dikes (`2..=4`).
- `activity: f32` — Volcanic youth/activity factor in `[0.0, 1.0]` (`subduction > 0` or `intrusion.is_some()` raises activity; drives sulfur fumaroles and lava-tube freshness).
- `crater_lake_z_m: Option<f32>` — Water/ice surface elevation inside caldera if `has_atmosphere && caldera_radius_m >= 500.0`.
- `crater_lake_state: Option<LakeState>` — Evaluated via `classify_lake_state` (`Freshwater | Frozen | DryPlaya | SaltFlat`).
- `parasitic_cones: Vec<ParasiticCone>` — 0..6 flank cinder cones.

### `VolcanicField`
Collection of all `VolcanicEdifice`s active within the 2-ring neighborhood of `LocalZoomAnchor`.
- `edifices: Vec<VolcanicEdifice>`
- Methods:
  - `VolcanicField::build(anchor: &LocalZoomAnchor, strat: &RegionalStratigraphyField, drainage: &DrainageNetwork) -> Self`
  - `VolcanicField::query(&self, x_m: f64, y_m: f64, has_atmosphere: bool, world_seed: u64) -> Option<VolcanicQueryResult>`

### `VolcanicQueryResult`
Continuous evaluation at `(x_m, y_m)` consumed by `synthesize_column_with_options` and `sample_cell_at_z`.
- `delta_elev_m: f32` — Net additive edifice relief (positive on cone/rim/dome, negative inside caldera depression).
- `in_caldera: bool` — True inside the caldera/crater depression floor.
- `crater_lake_z_m: Option<f32>` — Water/ice surface elevation if inside a flooded/frozen caldera.
- `crater_lake_state: Option<LakeState>`
- `in_conduit_or_dike: bool` — True if `(x_m, y_m)` intersects the vertical igneous conduit or a radial dike blade.
- `in_contact_halo: bool` — True within `4..10 m` of a conduit/dike (contact metamorphic `HORNFELS` + epithermal enrichment).
- `magma_material: MaterialId`
- `pluton_material: MaterialId`
- `sulfur_intensity: f32` — `[0.0, 1.0]` fumarolic sulfur crust intensity on active crater rims/caldera floors.
- `lava_tube_eligible: bool` — True on fresh basaltic shield flanks (`ShieldVolcano` + `activity > 0.25`).
- `radial_dir: [f32; 2]` — Unit vector pointing radially away from edifice center (aligns lava tubes downslope).

## Extensions to Existing Structs
- `NeighborTileSample` (`anchor.rs`): add `pub has_intrusion: bool` (populated from `tile.intrusion.is_some()`).
- `LocalZoomAnchor` (`anchor.rs`): add `pub volcanic_seeds: Vec<VolcanicSeedAnchor>` projected from canonical 3D sphere positions in `PlanetaryTerrainContext` so re-anchoring has `0.00 m` drift.
- `ColumnSynthesis` (`geomorphology.rs`): store `pub volcanic_hit: Option<VolcanicQueryResult>` so `sample_cell_at_z` evaluates 3D conduits, dikes, sulfur crusts, and lava tubes in $O(1)$.
- `LocalTerrainWorld` (`lib.rs`) & `LocalTerrainVolume` (`chunk.rs`): hold `Arc<VolcanicField>`.
