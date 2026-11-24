# Feature Specification: Prehistory (Glacial Cycles, Soil & Human Migration)

**Feature Branch**: `003-prehistory`

**Created**: 2026-11-23

**Status**: Implemented (Baseline Spec)

**Input**: Baseline specification for the Prehistory simulation part in Tec11.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Glacial / Interglacial Cycles, Physical Ice Sheets & Sea-Level Land Bridges (Priority: P1)

When a planet enters Prehistory, the timestep shifts to 4,000 years per step. Global glaciation oscillates between warm interglacial ("hothouse") and cold glacial ("snowball"/ice age) extremes, expanding and retreating continental and alpine ice sheets, dropping eustatic sea level to expose continental-shelf land bridges during glacial maxima, and submerging them during interglacials. Where glaciers and ice sheets exist MUST be determined physically from `ClimateModel` seasonal temperature and precipitation (snow accumulation vs. summer melt mass balance, plus adjacent cold-margin outflow) rather than from descriptive Köppen climate classifications (`EF`/`ET`).

**Why this priority**: Ice-age cycles reshape coastlines, fertility, and migration corridors—and determining ice sheets physically from climate fields upholds Constitution Principle I while keeping dry cold tundra (e.g., Pleistocene Siberian mammoth steppe) distinct from active scouring ice sheets.

**Independent Test**: Advance a planet to Prehistory, step through at least one full glacial cycle, and verify that active land glaciers emerge where snow accumulation survives summer melt (not on warm/dry tundra), while ice-sheet extent, eustatic sea-level land bridges, and polar sea-ice caps update coherently in Natural Color and Climate views.

**Acceptance Scenarios**:

1. **Given** a planet entering Prehistory, **When** the user steps forward, **Then** each step advances time by 4,000 years and updates global glaciation, physically derived ice-sheet extent (from seasonal temperature and snowfall mass balance, independent of Köppen codes), and eustatic sea level.
2. **Given** a cold dry tundra tile with warm-enough summers or negligible snowfall versus a snowy subpolar/alpine tile where winter snow survives summer melt, **When** determining glaciated land tiles, **Then** the snowy tile develops an active glacier/ice sheet while the dry warm-summer tundra remains ice-free (periglacial steppe).
3. **Given** a glacial maximum, **When** continental shelves drop below sea level, **Then** shallow straits expose temporary land bridges allowing migration across previously separated landmasses; during interglacials, those shelves re-submerge.

---

### User Story 2 - Physical Surficial Soil, Ice-Sheet Scouring & Periglacial Loess (Priority: P2)

Each tile maintains physical surficial soil state (`soil_depth_m` and `soil_fertility`) sitting atop its bedrock `TileGeology` strata, updated directly each tick rather than via elapsed-tick history counters. Advancing ice sheets physically strip `soil_depth_m` toward bare bedrock, ice-margin windblown dust and glacial outwash deposit mineral-rich loess/till (increasing `soil_depth_m` and enriching `soil_fertility`), and ice-free weathering slowly rebuilds soil toward the underlying exposed bedrock's mineral potential (`Rock::soil_fertility()`). This physical soil state persists in `PlanetSnapshot`, directly drives carrying capacity in Prehistory and History, and is displayed in the Geology Detail Modal for inspected tiles.

**Why this priority**: Demonstrates emergent physical causality (Constitution Principle I)—post-glacial breadbaskets and scoured shields arise from physical soil thickness and nutrient state rather than tick counters or random fertility noise.

**Independent Test**: Step through multiple glacial cycles and inspect tiles that were under deep ice sheets versus tiles along the glacial margin in the Geology Detail Modal to verify stripped `soil_depth_m` vs. deepened, loess-enriched `soil_depth_m`/`soil_fertility` and effective fertility, and verify those exact values survive `PlanetSnapshot` save/reload into History.

**Acceptance Scenarios**:

1. **Given** tiles covered by thick continental ice sheets, **When** Prehistory steps forward, **Then** ice-sheet scouring strips `soil_depth_m` toward bare bedrock, reducing effective carrying capacity until post-glacial weathering gradually rebuilds soil.
2. **Given** ice-free tiles along the periglacial margin, **When** ice sheets are active nearby, **Then** loess/till deposition increases `soil_depth_m` and enriches `soil_fertility`, raising carrying capacity in Prehistory and History.
3. **Given** ice-free tiles with an atmosphere and precipitation, **When** ticks elapse without glaciation, **Then** bedrock weathering slowly rebuilds `soil_depth_m` and relaxes `soil_fertility` toward the underlying exposed rock's `Rock::soil_fertility()`.
4. **Given** a selected land tile, **When** the user opens the Geology Detail Modal, **Then** the modal displays the tile's surficial soil thickness (`soil_depth_m`), surficial fertility, bedrock parent-material fertility, and effective fertility (alongside a descriptive surficial condition label such as bare scoured bedrock, weathered regolith, or deep periglacial loess/till).

