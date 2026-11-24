# Feature Specification: Tectonics (Plate Dynamics, Geology & Early Climate)

**Feature Branch**: `002-tectonics`

**Created**: 2026-11-23

**Status**: Implemented (Baseline Spec)

**Input**: Baseline specification for the Tectonics simulation part in Tec11.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Supercontinent Cycle & Plate Motion (Priority: P1)

A user creates a planet on a uniform hexagonal geodesic grid (with 12 pentagons) and steps through geological time at 5 million years per step. Tectonic plates drift, collide to build mountain ranges, subduct oceanic crust, and rift continents apart in a supercontinent cycle.

**Why this priority**: Plate tectonics generates the fundamental planetary topography, bathymetry, and crustal thickness distribution upon which all later climate, life, and history stages depend.

**Independent Test**: Create a planet in World mode, step 30+ times (150+ Myr), and verify that continents move, collide, form elevated orogenic belts, and rift dynamically without NaN elevations or frozen static plates.

**Acceptance Scenarios**:

1. **Given** a newly created planet of any supported grid size (Small, Medium, Large), **When** the user clicks **Step** or **Start**, **Then** each step advances planetary age by 5,000,000 years and updates plate positions, crust elevation, and mountainosity.
2. **Given** a large assembled continent, **When** tectonic steps continue, **Then** rifting probability increases with continental size and weight, eventually splitting the landmass into diverging plates.

---

### User Story 2 - Geological Composition, Stratigraphy & Erosion (Priority: P2)

As plates evolve and an atmosphere develops, physical processes (volcanism, metamorphism, sedimentation, chemical/physical erosion, and meteor impacts) continuously shape each tile's elevation and layered rock strata (including compositional properties like quartzity, sulfidity, and salinity for ore veins, BIFs, black smokers, and evaporites).

**Why this priority**: Enforces Core Principle I—rock types, mineral deposits, and mountain morphology emerge from physical history rather than stored age labels.

**Independent Test**: Click any land or ocean tile after 200+ Myr and open the Geology / tile inspector to verify realistic rock strata, mineral composition, and elevation consistent with tectonic and erosional history.

**Acceptance Scenarios**:

1. **Given** a planet in Tectonics, **When** the planetary age reaches the configured `Atm Δt`, **Then** an atmosphere develops, unlocking the global climate model, atmospheric erosion, and ocean circulation.
2. **Given** an atmosphere is active and age reaches `Life Δt`, **Then** a biosphere emerges, coloring vegetated land tiles according to local climate and stellar/atmospheric foliage optics.

---

### User Story 3 - Global Climate & Ocean Current Feedback (Priority: P3)

Once an atmosphere develops, the global climate model computes seasonal temperature, precipitation, atmospheric circulation cells, ocean currents, and polar sea-ice caps, driving erosion during Tectonics and rendering descriptive Köppen climate zones in the Climate view.

**Why this priority**: Ensures climate shapes landforms through erosion and provides immediate visual feedback of planetary habitability.

**Independent Test**: Switch the globe view to **Climate** after an atmosphere forms and verify that temperature bands, precipitation, ocean gyres, polar sea-ice caps, and Köppen classifications render coherently with latitude, spin rate, and continent placement.

**Acceptance Scenarios**:

1. **Given** a planet with an atmosphere in Tectonics, **When** the user switches to **Climate** view, **Then** descriptive Köppen zones, ocean current arrows, and polar sea-ice caps render on the globe.
2. **Given** a planet without an atmosphere yet, **When** viewing the globe, **Then** the planet renders as bare airless rock/crust and the **Weather** modal button remains disabled.

---

### Edge Cases

- What happens when `Land %` is very low (e.g., 5–10% ocean world) or very high (e.g., 70%+ desert world)? Plate tectonics, sea level, and climate remain numerically stable without division-by-zero or runaway elevations.
- What happens on very fast or very slow planetary spin rates? Atmospheric Hadley/Ferrel/Polar cell counts adapt dynamically to spin rate.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST simulate plate tectonics on a uniform hexagonal geodesic grid (plus 12 pentagons) at 5 Myr per step, supporting Small (2,432), Medium (7,292), and Large (21,872) tile counts.
- **FR-002**: System MUST model crustal thickness, elevation, mountainosity, volcanism, continental rifting (weighted by continental size/weight), subduction, and meteor impacts.
- **FR-003**: System MUST track per-tile stratigraphy and mineral/compositional properties emerging from geological processes rather than static age counters.
- **FR-004**: System MUST activate the global climate and erosion model when planetary age reaches `Atm Δt`, and biosphere/vegetation when age reaches `Life Δt`. `ClimateModel` MUST compute physical land glacier/ice-sheet presence from seasonal temperature and snowfall mass balance (independent of Köppen codes), and physical outputs (rivers, snow/ice rendering, sky vantage) MUST use physical climate/glacier fields rather than Köppen `EF`/`ET`.
- **FR-005**: System MUST support stepping, continuous play/pause, tile inspection, and lossless `PlanetSnapshot` export/import at any point in Tectonics.

### Key Entities

- **SphericalGrid**: Uniform hexagonal geodesic sphere tessellation with 12 pentagonal tiles and neighbor adjacency.
- **TectonicTile**: Crustal thickness, elevation, mountainosity, plate assignment, volcanic/orogenic activity, and stratigraphic rock layers.
- **ClimateModel**: Seasonal temperature, precipitation, atmospheric circulation cells, ocean currents, polar sea-ice extent, physical land glacier/ice-sheet mass balance, and descriptive Köppen classification.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Single Tectonics step completes interactively (< 150 ms on Medium 7,292-tile worlds in WASM).
- **SC-002**: Continents visibly assemble and break up over 200–500 Myr while preserving target land fraction within realistic tectonic variation.
- **SC-003**: All tile elevations, temperatures, and climate fields remain finite (zero NaNs) across extreme parameter combinations (spin, tilt, land %, moons, meteors).

## Assumptions

- Tile spacing on a Medium (6,400 km radius, 7,292 tiles) planet is ~285 km between adjacent tile centers.
- Transition to Prehistory occurs either automatically after `People Δt` post-life or manually when the user clicks **Next Stage**.
