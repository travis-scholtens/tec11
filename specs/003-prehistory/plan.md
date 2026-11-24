# Implementation Plan: Physical Surficial Soil State (Spec 003 — FR-004)

**Branch**: `003-prehistory` | **Date**: 2026-11-24 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/003-prehistory/spec.md` (User Story 2 & FR-004: physical surficial soil state replacing glacial tick counters, driven by physical `ClimateModel::is_tile_glaciated`)

## Summary

Replace the reified `glaciated_ticks` and `periglacial_ticks` history counters in `PrehistorySimulation` with physical surficial soil state (`soil_depth_m` and `soil_fertility`) stored directly on `TileGeology` and persisted via optional fields on `GeologicalTile` in `proto/tec11.proto`. Each Prehistory tick physically strips soil under active glaciers (`climate_model.is_tile_glaciated(t)`), deposits mineral-rich loess/till on periglacial margins, and weathers exposed bedrock on ice-free land. Both Prehistory and History carrying capacity read directly from `TileGeology::effective_soil_fertility()`, preserving full fidelity across `.tec11` snapshot save/reload.

## Technical Context

**Language/Version**: Rust (edition 2021) + Protobuf (`proto3`)  
**Primary Dependencies**: `tec11-proto`, `tec11-geology` (`TileGeology`, `Rock`, `PrehistorySimulation`, `HistorySimulation`)  
**Storage**: Binary protobuf `PlanetSnapshot` (`proto/tec11.proto`)  
**Testing**: `bazel test -c opt --incompatible_strict_action_env --disk_cache=~/.cache/bazel-disk-cache //crates/tec11-geology/... //crates/tec11-proto/...` + `bazel build //:web_bundle`  
**Target Platform**: Native Rust (Linux/macOS/Windows) and WebAssembly (`wasm32-unknown-unknown`)  
**Performance Goals**: Zero measurable regression in Prehistory tick latency (< 100 ms on Medium 7,292-tile worlds)  
**Constraints**: Backward-compatible decoding of existing `.tec11` snapshots that omit `soil_depth_m` / `soil_fertility`

## Constitution Check

- **I. Emergent Physical Processes Over Reified Abstractions**: PASS — eliminates `glaciated_ticks` / `periglacial_ticks` history counters in favor of physical `soil_depth_m` and `soil_fertility` mutated each tick by physical glacier scouring, loess deposition, and weathering.
- **II. Model-Derived Stochasticity**: PASS — deterministic physical update driven by `climate_model.is_tile_glaciated(t)`, adjacency, climate precipitation, and underlying bedrock `Rock::soil_fertility()`.
- **III. Immutable Isolation**: PASS — `to_proto_geological_tile(&self)` remains strictly immutable.
- **IV. Deterministic Snapshot Continuity**: PASS — fixes the persistence gap where glacial soil modifiers were lost on `.tec11` save/reload; older snapshots decode cleanly via optional proto fields.
- **V. Crate Modularity**: PASS — pure Rust simulation + proto change with zero UI duplication.

## Data Model & Physical Equations

### 1. Protobuf Schema (`proto/tec11.proto`)
Add two optional fields to `GeologicalTile`:
```protobuf
message GeologicalTile {
  float bottom = 1;
  float mountainosity = 2;
  repeated RockLayer layers = 3;
  optional float soil_depth_m = 4;
  optional float soil_fertility = 5;
}
```
- **Backward compatibility**: When decoding an older snapshot where `soil_depth_m` or `soil_fertility` is `None`:
  - `soil_depth_m` defaults to `2.0` on land (`elevation > 0.0`) and `0.0` on ocean (`elevation <= 0.0`).
  - `soil_fertility` defaults to `tile.surface_soil_fertility()` (the bedrock parent-material fertility from the top `0.30 km` of `layers`).

### 2. `TileGeology` (`crates/tec11-geology/src/tile.rs`)
Add fields to `TileGeology`:
- `pub soil_depth_m: f32` — physical thickness of weathered regolith + deposited loess/till in meters (`[0.0, 25.0]`, default `2.0` on land, `0.0` on ocean).
- `pub soil_fertility: f32` — current nutrient richness of the surficial soil (`[0.45, 1.50]`, initialized from `surface_soil_fertility()`).

Keep `TileGeology::surface_soil_fertility(&self) -> f32` as the **bedrock parent-material fertility**, and add `TileGeology::effective_soil_fertility(&self) -> f32`:
- Depth factor from `soil_depth_m`:
  - `0.0 m` (bare scoured bedrock) → `0.68`
  - `2.0 m` (normal weathered regolith) → `1.00`
  - `10.0+ m` (deep periglacial loess/till) → up to `1.08`
