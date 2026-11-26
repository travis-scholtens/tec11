# Implementation Plan: Volcanoes, Calderas, and 3D Magmatic Plumbing

**Branch**: `009-volcanoes-and-calderas` | **Date**: 2026-11-25 | **Spec**: [spec.md](file:///home/travis/tec11/specs/009-volcanoes-and-calderas/spec.md)

**Input**: Feature specification from `/specs/009-volcanoes-and-calderas/spec.md`

## Summary

Add a deterministic, anchor-invariant volcanic terrain & 3D magmatic plumbing subsystem (`volcanoes.rs`) to `tec11-local-terrain`. Using only persisted `TileGeology` signals (`subduction`, `intrusion`, top rock kind & `felsity`, `mountainosity`, `elevation`), the system synthesizes four physically distinct morphologies (`Stratovolcano`, `ShieldVolcano`, `ResurgentCaldera`, `VolcanicNeck`), flank `ParasiticCone`s, climate- & permeability-gated crater lakes, Level 3 vertical conduits/radial dikes/sulfur crusts/basalt lava tubes, and `Jump to:` volcanic POIs — composing cleanly with the Spec 008 Hydrology-First Vector Drainage Network and requiring zero `.tec11` schema changes.

## Technical Context

**Language/Version**: Rust 2021 (`tec11-local-terrain`, `tec11-wasm`), PureScript/Three.js frontend (unchanged — consumes existing POI & mesh buffers)  
**Primary Dependencies**: `tec11-geology`, `tec11-math-grid`, `tec11-climate`, `tec11-proto` (all existing workspace deps)  
**Storage**: N/A (read-only over immutable `.tec11` `PlanetSnapshot` bytes; sparse `DeltaOverlay` unchanged)  
**Testing**: `bazel test -c opt --incompatible_strict_action_env --disk_cache=~/.cache/bazel-disk-cache //crates/tec11-local-terrain/...`  
**Target Platform**: Native Rust (tests) + WebAssembly (`wasm32-unknown-unknown` via `//:web_bundle`)  
**Project Type**: Pure Rust simulation library + thin WASM session wrapper  
**Performance Goals**: `< 5 ms` `VolcanicField::build`; `< 50 ms` Level 3 `56×56×72` volume mesh generation (`SC-005`)  
**Constraints**: Zero `.tec11` breaking changes (`SC-001`), `< 0.01 m` re-anchor invariance (`SC-004`), strict river monotonicity (`SC-003`)  
**Scale/Scope**: 1 new module (`volcanoes.rs`) + surgical hooks in `anchor.rs`, `geomorphology.rs`, `chunk.rs`, `lib.rs`, `BUILD.bazel`

## Constitution Check

- **I. Emergent Physical Processes**: PASS — All 4 volcanic morphologies, crater-lake states, sulfur fumaroles, and lava tubes emerge from persisted `subduction`, `intrusion`, rock `felsity`/`kind`/`toughness`, `mountainosity`, and climate; zero reified labels.
- **II. Model-Derived Stochasticity**: PASS — Placement, radii, and parasitic cone counts are weighted by `subduction`, `mountainosity`, and rock composition, never uniform dice rolls on cratons.
- **III. Immutable Isolation**: PASS — Operates strictly inside `tec11-local-terrain` on immutable decoded `PlanetSnapshot` data.
- **IV. Deterministic Simulation & Snapshot Continuity**: PASS — Canonical 3D sphere positioning seeded by `(tile_id, world_seed)` guarantees bit-identical results on save/reload and `< 0.01 m` continuity across re-anchoring.
- **V. Crate Modularity & Clean WASM/UI Separation**: PASS — 100% of volcanic logic lives in pure Rust `tec11-local-terrain`; `tec11-wasm` and `src/Tec11/` require zero duplicate logic.

## Project Structure

### Documentation (this feature)

```text
specs/009-volcanoes-and-calderas/
├── spec.md              # Feature specification
├── plan.md              # This implementation plan
├── research.md          # Phase 0 design decisions
├── data-model.md        # Entity & field definitions
├── quickstart.md        # Validation & test guide
├── checklists/
│   └── requirements.md  # Spec quality checklist
└── contracts/
    └── rust-api.md      # Public Rust & WASM contracts
```

### Source Code (repository root)

```text
crates/tec11-local-terrain/
├── BUILD.bazel               # Add src/volcanoes.rs to rust_library
└── src/
    ├── volcanoes.rs          # NEW: VolcanicField, VolcanicEdifice, ParasiticCone, query & 3D plumbing helpers
    ├── anchor.rs             # Expose NeighborTileSample.has_intrusion & canonical sphere volcanic_seeds
    ├── geomorphology.rs      # Apply volcanic relief before river envelope; 3D conduits, dikes, sulfur, lava tubes
    ├── chunk.rs              # Hold Arc<VolcanicField> in LocalTerrainVolume
    └── lib.rs                # Wire Arc<VolcanicField>, volcanic POIs, and comprehensive unit tests
```

**Structure Decision**: Keep all volcanic synthesis encapsulated in `crates/tec11-local-terrain/src/volcanoes.rs` with minimal wiring in `anchor.rs`, `geomorphology.rs`, `chunk.rs`, and `lib.rs`. No changes needed in `tec11-geology`, `tec11-proto`, `tec11-wasm`, or PureScript UI!

## Complexity Tracking

No Constitution violations.
