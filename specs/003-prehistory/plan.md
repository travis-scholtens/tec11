# Implementation Plan: Physical Glacier Mass Balance & Removing Non-Anthropocentric Köppen Inputs

**Branch**: `003-prehistory` | **Date**: 2026-11-24 | **Specs**: [002-tectonics](../002-tectonics/spec.md), [003-prehistory](spec.md), [004-history](../004-history/spec.md), [005-weather](../005-weather/spec.md)
**Input**: Constitution Principle I update + Specs `002`–`005` in commit `yvqytsrv` (`4ddeb061`): derive land glaciers, river headwaters, weather snow/albedo, and snow/ice groundcover physically from `ClimateModel` instead of Köppen `EF`/`ET`.

## Summary

Add a first-class physical land glacier/ice-sheet mass-balance calculation (`tile_glacier_cover: Vec<f32>` in `[0.0, 1.0]`) directly on `ClimateModel` derived from its 8 seasonal `temperature` and `precipitation` snapshots (cold-season snowfall vs. warm-season positive-degree-day melt + 1-hop cold-margin outflow). Replace all non-anthropocentric `koeppen == 12 | 13` (`EF`/`ET`) checks in Prehistory glacial scouring, river headwater gating, Weather snow/albedo initialization, History groundcover/labels, Tides natural color, and Sky Vantage with `ClimateModel`'s physical glacier cover—while preserving Köppen shorthand in human habitability/passability (`population.rs`, `history.rs`) per Constitution Principle I.

## Technical Context

**Language/Version**: Rust (edition 2021)  
**Primary Dependencies**: `tec11-climate` (`ClimateModel`), `tec11-geology` (`prehistory.rs`, `rivers.rs`, `weather_sandbox.rs`, `history.rs`, `tides_sandbox.rs`, `sky_vantage.rs`, `lifeforms.rs`, `terrain.rs`)  
**Testing**: `bazel test -c opt --incompatible_strict_action_env --disk_cache=~/.cache/bazel-disk-cache //crates/tec11-climate/... //crates/tec11-geology/...` + `bazel build //:web_bundle`  
**Target Platform**: Native Rust and WebAssembly (`wasm32-unknown-unknown`)  
**Performance Goals**: Glacier mass balance runs once per `ClimateModel` recompute in $O(N)$ (< 1 ms on 7,292 tiles)  
**Constraints**: Zero changes to `PlanetSnapshot` schema (glacier cover is deterministically derived from `ClimateModel` on load/recompute); human passability/habitability in `population.rs` remains unchanged.

## Constitution Check

- **I. Emergent Physical Processes Over Reified Abstractions**: PASS — removes all non-anthropocentric Köppen `EF`/`ET` inputs from physical systems while retaining Köppen only for human survivability/passability.
- **II. Model-Derived Stochasticity**: PASS — deterministic mass-balance calculation from seasonal temperature and snowfall.
- **III. Immutable Isolation**: PASS — read-only accessors on `ClimateModel`.
- **IV. Deterministic Snapshot Continuity**: PASS — `ClimateModel` recomputes identical glacier cover on snapshot reload.
- **V. Crate Modularity**: PASS — physical glacier model lives in `tec11-climate`, consumed cleanly by `tec11-geology`.

## Physical Model (`ClimateModel::compute_land_glacier_cover`)

For each tile `v` in `0..n`:
1. If `elevations[v] <= 0.0` (ocean), `land_glacier_cover[v] = 0.0` (sea ice is handled separately by ocean temperature).
2. Otherwise, across the 8 `SeasonalSnapshot`s (using `templimits = self.temperature_range()` and `mm_per_season = 2000.0 / n_seasons`):
   - `t_c = templimits.to_celsius(snap.temperature[v] as f64)`
   - `p_mm = (snap.precipitation[v] as f64).max(0.0) * mm_per_season`
   - Track `max_t_c`, `snowfall_mm` (sum of `p_mm` when `t_c <= 0.0`), and `pos_deg_seasons` (sum of `t_c.max(0.0)`).
3. Annualized positive-degree-day melt potential (in Earth-year-equivalent mm/yr, matching `ClimateModel`'s annualized `p_mm` so orbital year length cancels out): `melt_mm = pos_deg_seasons * 110.0` (~2.4 mm/°C/Earth-day × 45.6 Earth-days per 1/8-year season).
4. Net snow balance:
   - **Perennial ice sheet / accumulation zone (`>= 0.50`)**:
     - If `max_t_c <= -1.0 && snowfall_mm >= 15.0`: `cover = (0.65 + (snowfall_mm / 300.0).min(0.35)).min(1.0)`
     - Else if `snowfall_mm >= 40.0 && snowfall_mm > melt_mm`: `cover = (0.55 + ((snowfall_mm - melt_mm) / 300.0).min(0.45)).min(1.0)`
   - **Seasonal / patchy snow (`0.0..0.45`, NOT an active ice sheet)**:
     - If `snowfall_mm >= 25.0 && max_t_c <= 8.0`: `cover = ((snowfall_mm / (melt_mm + 120.0)) * 0.45).clamp(0.0, 0.45)`
5. **1-hop ice-sheet outflow**:
   - Any land tile with `max_t_c <= 4.0` adjacent to a strong accumulation zone (`cover >= 0.70`) receives lobe outflow: `cover = cover.max(0.55)`.
6. Public API on `ClimateModel`:
   - `pub fn tile_glacier_cover(&self) -> &[f32]`
   - `pub fn is_tile_glaciated(&self, v: usize) -> bool` (`cover >= 0.50`)

## Files to Modify

```text
crates/tec11-climate/src/model.rs          # Compute & expose tile_glacier_cover() & is_tile_glaciated()
crates/tec11-geology/src/prehistory.rs     # Use climate_model.is_tile_glaciated(t) in record_glacial_soil_history()
crates/tec11-geology/src/rivers.rs         # Replace koeppen_ids param with glaciated/frozen mask from ClimateModel
crates/tec11-geology/src/lifeforms.rs      # Pass physical glacier mask to trace_coarse_rivers()
crates/tec11-geology/src/terrain.rs        # Pass physical glacier mask to trace_coarse_rivers()
crates/tec11-geology/src/tides_sandbox.rs  # Pass physical glacier mask to trace_coarse_rivers() + natural-color ice
crates/tec11-geology/src/weather_sandbox.rs# Derive initial snow depth & ice soil moisture from tile_glacier_cover()
crates/tec11-geology/src/history.rs        # Pass physical glacier mask to rivers; use glacier cover for veg/groundcover
crates/tec11-geology/src/sky_vantage.rs    # Use physical glacier cover instead of koeppen == 12 | 13 for snow/ice ground
```

## Verification Plan

1. Add unit tests in `crates/tec11-climate/src/model.rs` verifying:
   - Snowy subpolar/alpine tiles glaciate (`is_tile_glaciated == true`).
   - Dry polar deserts (`snowfall < 15 mm`) and warm-summer tundra (`max_t_c = +8 °C`) do NOT glaciate (`is_tile_glaciated == false`), proving independence from Köppen `ET`/`EF`.
2. Run `bazel test -c opt --incompatible_strict_action_env --disk_cache=~/.cache/bazel-disk-cache //crates/tec11-climate/... //crates/tec11-geology/...`.
3. Build `//:web_bundle` cleanly (`bazel build -c opt --incompatible_strict_action_env --disk_cache=~/.cache/bazel-disk-cache //:web_bundle`).
