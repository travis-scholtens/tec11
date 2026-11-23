# Feature Specification: Core Platform (Geodesic Grid, Snapshot Protobufs & Native CLI)

**Feature Branch**: `006-platform-and-snapshots`

**Created**: 2026-11-23

**Status**: Implemented (Baseline Spec)

**Input**: Baseline specification for Tec11's shared platform infrastructure: spherical geodesic grid geometry (`tec11-math-grid`), protobuf snapshot serialization (`proto/tec11.proto` / `tec11-proto`), and native headless simulation/profiling CLI (`tec11-cli`).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Uniform Geodesic Sphere & Coarse-to-Fine Subdivision (Priority: P1)

All planetary simulations run on a uniform Goldberg-polyhedron hexagonal geodesic sphere (with 12 pentagonal tiles ~33% smaller than hexagons) at selectable subdivision levels (`grid_size` 2–7, with `N = 2 + 10 × 3^(size - 1)` faces). Higher-resolution stages (History land terrain, Weather, Tides) deterministically subdivide coarse tiles onto finer spherical grids while preserving exact parent-child mapping (`FineParentage`) and orogeny-scaled sub-grid relief.

**Why this priority**: Every physical, ecological, and historical calculation depends on uniform spherical area, distortion-free neighbor adjacency, and consistent coarse-to-fine mapping across the globe (avoiding Mercator/lat-lon pole singularities).

**Independent Test**: Construct a `SphericalGrid` for sizes 4–7 and a `FullSphereFineGrid` / `SubdividedTerrainGrid` from a coarse planet; verify exact face counts, 5-neighbor pentagons (exactly 12), 6-neighbor hexagons, unit-sphere vertex normalization, and valid parent-tile indices for every fine face.

**Acceptance Scenarios**:

1. **Given** any supported `grid_size` $s \in \{4, 5, 6, 7\}$, **When** constructing a `SphericalGrid`, **Then** it contains exactly $12$ pentagonal tiles (5 neighbors each) and all remaining tiles are hexagonal (6 neighbors each), with symmetric neighbor adjacency and finite spherical areas/normals.
2. **Given** a coarse planet with elevation, mountainosity, and crustal thickness, **When** subdividing onto a fine grid (`SubdividedTerrainGrid` for History or `FullSphereFineGrid` at `coarse + 2` for Weather/Tides), **Then** sub-grid relief scales with each coarse tile's orogenic activity and crustal thickness rather than uniform noise, and every fine tile maps back to a valid primary coarse parent.

---

### User Story 2 - Portable Binary Planet Snapshots (`PlanetSnapshot`) (Priority: P2)

A user (or the native CLI) exports any planet state at any stage (Tectonics, Prehistory, or History) to a compact `.tec11` binary protobuf file defined by `proto/tec11.proto`, and reloads it later in the browser World simulation, in standalone sandboxes (Climate, Terrain, Tides, Weather, etc.), or in `tec11-cli` to continue stepping from the exact saved state.

**Why this priority**: Snapshots are the universal interchange format linking browser sessions, headless CLI generation, and isolated sandboxes (including the paused-only World Weather modal).

**Independent Test**: Run a World simulation to each stage (Tectonics, Prehistory, History), encode to `PlanetSnapshot` bytes, decode into a fresh session, and verify identical tile count, stage, age, PRNG state, planetary environment (star, moons, atmosphere, galaxy), and stage-specific state.

**Acceptance Scenarios**:

1. **Given** a World simulation in Tectonics, Prehistory, or History, **When** `encode_snapshot()` is called, **Then** it produces a valid `PlanetSnapshot` protobuf payload without mutating the source simulation.
2. **Given** a valid `.tec11` snapshot file, **When** loaded in the World UI, a standalone sandbox, or `tec11-cli --input`, **Then** the planet's grid size, elevations, climate, environment, PRNG state, and stage state are restored cleanly; malformed or unsupported bytes fail gracefully (returning `false` / `None`) without panicking.

---

### User Story 3 - Native Headless World Generation & Profiling CLI (`tec11-cli`) (Priority: P3)

