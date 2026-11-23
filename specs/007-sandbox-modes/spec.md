# Feature Specification: Standalone Model & Inspector Sandbox Modes

**Feature Branch**: `007-sandbox-modes`

**Created**: 2026-11-23

**Status**: Implemented (Baseline Spec)

**Input**: Baseline specification for the standalone individual model/sandbox screens accessible from the Tec11 Main Menu.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Main Menu Access to Isolated Sub-Model Sandboxes (Priority: P1)

From the Main Menu, a user can either launch the integrated **Start (World Simulation)** or open **Grid** and the collapsible sub-model menu to launch any standalone model sandbox in isolation, allowing focused inspection, debugging, and experimentation with individual physical or historical subsystems without stepping through an entire planetary lifecycle.

**Why this priority**: Standalone sandboxes allow rapid verification and interactive exploration of each model (geology, astronomy, climate, oceanography, biology, linguistics, history) independently.

**Independent Test**: Load the web app, verify the Main Menu renders the World Simulation button, Grid button, accordion of standalone sandboxes, and GitHub project link; open any sandbox screen, interact with it, and click **Done** to return cleanly to the Main Menu.

**Acceptance Scenarios**:

1. **Given** the Main Menu after WASM initializes, **When** the user clicks any sandbox button, **Then** the corresponding WASM session mounts on the interactive 3D globe (or inspector canvas) and its controls become active.
2. **Given** an active standalone sandbox screen, **When** the user clicks **Done** to return to the Main Menu, **Then** any running timers stop, WebGL resources dispose cleanly, and the WASM session is freed.

---

### User Story 2 - Geological, Astronomical & Oceanographic Sandboxes (Priority: P2)

Users can independently experiment with:
- **Grid** (`GridScreen`): Inspect the hexagonal/pentagonal geodesic sphere tessellation and subdivision sizes.
- **Tectonic & Geological Mechanics**: **Pangea** (`PangeaScreen`), **Split** (`SplitScreen`), **Move** (`MoveScreen`), **Erosion** (`ErosionScreen`), **Tectonics** (`TectonicsScreen`), **Rivers** (`RiversScreen`), and **Terrain** (`TerrainScreen`, subdivided fine terrain).
- **Astronomy, Climate & Oceans**: **Moons** (`MoonScreen`), **Meteors** (`MeteorScreen`), **Sun & Sky** (`SunScreen`), **Climate** (`ClimateScreen`), **Tides** (`TidesScreen`), and **Weather** (`WeatherScreen`).

**Why this priority**: Lets developers and users isolate physical behaviors (e.g., continental rifting, river routing, tidal scour, Rayleigh sky optics, baroclinic storms) and test edge-case parameters directly.

**Independent Test**: Open any sandbox (e.g., `Climate`, `Tides`, `Sun & Sky`, `Terrain`), adjust its parameters or load Earth / a saved `.tec11` planet, step the model, and verify responsive visual updates.

**Acceptance Scenarios**:

1. **Given** a snapshot-aware sandbox (`Climate`, `Terrain`, `Tides`, `Weather`, `Prehistory`, `History`), **When** the user loads Earth, the last saved planet, or a `.tec11` file from disk, **Then** the sandbox reinitializes on that planet's geometry and environment.
2. **Given** an astronomy or geology sandbox (`Moons`, `Meteors`, `Sun & Sky`, `Split`, `Move`, `Erosion`), **When** the user adjusts sliders/selectors and steps, **Then** the visualization updates immediately without requiring a full World run.

---

### User Story 3 - Biological, Cultural & Historical Sandboxes (Priority: P3)

Users can independently test downstream life and civilization models:
- **Lifeforms** (`LifeformsScreen`): Biome flora/fauna generation and inspection.
- **Population** (`PopulationScreen`), **Racination** (`RacinationScreen`), and **Prehistory** (`PrehistoryScreen`): Carrying capacity, migration, heritage branching, and glacial cycles.
- **Language** (`LanguageScreen`): Standalone language evolution, sound changes, vocabulary, and Klatt-88 IPA audio playback.
- **Travel** (`TravelScreen`): Movement cost and pathfinding across terrain/oceans.
- **History** (`HistoryScreen`): Standalone subdivided historical simulation with states, borders, trade, and chronicles.

**Why this priority**: Allows targeted testing of biological, linguistic, pathfinding, and political dynamics without waiting through hundreds of millions of years of tectonics.

**Independent Test**: Open `Language`, `Travel`, or `History` from the Main Menu, run steps or click tiles on the globe, and verify interactive inspection output.

**Acceptance Scenarios**:

1. **Given** the standalone `Language` sandbox, **When** the user evolves languages and triggers audio playback, **Then** sound changes apply and IPA speech synthesizes cleanly.
2. **Given** the standalone `Travel` sandbox, **When** the user selects origin/destination tiles, **Then** terrain- and sea-aware travel costs and paths render on the globe.

---

### Edge Cases

- What happens if WASM has not finished loading when the Main Menu first appears? All simulation/sandbox buttons remain disabled until `wasmReady` is true.
- What happens if a user loads an invalid file in a sandbox's file picker? The sandbox ignores the invalid payload and keeps its current valid session running without crashing.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST provide a Main Menu exposing **Start (World Simulation)**, **Grid**, a collapsible section of standalone model sandboxes, and the project repository URL at the bottom.
- **FR-002**: Each standalone sandbox MUST encapsulate its own WASM session (`tec11-wasm`) and cleanly stop timers, dispose Three.js WebGL resources, and free WASM memory on unmount.
- **FR-003**: Snapshot-compatible sandboxes (`Climate`, `Terrain`, `Tides`, `Weather`, `Prehistory`, `History`) MUST support loading canned Earth data, the browser's last saved planet, or an uploaded `.tec11` snapshot file.
- **FR-004**: Standalone sandboxes MUST reuse the exact same core Rust simulation crates (`tec11-geology`, `tec11-climate`, `tec11-math-grid`, `tec11-speech`) as the integrated World simulation so behaviors remain consistent.

### Key Entities

- **Screen**: Enum of all top-level UI screens (`MainMenu`, `WorldScreen`, and the 21 standalone model/inspector screens).
- **Wasm*Session**: Per-sandbox WASM wrapper classes (`WasmGridSession`, `WasmTectonicsSession`, `WasmClimateSession`, `WasmTidesSession`, `WasmWeatherSession`, `WasmHistorySession`, etc.).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Every sandbox listed on the Main Menu mounts without console/WebGL errors and cleans up without leaking WASM sessions when returning to `MainMenu`.
- **SC-002**: Snapshot-compatible sandboxes load any valid `.tec11` file produced by either `WorldScreen` or `tec11-cli`.

## Assumptions

- Standalone sandboxes are primarily exploration, educational, and debugging tools; the integrated **World Simulation** remains the primary end-to-end user experience.
