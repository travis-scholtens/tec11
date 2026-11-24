# Feature Specification: History (Subdivided Terrain, Languages, States & Chronicles)

**Feature Branch**: `004-history`

**Created**: 2026-11-23

**Status**: Implemented (Baseline Spec)

**Input**: Baseline specification for the History simulation part in Tec11.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Coarse-to-Fine Land Subdivision & State Formation (Priority: P1)

Upon entering History, land tiles are subdivided onto a finer spherical grid (~95 km spacing on a Medium world, with sub-grid relief scaled by coarse orogenic activity and crustal thickness) and the timestep shifts to 200 years per step. Agricultural populations coalesce into cities, nations, and states with visible borders, capitals, and overland/maritime trade routes.

**Why this priority**: Subdivided terrain gives historical polities realistic coastal inlets, mountain barriers, river valleys, and regional borders.

**Independent Test**: Advance a world to History, step 20–40 times in **Borders** view, and verify that subdivided terrain, multiple states, capitals, cities, and trade routes emerge cleanly.

**Acceptance Scenarios**:

1. **Given** a planet transitioning from Prehistory to History, **When** initialization completes, **Then** land tiles are subdivided onto the finer terrain grid with orogeny-scaled relief and rivers routed across the fine grid.
2. **Given** History steps at 200 years per step, **When** populations grow in fertile regions, **Then** states form, expand across habitable corridors, establish capitals and cities, and open trade links.

---

### User Story 2 - Emergent Political Dynamism (Secular Cycles & Separatism) (Priority: P2)

Empires do not remain frozen for millennia: demographic-structural strain (Turchin–Nefedov secular cycles), elite overproduction, fiscal stress, and multi-ethnic or distant-province separatism periodically trigger internal crises, rebellions, conquests, and fragmentation without relying on arbitrary random catastrophe scripts.

**Why this priority**: Addresses historical stagnation by making state rise-and-fall emerge from population pressure, distance, and ethnic/cultural diversity.

**Independent Test**: Run a History simulation for 50+ steps (10,000+ years) and inspect state chronicles to verify recurring cycles of expansion, consolidation, crisis, separatism/dissolution, and successor state formation.

**Acceptance Scenarios**:

1. **Given** a large, densely populated state approaching carrying capacity, **When** secular-cycle strain accumulates, **Then** fiscal/elite stress increases instability and fragmentation risk.
2. **Given** an overextended multi-ethnic empire with distant provinces, **When** administrative strain rises, **Then** distant or distinct-heritage provinces exhibit elevated separatism and can break away into independent states.

---

### User Story 3 - Evolving Languages, Sound Changes, Audio & Historical Chronicles (Priority: P3)

Populations develop evolving languages with phonological inventories, diachronic sound changes (including palatalization, regressive vowel nasalization, sprachbund areal diffusion, tonal contagion, marked feature convergence, and prestige borrowing doublets), adaptive time-unit/lunar-month vocabulary, linguistically named states and cities, Klatt-88 IPA speech synthesis playback, and clickable State Details chronicles timestamped in local orbital years.

**Why this priority**: Brings civilizations to life with deeply grounded linguistic, cultural, and historical detail inspectable directly on the globe.

**Independent Test**: Click a state on the globe in History, open **State Details** and **Language Details**, verify the historical chronicle with clickable state links and local-year timestamps, and play synthesized IPA pronunciation audio.

**Acceptance Scenarios**:

1. **Given** a state in History, **When** the user opens **State Details**, **Then** a chronological chronicle displays founding, trade opened/ended, wars, conquests, and dissolutions in rounded local orbital years with clickable links to referenced states (including conquered/former states retaining their linguistic names).
2. **Given** a language in **Language Details**, **When** the user clicks a word's audio playback button, **Then** the Klatt-88 IPA synthesizer renders and plays Web Audio speech in the browser.
3. **Given** a moonless world, **When** languages generate time-unit vocabulary, **Then** month concepts and lunar calendar months are omitted cleanly.

---

### Edge Cases

- What happens when a state is conquered and later referenced in another state's chronicle? Its linguistic name is preserved via its `founding_language_id` fallback so chronicle entries remain readable and clickable.
- What happens when a state's trade partners change during a single tick? Previous trade partners are snapshotted before mutation so both `TradeOpened` and `TradeEnded` events are recorded accurately.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST subdivide land tiles upon entering History using orogeny-scaled procedural relief and step History at 200 years per step. Natural-color snow/ice groundcover and sky vantage MUST derive from `ClimateModel`'s physical glacier/temperature state rather than Köppen `EF`/`ET` (while human settlement/passability may use Köppen as anthropocentric shorthand).
- **FR-002**: System MUST simulate state formation, cities, capitals, diplomacy, trade routes, warfare, Turchin–Nefedov secular cycles, and multi-ethnic/distant-province separatism.
- **FR-003**: System MUST simulate diachronic language evolution (sound changes, areal sprachbund diffusion, prestige borrowing doublets, adaptive calendar/time vocabulary) and generate linguistic names for states, capitals, and cities.
- **FR-004**: System MUST record per-state and per-city historical chronicles timestamped in rounded local orbital years with clickable state cross-links.
- **FR-005**: System MUST synthesize playable IPA speech audio via the `tec11-speech` Klatt-88 synthesizer in the browser.
- **FR-006**: System MUST support full `PlanetSnapshot` serialization and deserialization of History state.

### Key Entities

- **HistorySimulation**: Subdivided terrain grid, fine-tile populations, cities, states, diplomatic/trade networks, languages, and chronicles.
- **State**: Polity with territory, capital, cities, founding language, heritage composition, secular-cycle strain metrics, trade partners, and chronicle events.
- **Language**: Phoneme inventory, sound-change history, lexicon (including synonyms/doublets and calendar terms), and areal contact relationships.
- **ChronicleEvent**: Timestamped historical event (`Founded`, `TradeOpened`, `TradeEnded`, `Conquered`, `Dissolved`, etc.) linked to state IDs.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: History subdivision completes without browser freeze (< 3 seconds on Medium worlds) and individual 200-year steps execute interactively.
- **SC-002**: Multi-millennia runs produce ongoing political turnover (rise, expansion, crisis, fragmentation, and successor states) rather than permanent static equilibrium.
- **SC-003**: Every state and city name in tiles and chronicles resolves to a valid linguistic name (never raw numeric IDs or empty strings), including conquered/former states.

## Assumptions

- Fine-grid land tile spacing is approximately 95 km on a Medium (6,400 km radius) planet.
- Web Audio API is available in the user's browser for optional IPA speech synthesis playback.
