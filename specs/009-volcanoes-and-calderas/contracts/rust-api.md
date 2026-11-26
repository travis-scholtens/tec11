# Rust & WASM Contracts: Volcanoes, Calderas, and 3D Magmatic Plumbing (Spec 009)

## Public Rust API (`tec11-local-terrain`)

### New Module `tec11_local_terrain::volcanoes`
```rust
pub use volcanoes::{
    ParasiticCone, VolcanicEdifice, VolcanicField, VolcanicQueryResult, VolcanoKind,
};
```
- `VolcanicField::build(anchor: &LocalZoomAnchor, strat: &RegionalStratigraphyField, drainage: &DrainageNetwork) -> Self`
- `VolcanicField::query(&self, x_m: f64, y_m: f64, has_atmosphere: bool, world_seed: u64) -> Option<VolcanicQueryResult>`

### `LocalTerrainWorld` (`lib.rs`)
- `pub fn volcanoes(&self) -> &[VolcanicEdifice]` — Read-only access to synthesized volcanic edifices for inspection and tests.
- `find_points_of_interest(&self) -> Vec<LocalPointOfInterest>`:
  - Emits volcanic POIs with `kind: "volcano"` and descriptive labels:
    - `"Stratovolcano"` / `"Shield Volcano"` / `"Caldera"` / `"Crater Lake"` / `"Volcanic Neck"`
  - Anchors `local_xy_m` right on the **summit/caldera rim** (offset by `caldera_radius_m` from center, or `0.5 * conduit_radius_m` for a neck) so jumping to Level 2 or Level 3 immediately frames both the rim/slope and the crater/caldera interior (and conduit contact at Level 3).

### `WasmLocalTerrainSession` (`tec11-wasm`)
- No breaking changes to any `#[wasm_bindgen]` method signatures or JSON schemas!
- `status_json()` automatically exposes the new volcanic POI buttons in `pois: [{ kind, label, description }]`.
- `inspect_hit_json()` automatically reports `Volcanic Sulfur Deposit`, igneous conduit/dike rock, `Hornfels` contact aureole, `Air (Natural Cave)` inside basalt lava tubes, and `· Crater Lake` / `· Caldera` tags.
