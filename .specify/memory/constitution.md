# Tec11 World Simulation Constitution

## Core Principles

### I. Emergent Physical Processes Over Reified Abstractions
Every simulated property MUST emerge from physical, geological, biological, or social processes rather than being tracked as an artificial label or script. For example:
- Mountain size, shape, and mineral composition emerge from plate tectonics, erosion, and geological metamorphism rather than a stored "mountain age."
- Climate classifications (e.g., Köppen zones) are purely descriptive readouts for physical systems—never inputs into physical mechanics like glaciers, river headwaters, surface albedo, or terrain rendering. Because Köppen classes are anthropocentric by design, they MAY be used as shorthand only for human survivability, habitability, and passability.
- Political instability, empire collapse, trade routes, and linguistic divergence emerge from underlying demographics, geography, resource disparities, and contact rather than arbitrary scripted events.

### II. Model-Derived Stochasticity
Randomness and procedural noise MUST represent abstracted physical or historical sub-grid detail weighted by model-derived probabilities, never uniform or arbitrary dice rolls. For example:
- Continental rifting probability scales with continental size and lithospheric weight.
- Sub-grid terrain relief added during coarse-to-fine subdivision scales with each coarse tile's orogenic activity and crustal thickness.
- Inter-state conflict and migration probabilities derive from resource disparities, distance, terrain, and cultural/national identity strength.

### III. Immutable Isolation for Secondary Views & Sandboxes
Inspectors, modals, and sub-timestep sandboxes (such as the paused-only Weather modal inside the World simulation) MUST NOT mutate the primary simulation state. Any subdivision, stepping, or experimentation MUST operate on an isolated copy (e.g., via immutable `PlanetSnapshot` encoding/decoding) so the coarse base world remains identical to before the modal was opened.

### IV. Deterministic Simulation & Portable Snapshot Continuity
All simulation stages (Tectonics, Prehistory, History) MUST be reproducible from their seeded PRNG state and serializable into `PlanetSnapshot` protobuf bytes. Saving and reloading a planet—or handing a snapshot to standalone sandboxes (Weather, Tides, Terrain, Climate)—MUST preserve planetary geometry, orbital/stellar/atmospheric parameters, and stage state cleanly.

### V. Crate Modularity & Clean WASM/UI Separation
Simulation logic MUST live in pure, unit-testable Rust crates (`tec11-math-grid`, `tec11-climate`, `tec11-geology`, `tec11-proto`, `tec11-speech`) independent of browser APIs. `tec11-wasm` exposes thin, memory-safe FFI session wrappers; the PureScript/React + Three.js frontend (`src/Tec11/`) handles rendering, user interaction, and audio playback without duplicating physical simulation rules.

## Technical Constraints & Architecture

- **Grid Geometry**: Planets are tessellated as uniform hexagonal geodesic grids with 12 pentagons (~33% smaller). Coarse planetary simulation supports selectable grid sizes (Small / Medium / Large, up to 21,872 coarse tiles); History and Weather subdivide onto finer spherical grids as needed.
- **Multi-Stage Timescales**:
  - **Tectonics**: 5 Myr per step
  - **Prehistory**: 4,000 years per step
  - **History**: 200 years per step (on subdivided land terrain)
  - **Weather**: 1 hour per step (on a uniform `coarse + 2` finite-volume spherical grid)
- **Build & Deployment**: All Rust crates, WASM bindings, and the PureScript/esbuild web bundle are built hermetically with Bazel (`//:web_bundle`) and deployed to GitHub Pages via `.github/workflows/deploy-pages.yml`.
- **Browser Performance**: Simulations and WebGL globe rendering MUST remain interactive in a single browser tab without requiring a backend server.

## Development Workflow & Quality Gates

- **Specification-First**: New simulation features and behavioral changes should begin with a Spec Kit specification (`specs/###-feature-name/spec.md`) reviewed against this Constitution before implementation.
- **Verification**:
  - Rust simulation changes MUST pass relevant `bazel test` suites (`//crates/...`) with `-c opt --incompatible_strict_action_env --disk_cache=~/.cache/bazel-disk-cache`.
  - Frontend or WASM binding changes MUST build `//:web_bundle` cleanly with zero Rust, PureScript, or bundler errors.
- **Commit Hygiene**: Keep each distinct feature or fix in its own focused commit with a clear description. Preserve existing comments and documentation unrelated to the change.

## Governance

This Constitution supersedes ad-hoc conventions for all simulation modeling, architecture, and UI decisions in Tec11. Amendments MUST update this file, increment the version according to semantic versioning, and record the amendment date. All feature specifications and plans MUST verify alignment with Core Principles I–V.

**Version**: 1.0.0 | **Ratified**: 2026-11-23 | **Last Amended**: 2026-11-23
