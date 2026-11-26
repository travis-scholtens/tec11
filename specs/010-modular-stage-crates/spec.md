# Feature Specification: Modular Stage-by-Stage Crate Architecture

**Feature Branch**: `010-modular-stage-crates`  
**Created**: 2026-11-26  
**Status**: Draft  
**Input**: Refactor the monolithic `tec11-geology` crate and bundled climate/math utilities into independent, minimal-dependency libraries per simulation stage, pure data domain, rendering/viewing helper, and sandbox, using existing `tec11-proto` schemas for cross-stage data handoffs and executing in incremental, independently green commits.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Standalone Foundational & Domain Libraries (Priority: P1)

As an application developer integrating a single TEC11 simulation model (such as procedural languages, cosmos/astronomy, hourly weather, tides, or Köppen climate classification), I want to depend on only that crate and its minimal mathematical/data prerequisites, without pulling in spherical grids, rendering helpers, plate tectonics, or interactive sandboxes.

**Why this priority**: Extracting zero-dependency and minimal-dependency leaf libraries (`tec11-rng`, `tec11-vec3`, `tec11-koeppen`, `tec11-tides`, `tec11-weather`, `tec11-language`, `tec11-cosmos`, `tec11-sky-view`) immediately eliminates the tightest artificial couplings and unlocks clean downstream stage boundaries.

**Independent Test**: Each leaf crate compiles and passes `bazel test` with strictly bounded `deps` verified in its `BUILD.bazel` (e.g., `tec11-language` depends only on `tec11-rng`; `tec11-cosmos` depends only on `tec11-vec3`, `tec11-rng`, and `tec11-proto`).

**Acceptance Scenarios**:

1. **Given** an external app wants procedural language generation, **When** it depends on `//crates/tec11-language`, **Then** its transitive internal dependencies consist solely of `//crates/tec11-rng` (zero `tec11-math-grid`, `tec11-climate`, `tec11-geology`, or `tec11-proto`).
2. **Given** an external app wants the cosmos/star/solar-system simulation without rendering it, **When** it depends on `//crates/tec11-cosmos`, **Then** it pulls only `tec11-vec3`, `tec11-rng`, and `tec11-proto` — with zero `SphericalGrid`, `PointTree`, `sky_vantage`, or `TileGeology` dependencies.
3. **Given** an app wants to render the sky dome or inspect tile sky vantage, **When** it depends on `//crates/tec11-sky-view`, **Then** it gets the celestial sphere viewer and `sky_vantage` / `inspect_tile_sky` adapters without forcing those rendering helpers into `tec11-cosmos`, `tec11-tectonics`, or `tec11-prehistory`.
4. **Given** an app wants hourly weather or tides independently of the core FV climate solver, **When** it depends on `//crates/tec11-tides` or `//crates/tec11-weather`, **Then** `tec11-tides` has no climate-model dependency, `tec11-climate` does not depend on `tec11-weather` or `tec11-koeppen`, and `tec11-koeppen` is importable independently.

---

### User Story 2 - Pure Geology Data Crate & Decoupled Tectonics / Rivers (Priority: P1)

As a maintainer of downstream planetary stages (`tec11-prehistory`, `tec11-history`, `tec11-local-terrain`), I want planetary geology data (`Rock`, `RockLayer`, `TileGeology`, `LithosphericGroup`, `planet_radius_km`) and post-tectonic coarse river tracing (`tec11-rivers`) separated from active plate tectonics (`tec11-tectonics`), so stages that only read resulting terrain never depend on plate movement or tectonic simulation logic.

**Why this priority**: Separating data (`tec11-geology`) from tectonic motion (`tec11-tectonics`) and post-tectonic hydrology (`tec11-rivers`) breaks the central bottleneck that previously forced every planetary stage and `tec11-local-terrain` to import the entire geology monolith.

**Independent Test**: `tec11-geology`, `tec11-rivers`, `tec11-tectonics`, and `tec11-local-terrain` all compile and pass `bazel test`, and neither `tec11-rivers` nor `tec11-local-terrain` (nor `tec11-prehistory`/`tec11-history`) has `tec11-tectonics` in its runtime `deps`.

**Acceptance Scenarios**:

1. **Given** `tec11-local-terrain` builds a local 3D world from a `.tec11` snapshot, **When** its `BUILD.bazel` `deps` are inspected, **Then** it depends on `tec11-geology` (data) and has zero runtime dependency on `tec11-tectonics`, `tec11-prehistory`, or `tec11-history`.
2. **Given** an app wants to run plate tectonics, **When** it imports `//crates/tec11-tectonics`, **Then** it pulls `tec11-geology`, `tec11-climate`, `tec11-math-grid`, and `tec11-proto` — with zero dependency on `tec11-rivers`, `tec11-koeppen`, `tec11-cosmos`, `tec11-language`, `tec11-prehistory`, or any sandbox crate.
3. **Given** an app wants coarse river tracing over tiles, **When** it imports `//crates/tec11-rivers`, **Then** it pulls only `tec11-geology` and `tec11-math-grid`.

---

### User Story 3 - Data-Decoupled Prehistory, History, and Top-Level World Orchestrator (Priority: P2)

As an application developer running a single planetary era (Prehistory or History) or the full planet lifecycle (`tec11-world`), I want `tec11-prehistory` to drop its `tec11-language` and `tec11-tectonics` dependencies, `tec11-history` to consume earlier-stage results via existing `tec11-proto` snapshots (`PlanetSnapshot`) rather than depending on `tec11-tectonics` or `tec11-prehistory` simulation loops, and `tec11-world` to orchestrate the complete multi-stage pipeline.

**Why this priority**: Using existing `tec11-proto` messages as the stage handoff boundary makes Prehistory and History genuinely independent simulation libraries while preserving 100% `.tec11` save compatibility.

**Independent Test**: `tec11-prehistory`, `tec11-history`, and `tec11-world` each compile and pass `bazel test`; existing `.tec11` snapshots load identically in `tec11-cli` and `//:web_bundle`.

**Acceptance Scenarios**:

