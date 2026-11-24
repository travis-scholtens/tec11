# Implementation Plan: Local 3D Hex-Prism Terrain & Underground Zoom

**Branch**: `008-local-3d-terrain` | **Date**: 2026-11-24 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/008-local-3d-terrain/spec.md`

## Summary

Introduce a pure-Rust, browser-independent crate `tec11-local-terrain` that takes immutable `.tec11` `PlanetSnapshot` bytes (across Tectonics, Prehistory, and History stages) and performs lazy hierarchical zoom from a ~95–285 km parent tile down to a local tangent-plane **~0.5–1.0 m hexagonal-prism 3D volume** with underground stratigraphy, caves, ore veins, monotonic carved rivers, and physical surficial soil. Surface landforms and underground geology emerge from laterally continuous neighbor-correlated stratigraphy (with realistic pinch-out wedges and continuous structural dip/folding) and six composable physical geomorphology operators. Expose a thin `tec11-wasm` `LocalTerrainSession` wrapper and an isolated, paused-only **Local 3D Terrain** modal in the World UI featuring progressive multi-step 3D hex-column drill-down, 3D cutaway/orbit inspection, interactive Z-level slicing, and click-to-inspect voxel details—plus a self-contained binary `DeltaOverlay` codec for downstream games.

## Technical Context

**Language/Version**: Rust (edition 2021) + WebAssembly (`wasm-bindgen`) + PureScript / React + Three.js  
**Primary Dependencies**: `tec11-proto`, `tec11-math-grid`, `tec11-climate`, `tec11-geology` (new crate `tec11-local-terrain` has zero browser/WASM dependencies)  
**Storage**: Read-only `.tec11` `PlanetSnapshot` bytes + self-contained versioned binary `DeltaOverlay` (`T11D`)  
**Testing**: `bazel test -c opt --incompatible_strict_action_env --disk_cache=~/.cache/bazel-disk-cache //crates/tec11-local-terrain/... //crates/tec11-geology/... //crates/tec11-proto/...` + `bazel build //:web_bundle`  
**Target Platform**: Native Rust (Linux/macOS/Windows) and WebAssembly (`wasm32-unknown-unknown`) in modern browsers  
**Performance Goals**: `< 500 ms` native / `< 1.5 s` WASM to generate 128×128×128 volume (SC-001); `< 50 ms` Z-level slice update (SC-003)  
**Constraints**: Peak memory `< 16 MB` in WASM for 128×128×256 volume + mesh (SC-002); zero mutation to base World simulation (FR-009); pentagon-safe tangent projection  
**Scale/Scope**: 4 progressive zoom levels (`0..=3`), `16×16×32` 3D hex chunks, ~96 `MaterialId` entries covering all Tec11 rocks, surficial mantles, and ore veins

## Constitution Check

*GATE: Evaluated before Phase 0 research and re-verified post-Phase 1 design.*

- **I. Emergent Physical Processes Over Reified Abstractions**: PASS — All landforms and underground structures derive from physical operators (neighbor-correlated stratigraphy, `Rock::toughness` differential erosion, physical `ClimateModel` glacier cover + `soil_depth_m` scouring/loess, karst dissolution in high-`calcity` strata, monotonic river carving, coastal beveling) with zero "mountain age" or hardcoded biome-shape labels.
- **II. Model-Derived Stochasticity**: PASS — Every procedural detail (fold wavelengths, pinch-out wedges, river meanders, karst caverns, ore veins) is deterministically seeded from `world_seed` and scaled strictly by model-derived parameters (`mountainosity`, `toughness`, `calcity`, `precip_norm`, `slope`, `derive_layer_occurrences`).
- **III. Immutable Isolation for Secondary Views & Sandboxes**: PASS — `Local3dTerrainModal` constructs `LocalTerrainSession` from immutable `PlanetSnapshot` bytes (`session.to_snapshot_bytes()`) only when paused, and disposes WebGL + WASM cleanly on close (matching `WeatherModal` precedent).
- **IV. Deterministic Simulation & Portable Snapshot Continuity**: PASS — Identical `(snapshot, parent_tile_id, center_xy_m, chunk_coord)` produces bit-for-bit identical `HexChunk` data; older snapshots omitting `soil_depth_m`/`soil_fertility` fall back cleanly via `TileGeology::from_proto_geological_tile`.
- **V. Crate Modularity & Clean WASM/UI Separation**: PASS — All terrain synthesis, chunking, meshing, and delta serialization live in pure Rust (`crates/tec11-local-terrain/`); `tec11-wasm` exposes thin `LocalTerrainSession` FFI buffers; PureScript/Three.js handles camera, raycasting, and modal controls without duplicating physical rules.