A developer or power user runs `bazel run //:tec11_cli -- [OPTIONS]` to generate or resume `.tec11` worlds natively at full compiled speed (matching all World Simulation GUI parameters including `--randomize`, `--seed`, `--grid-size`, orbital/stellar/atmospheric flags, `--target-stage`, and step counts), optionally collecting per-phase timing and heap-allocation / RSS memory profiles via `--profile`.

**Why this priority**: Enables rapid batch world generation, reproducible regression testing, and performance/memory profiling without browser/WASM overhead.

**Independent Test**: Run `tec11_cli` with a fixed `--seed`, `--target-stage history`, `--profile`, and `-o /tmp/test.tec11`, verify exit code 0, non-empty output file, printed profiling breakdown, and successful reload via `--input /tmp/test.tec11`.

**Acceptance Scenarios**:

1. **Given** CLI flags matching GUI creation parameters (or `--randomize` with selective overrides), **When** `tec11_cli` runs, **Then** it simulates to the requested `--target-stage` (and optional `--prehistory-ticks` / `--history-ticks` / `--exact-steps`) and writes a valid `.tec11` snapshot to `--output`.
2. **Given** `--profile` is enabled, **When** `tec11_cli` completes, **Then** it reports per-phase wall time, heap allocation counts/bytes (via `ProfiledAllocator`), and OS peak memory (`VmRSS` / `VmPeak` on Linux).

---

### Edge Cases

- What happens when loading an older `.tec11` snapshot missing newly added optional protobuf fields (e.g., `environment` or newer stage fields)? Protobuf defaults and sensible fallbacks reconstruct missing fields cleanly without rejecting older saves.
- What happens when `tec11-cli` receives invalid flags or an unreadable `--input` path? It prints a clear diagnostic message to stderr and exits with a non-zero exit code.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST provide a deterministic spherical hexagonal geodesic grid (`tec11-math-grid`) with 12 pentagons, symmetric adjacency, spatial point lookup, deterministic PRNG (`Pcg64Mcg`), and coarse-to-fine subdivision with orogeny-scaled relief and parent tracking.
- **FR-002**: System MUST define a version-tolerant binary protobuf schema (`proto/tec11.proto` / `tec11-proto`) encapsulating grid metadata, PRNG state, `PlanetaryEnvironment`, and a stage oneof (`Tectonics`, `Prehistory`, `History`).
- **FR-003**: Snapshot encoding MUST be strictly read-only (`&self`) so sandboxes and modals can snapshot a paused simulation without mutating it.
- **FR-004**: System MUST provide a native CLI (`tec11-cli`) supporting creation, `--randomize`, `--input` resumption, `--output` export, stage/step targeting, and `--profile` timing/memory instrumentation.

### Key Entities

- **SphericalGrid**: Geodesic face/vertex mesh, neighbor adjacency list, face normals, and area metrics.
- **SubdividedTerrainGrid / FullSphereFineGrid**: Fine-resolution grids derived from a coarse grid with `FineParentage` links and routed river networks.
- **PlanetSnapshot**: Top-level protobuf message storing `grid_size`, `radius_km`, `spin`, `tilt_deg`, `rng_state_16b`, `PlanetaryEnvironment`, and `PlanetStageOneof`.
- **CliConfig**: Parsed CLI configuration mirroring all World simulation creation parameters plus execution and profiling controls.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: `SphericalGrid` construction and adjacency checks pass all `tec11-math-grid` unit tests with zero non-symmetric edges and exact Euler characteristic ($\chi = 2$).
- **SC-002**: Round-tripping any World stage through `encode_snapshot()` → `load_snapshot_bytes()` preserves stage, tile count, age, and environment with 100% fidelity.
- **SC-003**: `tec11-cli` generates a complete Medium (7,292-tile) world through History and writes a valid `.tec11` file loadable in the web UI.

## Assumptions

- `.tec11` files are raw serialized protobuf bytes (no external wrapper archive required).
- `tec11-cli` is invoked via Bazel (`bazel run -c opt //:tec11_cli -- ...`) on Linux/macOS/Windows; OS `VmRSS`/`VmPeak` inspection is Linux-specific with graceful `None` fallback on other platforms.
