# Feature Specification: Weather (Global Coupled Weather, Clouds & Storm Surge)

**Feature Branch**: `005-weather`

**Created**: 2026-11-23

**Status**: Implemented (Baseline Spec)

**Input**: Baseline specification for the Weather simulation part (both standalone Weather mode and the paused-only World Weather modal) in Tec11.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Live Global Weather on an Isolated Copy of the World (Priority: P1)

While paused in the main World simulation on a planet that has developed an atmosphere, the user clicks the **Weather** button to open a live Weather modal running on a completely isolated, subdivided copy of the current planet (`coarse + 2` finite-volume spherical grid). Stepping weather forward at 1 hour per step evolves baroclinic cyclones, fronts, clouds, precipitation, upper-level jets, and lunar/solar tides without modifying or subdividing the underlying World simulation.

**Why this priority**: Lets users experience real-time hourly meteorology on any planet they have evolved while strictly preserving Core Principle III (zero mutation to the coarse World simulation).

**Independent Test**: Pause a World simulation after an atmosphere develops, click **Weather**, step +24h several times, close the modal, and verify that the base World simulation's tile count, stage, age, and state are completely unchanged.

**Acceptance Scenarios**:

1. **Given** a World simulation in Tectonics before an atmosphere has developed, **When** paused or running, **Then** the **Weather** button is disabled with a tooltip explaining that an atmosphere is required.
2. **Given** a World simulation with an atmosphere while actively running (`Play`), **When** viewing the controls, **Then** the **Weather** button is disabled until the user pauses the main simulation.
3. **Given** a paused World simulation with an atmosphere, **When** the user clicks **Weather**, **Then** the modal copies the current world via immutable `encode_snapshot()` bytes into an independent `WasmWeatherSession`, subdividing only the copy and displaying the current planet's terrain, climate, glaciation, star, atmosphere, and moons.
4. **Given** an open World Weather modal, **When** the user closes the modal, **Then** the weather timer stops, the modal's Three.js scene is disposed, the copied WASM weather session is freed, and the main World globe remains coarse and untouched.

---

### User Story 2 - Multi-Layer Meteorological Visualization & Vector Overlays (Priority: P2)

A user in either standalone Weather mode or the World Weather modal switches between six visualization modes (**Satellite**, **Pressure & Fronts**, **Surface Temp**, **Precip & Clouds**, **300 hPa Jet**, **Tides & Surge**) and four 3D vector arrow overlays (**Off**, **Surface Wind**, **Upper Jet**, **Tidal Current**) while stepping `+1h`, `+24h`, playing continuously, or resetting.

**Why this priority**: Makes atmospheric circulation, synoptic storms, and ocean/tidal dynamics directly legible on the 3D globe.

**Independent Test**: Open Weather, spin up 72 hours (`+24h` × 3), cycle through all 6 globe modes and 4 vector arrow overlays, and verify immediate visual updates without WebGL errors.

**Acceptance Scenarios**:

1. **Given** an active weather session, **When** the user steps `+1h`, `+24h`, or clicks **Play**, **Then** the moist baroclinic solver advances hourly, updating sea-level pressure, winds, clouds, precipitation, and tidal surge.
2. **Given** any visualization mode or vector arrow toggle, **When** selected, **Then** the globe colors and 3D vector arrows update immediately while preserving the current weather state.

---

### User Story 3 - Click-to-Inspect Tile Weather & 48-Hour Meteogram (Priority: P3)

Clicking any fine tile on the weather globe displays a detailed local weather report (elevation/ocean depth, temperature/SST, SLP, tidal height and storm surge, surface wind, 300 hPa jet speed, cloud cover, precipitation, atmosphere/moon metadata, and native flora) alongside a 48-hour meteogram plotting temperature, sea-level pressure, and hourly precipitation.

**Why this priority**: Provides quantitative, local meteorological observability at the fine-grid scale.

**Independent Test**: Step weather 48+ hours, click a tile on the weather globe, and verify that the 48-hour meteogram curves/bars and fine-tile weather report populate with finite physical values.

**Acceptance Scenarios**:

1. **Given** a weather session stepped 48+ hours, **When** the user clicks a tile on the globe, **Then** the 48-hour meteogram renders temperature, SLP, and precipitation history for that tile and the report displays its live local conditions.
2. **Given** a standalone Weather mode session, **When** the user loads Earth, a saved planet, or a `.bin` snapshot file, or adjusts environment parameters (day of year, atmosphere, glaciation, moons), **Then** the weather solver reinitializes cleanly for the new configuration.

---

### Edge Cases

- What happens on a moonless world (`Moons: 0`)? Lunar tides are zero and solar tides alone drive tidal elevation and currents.
- What happens on exotic atmospheres (e.g., dense 2.5 bar Hycean or thin CO2)? Surface pressure ranges, temperature, and moisture dynamics scale physically with the planet's atmosphere preset without numerical blowup.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST run a coupled moist baroclinic weather and storm-surge solver at a 1-hour timestep on a uniform `coarse + 2` spherical finite-volume grid driven by solar declination/diurnal cycle, lunar/solar tides, atmospheric circulation, and ocean SST anomalies, deriving surface snow/ice albedo from `ClimateModel`'s physical land glacier/temperature state rather than Köppen `EF`/`ET`.
- **FR-002**: System MUST gate the World simulation's **Weather** button so it is enabled ONLY when the main simulation is paused AND `has_atmosphere()` is true.
- **FR-003**: System MUST initialize the World Weather modal exclusively from an immutable `encode_snapshot()` copy of the current world, ensuring zero mutation or subdivision of the base `WorldSimulation`.
- **FR-004**: System MUST provide Play/Pause, `+1h`, `+24h`, and Reset controls, 6 globe visualization modes, 4 3D vector arrow overlays, independent globe rotation, and click-to-inspect tile reports with a 48-hour meteogram.
- **FR-005**: System MUST dispose WebGL resources, clear timers, and free the copied `WasmWeatherSession` memory when closing the World Weather modal.

### Key Entities

- **WeatherSandbox**: Self-contained weather simulation pairing a `FinePlanetBase` (`coarse + 2` grid, rivers, optics, sky) with a `MoistBaroclinicSolver`.
- **MoistBaroclinicSolver**: Finite-volume spherical atmosphere + tidal surge state (surface temperature, SLP, moisture, clouds, precipitation, surface wind, 300 hPa jet, tidal elevation, and 48-hour ring buffer).
- **WasmWeatherSession**: Memory-managed WASM wrapper exposing hourly stepping, visualization buffers, and tile inspection to the UI.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Opening the World Weather modal on a Medium (7,292 → 65,612 fine tiles) world initializes and renders within 3 seconds, with zero changes to the underlying World simulation upon closing.
- **SC-002**: Stepping `+24h` produces coherent synoptic pressure lows/highs, swirling cloud bands, and finite wind/precipitation/surge values across 72+ simulated hours without numerical divergence.
- **SC-003**: Re-opening and closing the World Weather modal repeatedly does not leak WASM memory or orphaned timers.

## Assumptions

- The World Weather modal uses the paused world's exact current environment (star, atmosphere, glaciation, moons) without exposing environment-override sliders that belong to standalone Weather sandbox mode.
- Fine-grid subdivision (`coarse + 2`) matches standalone Weather mode for identical physical behavior.