## Project Structure

### Documentation (this feature)

```text
specs/008-local-3d-terrain/
├── plan.md                 # This file
├── research.md             # Phase 0 architectural & algorithmic decisions
├── data-model.md           # Phase 1 canonical entities, MaterialId palette, chunk & delta formats
├── quickstart.md           # Phase 1 validation scenarios & Bazel commands
└── contracts/
    ├── rust-api.md         # Pure-Rust tec11-local-terrain public API
    └── wasm-ui.md          # tec11-wasm LocalTerrainSession & World UI modal contract
```

### Source Code (repository root)

```text
crates/tec11-local-terrain/
├── BUILD.bazel             # rust_library + rust_test targets (pure Rust, zero WASM deps)
└── src/
    ├── lib.rs              # Public API exports, LocalTerrainWorld, LocalVolumeConfig
    ├── anchor.rs           # PlanetSnapshot (Tectonics/Prehistory/History) -> LocalZoomAnchor & tangent frame
    ├── materials.rs        # MaterialId(u16) palette, HexPrismCell, RGB colors & inspection metadata
    ├── stratigraphy.rs     # Neighbor bed correlation, pinch-out wedges, structural dip/folding/faults/intrusions
    ├── rivers.rs           # Monotonic hierarchical RiverSplineSegment subdivision & valley/channel carving
    ├── geomorphology.rs    # Composable 6-operator surface & underground synthesis (erosion, glacial/fluvial, karst, soil/mantle, coast)
    ├── chunk.rs            # ChunkCoord, HexChunk (Uniform/RLE/Dense), LocalTerrainVolume streaming
    ├── delta.rs            # Self-contained versioned binary DeltaOverlay (T11D) codec
    └── mesher.rs           # Face-culled hex-prism mesher for Levels 0–2 patches & Level 3 Z-sliced volumes

crates/tec11-wasm/
├── BUILD.bazel             # Add //crates/tec11-local-terrain:tec11_local_terrain + local_terrain_session.rs
└── src/
    ├── lib.rs              # Export LocalTerrainSession
    └── local_terrain_session.rs  # Thin wasm_bindgen wrapper for progressive drill-down, Z-slicing, mesh buffers & picking

src/Tec11/
├── App.purs                # Add LocalTerrainModal variant, paused-only button, zoom/Z-slice/inspect controls
└── App.js                  # Add mountWorldLocalTerrainModal (Three.js orbit/cutaway/raycast + clean disposal)
```

## Phased Delivery Strategy

1. **Phase A — Foundation & Anchor (`tec11-local-terrain` scaffold)**: Bazel build, `MaterialId`/`HexPrismCell`, `LocalZoomAnchor::from_snapshot_bytes` across Tectonics/Prehistory/History with pentagon-safe tangent frame.
2. **Phase B — Continuous Stratigraphy, Geomorphology & Monotonic Rivers**: Neighbor bed correlation + pinch-outs + structural deformation (`stratigraphy.rs`), monotonic river spline subdivision & carving (`rivers.rs`), and the 6 physical geomorphology operators (`geomorphology.rs`).
3. **Phase C — Chunked Volume, Sparse Delta & Mesher**: `HexChunk` (`Uniform`/`RLE`/`Dense`), `LocalTerrainVolume` streaming & Level 0–2 `DrillDownPatch`, `DeltaOverlay` binary codec (`delta.rs`), and face-culled `LocalTerrainMesher` with fast Z-level slicing (`mesher.rs`).
4. **Phase D — WASM Wrapper & World UI Modal**: `LocalTerrainSession` in `tec11-wasm` (including `shift_z_window` for inspecting deep multi-layer crust down toward the Moho), `LocalTerrainModal` in `App.purs`, and Three.js cutaway/orbit/drill-down/raycast renderer in `App.js` with strict WebGL + WASM cleanup.
5. **Phase E — Comprehensive Verification**: Unit & integration tests covering all 6 success criteria (`SC-001`–`SC-006`) + `//:web_bundle` build.

## Complexity Tracking

No Constitution violations or unjustified complexity deviations.