1. **Given** `tec11-prehistory` runs biosphere, population, and racination, **When** its `BUILD.bazel` `deps` are inspected, **Then** it has zero dependency on `tec11-tectonics`, `tec11-language`, `tec11-cosmos`, or `tec11-sky-view`; continents and oceans receive deterministic numeric/structural IDs rather than generated language names.
2. **Given** `tec11-history` starts from completed Tectonics/Prehistory/Cosmos outputs, **When** it is constructed from a `tec11_proto::PlanetSnapshot`, **Then** it runs civilizations, diplomacy, language evolution (naming known continents/oceans per culture's language), calendars, and lore without depending on `tec11-tectonics` or `tec11-prehistory` simulation code.
3. **Given** an app wants the complete multi-stage planet lifecycle (`WorldSimulation`), **When** it imports `//crates/tec11-world`, **Then** `tec11-world` orchestrates Cosmos, Tectonics, Prehistory, History, and Weather transitions cleanly via proto/data handoffs.

---

### User Story 4 - Lean Per-Stage Sandbox Crates (Priority: P2)

As an application developer embedding a simulation stage (such as `tec11-tectonics` or `tec11-climate`), I want all interactive debugging/experiment sandboxes (`erosion`, `move`, `split`, `meteor`, `moon`, `tides`, `weather`) isolated in small dedicated sandbox crates outside the core simulation libraries, so production stage consumers never compile or link sandbox code.

**Why this priority**: Keeps every simulation library minimal and focused while preserving all existing web app sandbox modes with zero behavioral regressions.

**Independent Test**: Every sandbox crate builds and passes tests; `tec11-wasm` wires all sandbox sessions from the new sandbox crates and `//:web_bundle` builds cleanly.

**Acceptance Scenarios**:

1. **Given** a client app imports `tec11-tectonics`, **When** its dependency closure is inspected, **Then** none of `erosion_sandbox`, `move_sandbox`, or `split_sandbox` are included.
2. **Given** the web app (`tec11-wasm` + `//:web_bundle`) is built, **When** any sandbox mode is opened in the UI, **Then** all 7 sandbox modes (`erosion`, `move`, `split`, `meteor`, `moon`, `tides`, `weather`) work identically to before the refactor.

---

### User Story 5 - Staged, Independently Green Commits (Priority: P1)

As the repository owner reviewing and landing this refactor, I want the split executed in small, logical phases where every single phase leaves `bazel test //...` and `bazel build //:web_bundle` 100% green, so no mega-commit is ever introduced.

**Why this priority**: Incremental green commits make each boundary verifiable, reviewable, and safely reversible.

**Independent Test**: After each phase commit, `bazel test -c opt --incompatible_strict_action_env --disk_cache=~/.cache/bazel-disk-cache //crates/...` and `bazel build -c opt --incompatible_strict_action_env --disk_cache=~/.cache/bazel-disk-cache //:web_bundle` both exit 0.

---

### Edge Cases

- **Backward-compatible `.tec11` snapshots**: Zero wire-format breaking changes to `proto/tec11.proto`. Existing `.tec11` save files produced before this refactor MUST decode and simulate identically in `tec11-world`, `tec11-local-terrain`, `tec11-cli`, and `tec11-wasm`.
- **Deterministic PRNG streams**: Extracting `tec11-rng` and `tec11-vec3` must preserve bit-for-bit identical numerical behavior (`Pcg64Mcg`, `Vec3`) so existing simulation seeds produce matching results (except where Prehistory intentionally drops procedural continent/ocean language naming).
- **WASM & PureScript surface stability**: `crates/tec11-wasm` exported `#[wasm_bindgen]` symbols and JS/PureScript FFI signatures remain unchanged throughout all phases.
- **Test-only vs. runtime dependencies**: Where a downstream crate's unit tests construct a synthetic snapshot via an upstream stage (e.g., `tec11-local-terrain` tests generating an Earth snapshot via `PrehistorySimulation`), Bazel `rust_test` `deps` MUST carry that test-only dependency rather than leaking it into `rust_library` `deps`.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001 (Zero-Dep Math & RNG Foundations)**: Extract `//crates/tec11-rng` (`Pcg64Mcg`, zero internal deps) and `//crates/tec11-vec3` (`Vec3` and vector ops, zero internal deps). `//crates/tec11-math-grid` depends on and re-exports them.
- **FR-002 (Climate Domain Split)**: Split `//crates/tec11-climate` into:
  - `//crates/tec11-koeppen` (Köppen classification, `koeppen_climate_name`, `koeppen_rgb`; zero climate-solver dep)
  - `//crates/tec11-tides` (`LaplaceTidalSolver`, `TidalMoonBody`, `AtmosphericTideForcing`)
  - `//crates/tec11-weather` (hourly synoptic weather; depends on `tec11-climate`, `tec11-tides`, `tec11-math-grid`)
  - `//crates/tec11-climate` (FV climate + ocean currents; depends on `tec11-math-grid` only)
- **FR-003 (Standalone Language & Cosmos + Separate Sky View)**:
  - Extract `//crates/tec11-language` depending ONLY on `tec11-rng`.
  - Extract `//crates/tec11-cosmos` (pure star/planet/orbital/optics simulation + `data/hipparcos_naked.tsv`) depending ONLY on `tec11-vec3`, `tec11-rng`, and `tec11-proto`.
  - Extract `//crates/tec11-sky-view` (celestial sphere viewer + `sky_vantage.rs` + `inspect_tile_sky` adapters) as a dedicated viewing library so simulation crates never pull in sky rendering code.
- **FR-004 (Pure Geology Data, Rivers, and Isolated Tectonics)**:
  - Narrow `//crates/tec11-geology` to pure planetary geology data (`Rock`, `RockKind`, `RockLayer`, `TileGeology`, `TileSourceColumn`, `LithosphericGroup`, `planet_radius_km`) depending only on `tec11-math-grid`, `tec11-vec3`, `tec11-rng`, and `tec11-proto`.
  - Extract `//crates/tec11-rivers` (`trace_coarse_rivers`) depending only on `tec11-geology` and `tec11-math-grid`.
  - Extract `//crates/tec11-tectonics` (`movement`, `pangea`, `tectonics`, `profile`) depending on `tec11-geology`, `tec11-climate`, `tec11-math-grid`, and `tec11-proto` (zero `rivers`, `koeppen`, `cosmos`, `language`, `prehistory`, `sky-view`, or sandboxes).
- **FR-005 (Decoupled Prehistory, Proto-Handoff History, and Top-Level World)**:
  - Extract `//crates/tec11-prehistory` (`lifeforms`, `population`, `racination`, `prehistory`, `terrain`, `data/animals.tsv`, `data/plants.tsv`) with ZERO dependency on `tec11-tectonics`, `tec11-language`, `tec11-cosmos`, or `tec11-sky-view`. Drop language-generated continent/ocean names from Prehistory.
  - Extract `//crates/tec11-history` (`history.rs`, `data/hist.pgm`) consuming prior-stage state via `tec11-proto::PlanetSnapshot` (and `tec11-geology`, `tec11-rivers`, `tec11-climate`, `tec11-koeppen`, `tec11-language`, `tec11-cosmos` metadata/optics as needed, with ZERO dependency on `tec11-tectonics` or `tec11-prehistory` simulation code).
  - Extract `//crates/tec11-world` (`world.rs`) as the top-level full-lifecycle orchestrator wiring Cosmos, Tectonics, Prehistory, History, and Weather.
- **FR-006 (Minimal Per-Stage Sandbox Crates)**: Extract all 7 sandboxes out of core stage libraries into lean, stage-scoped sandbox crates (`tec11-tectonics-sandboxes`, `tec11-planet-sandboxes`, `tec11-tides-sandbox`, `tec11-weather-sandbox`).
- **FR-007 (Shared Proto Preserved)**: Keep a single `//crates/tec11-proto` crate with zero breaking wire-format changes to `proto/tec11.proto`.
- **FR-008 (Incremental Green Phases)**: Execute the refactor across 6 self-contained phases, each passing all Rust tests and building `//:web_bundle` cleanly before moving to the next phase.

### Key Entities & Crate Boundaries

- **Foundations**: `tec11-rng`, `tec11-vec3`, `tec11-math-grid`, `tec11-proto`, `tec11-speech`, `tec11-koeppen`
- **Data & Domain Libraries**: `tec11-geology` (data only), `tec11-rivers`, `tec11-tides`, `tec11-climate`, `tec11-weather`, `tec11-language`, `tec11-cosmos`, `tec11-sky-view`
- **Simulation Stage Libraries**: `tec11-tectonics`, `tec11-prehistory`, `tec11-history`, `tec11-local-terrain`
- **Top-Level Orchestrator**: `tec11-world`
- **Sandbox Libraries**: `tec11-tectonics-sandboxes`, `tec11-planet-sandboxes`, `tec11-tides-sandbox`, `tec11-weather-sandbox`
- **Application Entrypoints**: `tec11-cli`, `tec11-wasm`

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: `bazel query 'deps(//crates/tec11-language:tec11_language)'` contains `//crates/tec11-rng` and zero other `//crates/*` targets.
- **SC-002**: `bazel query 'deps(//crates/tec11-cosmos:tec11_cosmos)'` contains only `tec11-vec3`, `tec11-rng`, and `tec11-proto` among `//crates/*` targets (zero `tec11-math-grid`, `tec11-sky-view`, or `tec11-geology`).
- **SC-003**: `bazel query 'deps(//crates/tec11-tectonics:tec11_tectonics)'` contains zero `tec11-rivers`, `tec11-koeppen`, `tec11-cosmos`, `tec11-language`, `tec11-prehistory`, `tec11-sky-view`, or sandbox targets.
- **SC-004**: `bazel query 'deps(//crates/tec11-prehistory:tec11_prehistory)'` contains zero `tec11-tectonics`, `tec11-language`, `tec11-cosmos`, `tec11-sky-view`, or sandbox targets.
- **SC-005**: `bazel query 'deps(//crates/tec11-history:tec11_history)'` contains zero `tec11-tectonics`, `tec11-prehistory`, or sandbox targets (stage handoff runs through `tec11-proto::PlanetSnapshot`).
- **SC-006**: `bazel query 'deps(//crates/tec11-local-terrain:tec11_local_terrain)'` contains `tec11-geology` (data) and zero `tec11-tectonics`, `tec11-prehistory`, or `tec11-history` targets at runtime.
- **SC-007**: Every phase commit passes `bazel test -c opt --incompatible_strict_action_env --disk_cache=~/.cache/bazel-disk-cache //crates/...` and `bazel build -c opt --incompatible_strict_action_env --disk_cache=~/.cache/bazel-disk-cache //:web_bundle` with zero failures and zero `.tec11` save regressions.
