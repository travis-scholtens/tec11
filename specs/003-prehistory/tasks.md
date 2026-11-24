# Tasks: Physical Glacier Mass Balance & Removing Non-Anthropocentric Köppen Inputs

**Input**: Design documents from `/specs/003-prehistory/` (`spec.md`, `plan.md`)

## Phase 1: Core Climate Model (`tec11-climate`)

- [x] T001 Add `land_glacier_cover: Vec<f32>` field and `compute_land_glacier_cover()` (8-season temperature & snowfall mass balance + 1-hop cold-margin outflow) in `crates/tec11-climate/src/model.rs`, updated automatically on every `recompute()` and `update_for_tectonics()`.
- [x] T002 Expose `pub fn tile_glacier_cover(&self) -> &[f32]` and `pub fn is_tile_glaciated(&self, v: usize) -> bool` on `ClimateModel` in `crates/tec11-climate/src/model.rs`.
- [x] T003 Add unit tests in `crates/tec11-climate/src/model.rs` verifying snowy cold tiles glaciate, warm-summer tundra (`+8 °C`) and arid polar deserts do not glaciate, and 1-hop cold-margin outflow works.

## Phase 2: Prehistory & River Headwater Wiring (`tec11-geology`)

- [x] T004 Update `PrehistorySimulation::record_glacial_soil_history()` in `crates/tec11-geology/src/prehistory.rs` to use `self.climate_model.is_tile_glaciated(t)` instead of `matches!(self.tile_koeppen[t], 12 | 13)`.
- [x] T005 Update `trace_coarse_rivers()` in `crates/tec11-geology/src/rivers.rs` to gate headwaters on physical `glacier_cover: &[f32]` (`< 0.50`) instead of `koeppen_ids` (`k != 12 && k != 13`).
- [x] T006 Update all `trace_coarse_rivers()` callers in `rivers.rs`, `lifeforms.rs`, `terrain.rs`, `prehistory.rs`, `tides_sandbox.rs`, and `history.rs` to pass `climate_model.tile_glacier_cover()`.

## Phase 3: Weather Albedo, History Groundcover, Tides & Sky Vantage

- [x] T007 Update `WeatherSandbox::build_solver_for_base()` in `crates/tec11-geology/src/weather_sandbox.rs` to derive `coarse_snow_m` and ice-sheet surface moisture/albedo from `climate_model.tile_glacier_cover()` instead of `koeppen == 12 | 13`.
- [x] T008 Update History groundcover/vegetation (`history.rs:8944, 9118`), Tides natural color (`tides_sandbox.rs:426-435`), and Sky Vantage snow/ice checks (`sky_vantage.rs:827-828, 976-977`) to use physical glacier cover instead of `koeppen == 12 | 13`.

## Phase 4: Verification

- [x] T009 Run `bazel test -c opt --incompatible_strict_action_env --disk_cache=~/.cache/bazel-disk-cache //crates/tec11-climate/... //crates/tec11-geology/...` and verify all tests pass.
- [x] T010 Build `//:web_bundle` cleanly (`bazel build -c opt --incompatible_strict_action_env --disk_cache=~/.cache/bazel-disk-cache //:web_bundle`).