---

### User Story 3 - Hunter-Gatherer Spread, Isolation-by-Distance & Agriculture Gate (Priority: P3)

Early hunter-gatherer humans emerge at an initial habitable origin and spread across land (and across narrow straits once basic seafaring develops). Wide-ranging populations diverge via isolation-by-distance regional branching and localized contact-zone admixture rather than whole-continent homogenization. Once the configured number of glaciation cycles (`prehistory_glaciation_cycles`) has elapsed and suitable post-glacial conditions arise, agriculture is invented and the simulation transitions into History.

**Why this priority**: Produces geographically coherent ancestral heritages and population distributions that seed History's languages, nations, and states.

**Independent Test**: Step through Prehistory in **Population** view and inspect tiles on different continents to verify multi-lineage heritage trees, regional divergence, and transition to History after the configured glacial cycles.

**Acceptance Scenarios**:

1. **Given** an initial hunter-gatherer population, **When** Prehistory steps forward, **Then** populations expand into habitable tiles, respecting deserts, mountains, oceans, and ice sheets.
2. **Given** separated continental or distant regional populations, **When** generations pass, **Then** heritages branch regionally with localized contact-zone admixture rather than instant global homogenization.
3. **Given** `prehistory_glaciation_cycles` is configured to $N$, **When** fewer than $N$ cycles have completed, **Then** agriculture remains gated until the configured cycles elapse (unless the user manually clicks **Next Stage**).

---

### Edge Cases

- What happens if all landmasses are separated by deep ocean wider than seafaring range? Populations remain on reachable landmasses without crashing or teleporting across abyssal oceans.
- What happens if an extreme glacial peak covers most habitable land? Refugia retain surviving populations without total extinction deadlock.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST simulate Prehistory at 4,000 years per step with oscillating glacial/interglacial cycles controlled by the planet's `prehistory_glaciation_cycles` parameter.
- **FR-002**: System MUST determine active land glaciers and continental ice sheets physically from `ClimateModel` seasonal temperature and precipitation (cold-season snow accumulation vs. warm-season melt mass balance, with optional outflow onto adjacent cold margins), NEVER from descriptive Köppen climate classifications (`EF`/`ET`). River headwater gating MUST also use physical glacier/temperature state rather than Köppen `EF`/`ET`.
- **FR-003**: System MUST model eustatic sea-level drop during glacial maxima to expose and re-submerge continental-shelf land bridges.
- **FR-004**: System MUST store physical surficial soil state (`soil_depth_m` and `soil_fertility`) on `TileGeology`, updating it each tick via ice-sheet stripping (driven by the physical ice-sheet mask from FR-002), periglacial loess/till deposition, and climate-driven bedrock weathering (replacing elapsed tick counters), persisting it in `PlanetSnapshot`, using it to drive carrying capacity in Prehistory and History, and displaying it in the Geology Detail Modal.
- **FR-005**: System MUST simulate hunter-gatherer population growth, migration, limited seafaring bridging, isolation-by-distance heritage branching, and localized contact-zone admixture (human habitability and passability may use Köppen classes as anthropocentric shorthand).
- **FR-006**: System MUST continue climate-driven erosion during Prehistory and preserve full Prehistory state in `PlanetSnapshot`.

### Key Entities

- **PrehistorySimulation**: Coarse-grid simulation state tracking glaciation index, eustatic sea level, soil fertility modifiers, population counts, and heritage lineages.
- **HeritageLineage**: Ancestral population group with origin, phylogenetic parent link, regional branching history, and per-tile admixture fractions.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Single Prehistory step executes interactively (< 100 ms on Medium 7,292-tile worlds in WASM).
- **SC-002**: Populations spread across connected habitable continents within 10–30 steps (40,000–120,000 years) and exhibit distinct regional heritage clusters.
- **SC-003**: Advancing to History cleanly hands off terrain, climate, soil fertility, populations, and heritages with zero data loss.

## Assumptions

- In randomized world generation, `prehistory_glaciation_cycles` is sampled from a Gaussian distribution centered around 3 cycles.
- Users may override the automatic agriculture gate at any time by clicking **Next Stage**.
