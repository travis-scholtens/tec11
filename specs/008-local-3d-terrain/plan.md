# Implementation Plan: Local 3D Hex-Prism Terrain & Underground Zoom

**Branch**: `008-local-3d-terrain` | **Date**: 2026-11-24 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/008-local-3d-terrain/spec.md`

## Summary

Introduce a pure-Rust, browser-independent crate `tec11-local-terrain` that takes immutable `.tec11` `PlanetSnapshot` bytes (across Tectonics, Prehistory, and History stages, grounding coastlines and mainstem rivers 1:1 on History's `SubdividedTerrainGrid` when in History and on the coarse geology grid in Tectonics/Prehistory) and performs seamless 4-level lazy zoom (`Level 0 → 1 → 2 → 3`) down to a local tangent-plane **~0.5–1.0 m hexagonal-prism 3D volume**. Terrain surface height is constructed **hydrology-first**: a continuous, anchor-invariant vector **`DrainageNetwork`** (Chaikin $C^1$ mainstems, multi-order tributaries snapping strictly downhill to parent $Z_{\text{water}}$, Leopold-Maddock channel widths with braided sandbars and estuaries, and climate/evaporation/permeability/freezing-gated structural `LakeBasin`s) establishes a strictly monotonic lower water-surface envelope, and land elevation rises **strictly upward** from the drainage floor ($z = z_{\text{valley}} + R(d, x, y)$, $R \ge 0$) driven by Tec11's `mountainosity`, `Rock::toughness`, glacial U-valley vs. fluvial V-valley profiles, and neighbor-correlated stratigraphy — eliminating floating aqueducts, uphill waterfalls, upsampling beads, and tile seams while staying $O(1)$ (`< 50 ms` in WASM). Expose a thin `tec11-wasm` `WasmLocalTerrainSession` wrapper with `< 1 ms` pan re-anchoring (`PlanetaryTerrainContext`) and an isolated, paused-only **Local 3D Terrain** modal in the World UI.

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
    ├── anchor.rs           # PlanetaryTerrainContext (with History SubdividedTerrainGrid grounding) -> LocalZoomAnchor & tangent frame
    ├── materials.rs        # MaterialId(u16) palette, HexPrismCell, RGB colors & inspection metadata
    ├── stratigraphy.rs     # Neighbor bed correlation, pinch-out wedges, structural dip/folding/faults/intrusions
    ├── rivers.rs           # Hydrology-first DrainageNetwork (anchor-invariant Chaikin mainstems, multi-order tributaries, climate/rock-gated LakeBasins)
    ├── geomorphology.rs    # Non-negative relief envelope (z = z_valley + R(d, x, y), R >= 0) + 6 physical geomorphology operators
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

1. **Phase A — History Fine-Grid Grounding (`anchor.rs`)**: Extend `PlanetaryTerrainContext` to reconstruct History's `SubdividedTerrainGrid` (`from_coarse_with_crust` + `route_rivers`), populating `anchor.fine_samples` and directed History fine `river_edges` for a 1:1 match with the Regional View.
2. **Phase B — Hydrology-First Vector Drainage Network (`rivers.rs`)**: Build `DrainageNetwork` with anchor-invariant Chaikin $C^1$ mainstems (strictly non-increasing $Z_{\text{water}}$ even through high mountain tiles), deterministic 1st/2nd-order tributaries snapping cleanly to parent $Z_{\text{water}}$, Leopold-Maddock width/depth/estuary flaring & braided sandbars, and climate/evaporation/permeability/freezing-gated `LakeBasin`s.
3. **Phase C — Non-Negative Relief Envelope (`geomorphology.rs`, `chunk.rs`, `lib.rs`, `mesher.rs`)**: Construct terrain strictly upward from the drainage floor ($z = z_{\text{valley}} + R(d, x, y)$, $R \ge 0$, $R(0) = 0$) with glacial U-valley ($u^2$) vs. fluvial V-valley ($u$) wall profiles, `mountainosity`-scaled ridges, `Rock::toughness`-stepped plateaus, lake basin carving/freezing/salt flats, coastal beaches, and Level 3 `z_max` auto-fit so coastal/mountain hills are never decapitated.
4. **Phase D — Verification & Tests**: Comprehensive Rust unit tests verifying strict downstream monotonicity across all mainstems & tributaries, `surface_elev_m >= z_water` everywhere outside channels (zero aqueducts, zero uphill waterfalls), climate/permeability/freezing lake gating, History 1:1 grounding, and `//:web_bundle` build.

## Complexity Tracking

No Constitution violations or unjustified complexity deviations.