- Returns `(self.soil_fertility * depth_factor).clamp(0.45, 1.55)`.

### 3. Per-Tick Physical Soil Update (`crates/tec11-geology/src/prehistory.rs`)
Replace `glaciated_ticks`, `periglacial_ticks`, `record_glacial_soil_history`, and `glacial_soil_fertility_modifier` with `update_surficial_soil(&mut self)` (called each 4,000-year Prehistory step):
- **Active glacier / ice sheet** (`eff_e > 0.0 && self.climate_model.is_tile_glaciated(t)`):
  - Strip soil: `soil_depth_m = (soil_depth_m * 0.65 - 0.35).max(0.0)`.
  - Relax `soil_fertility` toward bedrock baseline (`lerp(soil_fertility, bedrock_fert, 0.25)`).
- **Periglacial margin** (ice-free land within 2 hops of an active glacier):
  - Deposit loess/till: `added = 0.65 m` (capped at `20.0 m`), blending `soil_fertility` mass-weighted with fresh loess fertility (`1.38`).
- **Ice-free non-periglacial land**:
  - Slow climate-driven weathering rebuilds `soil_depth_m` toward `target_depth = (0.6 + 2.0 * precip_norm).clamp(0.5, 2.6)` and gently relaxes `soil_fertility` toward `bedrock_fert`.
- Both Prehistory (`prehistory.rs`) and History (`history.rs`) carrying capacity read `tile.effective_soil_fertility()` directly.

### 4. Geology Detail Modal Soil Display (`tile.rs`, `SkyVantage.js`, `SkyVantage.purs`, `App.js`, `App.purs`)
- Export `soilDepthM`, `soilFertility`, `bedrockSoilFertility`, and `effectiveSoilFertility` in `TileGeology::to_stratigraphy_json` (`crates/tec11-geology/src/tile.rs`).
- Format a concise `soilSummary` string in `src/Tec11/SkyVantage.js` from those Rust-computed JSON fields (no duplicated physical formulas in JS/PureScript):
  - Land tiles (`elevationKm > 0`): e.g. `Surficial Soil: Deep loess/till · Depth: 5.9 m · Fertility: 1.25× (effective 1.30×, bedrock 0.92×)` (or `Bare scoured bedrock` when `< 0.35 m`, `Thin regolith` when `< 1.2 m`, `Weathered regolith` when `1.2..3.5 m`, `Deep loess/till` when `>= 3.5 m`).
  - Submarine tiles (`elevationKm <= 0`): `Surficial Soil: Submarine seafloor (no terrestrial soil)`.
- Expose `soilSummary :: String` on `TileViewInfo` (`src/Tec11/SkyVantage.purs`, `src/Tec11/App.js`) and render it inside `GeologyModal` and the stratigraphy details box in `src/Tec11/App.purs`.

## Files to Modify

```text
proto/tec11.proto                         # Add optional soil_depth_m & soil_fertility to GeologicalTile
crates/tec11-proto/src/lib.rs             # Update GeologicalTile literals in unit tests
crates/tec11-geology/src/tile.rs          # Add soil_depth_m, soil_fertility, effective_soil_fertility, proto serde, stratigraphy JSON
crates/tec11-geology/src/prehistory.rs    # Replace tick counters with update_surficial_soil(); update tests
crates/tec11-geology/src/history.rs       # Read effective_soil_fertility() directly from TileGeology
src/Tec11/SkyVantage.js                   # Format soilSummary from stratigraphy JSON
src/Tec11/SkyVantage.purs                 # Add soilSummary to TileViewInfo / defaultTileViewInfo
src/Tec11/App.js                          # Add soilSummary to emptyTileView
src/Tec11/App.purs                        # Render soilSummary in GeologyModal and stratigraphy details box
```

## Verification Plan

1. Update `test_glacial_scouring_vs_periglacial_loess_fertility` in `prehistory.rs` to assert physical `soil_depth_m`, `soil_fertility`, and `effective_soil_fertility()`.
2. Add a `PlanetSnapshot` round-trip test verifying `soil_depth_m` and `soil_fertility` survive encode/decode identically, and legacy snapshots without those fields decode cleanly.
3. Run `bazel test //crates/tec11-geology/... //crates/tec11-proto/...` and `bazel build //:web_bundle`.
