# Feature Specification: Prehistory (Glacial Cycles, Soil & Human Migration)

**Feature Branch**: `003-prehistory`

**Created**: 2026-11-23

**Status**: Implemented (Baseline Spec)

**Input**: Baseline specification for the Prehistory simulation part in Tec11.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Glacial / Interglacial Cycles & Sea-Level Land Bridges (Priority: P1)

When a planet enters Prehistory, the timestep shifts to 4,000 years per step. Global glaciation oscillates between warm interglacial ("hothouse") and cold glacial ("snowball"/ice age) extremes, expanding and retreating ice sheets, dropping eustatic sea level to expose continental-shelf land bridges during glacial maxima, and submerging them during interglacials.

**Why this priority**: Ice-age cycles reshape coastlines, fertility, and migration corridors—creating the geographic bottlenecking and isolation that drive human diversity before civilization.

**Independent Test**: Advance a planet to Prehistory, step through at least one full glacial cycle, and observe ice-sheet expansion/retreat, sea-level coastline changes, and polar sea-ice caps updating in both Natural Color and Climate views.

**Acceptance Scenarios**:

1. **Given** a planet entering Prehistory, **When** the user steps forward, **Then** each step advances time by 4,000 years and updates global glaciation, ice-sheet extent, and eustatic sea level.
2. **Given** a glacial maximum, **When** continental shelves drop below sea level, **Then** shallow straits expose temporary land bridges allowing migration across previously separated landmasses; during interglacials, those shelves re-submerge.

---

### User Story 2 - Ice-Sheet Scouring & Periglacial Soil Fertility (Priority: P2)

Advancing ice sheets scour underlying bedrock (reducing soil fertility on heavily glaciated shields), while ice-margin meltwater and windblown dust deposit fertile periglacial loess and till soils along glacial margins, permanently influencing carrying capacity in Prehistory and History.

**Why this priority**: Demonstrates emergent physical causality—post-glacial breadbaskets and barren shields arise from ice-sheet history rather than random fertility noise.

**Independent Test**: Step through multiple glacial cycles and inspect tiles that were under deep ice sheets versus tiles along the glacial margin to verify scoured vs. loess-enriched soil fertility.

**Acceptance Scenarios**:

1. **Given** tiles covered by thick continental ice sheets, **When** the ice retreats, **Then** bedrock scouring reduces baseline soil fertility on those tiles.
2. **Given** tiles along the periglacial margin, **When** ice sheets advance and retreat, **Then** loess/till deposition enriches soil fertility and increases subsequent carrying capacity.

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
- **FR-002**: System MUST model eustatic sea-level drop during glacial maxima to expose and re-submerge continental-shelf land bridges.
- **FR-003**: System MUST model ice-sheet bedrock scouring and periglacial loess/till soil deposition, carrying resulting soil fertility forward into History.
- **FR-004**: System MUST simulate hunter-gatherer population growth, migration, limited seafaring bridging, isolation-by-distance heritage branching, and localized contact-zone admixture.
- **FR-005**: System MUST continue climate-driven erosion during Prehistory and preserve full Prehistory state in `PlanetSnapshot`.

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
