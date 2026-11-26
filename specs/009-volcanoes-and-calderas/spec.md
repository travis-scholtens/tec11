# Feature Specification: Volcanoes, Calderas, and 3D Magmatic Plumbing

**Feature Branch**: `009-volcanoes-and-calderas`

**Created**: 2026-11-25

**Status**: Draft

**Input**: User description: "Volcanos — we have magma intrusions, hotspots, and subduction zones; synthesize physically grounded volcanic cones, calderas, crater lakes, parasitic cinder cones, eroded volcanic necks, and 3D subsurface conduits/lava tubes at the appropriate Local 3D Terrain scales (Levels 0–3)."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Explore Scale-Appropriate Volcanic Edifices & Calderas (Priority: P1)

When a user opens the Local 3D Terrain modal on a tectonically volcanic tile (a subduction-arc margin, an oceanic hotspot island, a continental rift/rhyolite province, or a tile with a shallow magmatic intrusion), the terrain deterministically synthesizes the physically appropriate volcanic landform across Levels 0–2:
- **Stratovolcanoes & collapse calderas** (`10–25 km` composite cones with steep concave slopes, radial erosion gullies, and `1–10 km` summit craters or collapse calderas) along active subduction arcs with intermediate volcanic rock.
- **Broad basaltic shield volcanoes** (`40–100 km` low-angle domes with `3–6 km` nested summit pit calderas) over oceanic hotspots and mafic extrusive provinces.
- **Resurgent / supervolcano calderas** (`20–55 km` sunken collapse basins with a central resurgent dome and geothermal rim) in felsic rhyolitic provinces with active subduction or magmatic intrusion.
- **Eroded volcanic necks & radial dikes** where intrusive plutonic rock is exposed on older, non-subducting terrain after the softer outer cone has weathered away.

**Why this priority**: Volcanic landforms are the core user-visible outcome; tying landform geometry directly to rock composition (`felsity`), `subduction`, `intrusion`, and `mountainosity` fulfills Constitution Principle I (Emergent Physical Processes) and makes different planetary regions immediately recognizable in 3D.

**Independent Test**: Open Local 3D Terrain on (a) a subduction-arc andesitic tile, (b) an oceanic basaltic hotspot island, (c) a felsic rhyolitic tile, and (d) an eroded intrusive tile from any existing `.tec11` save. Verify each produces its distinct edifice profile, slope angle, and summit depression across Levels 0–2 without mutating the parent world.

**Acceptance Scenarios**:

1. **Given** a tile with active subduction and extrusive intermediate rock (`andesite`), **When** the user views Levels 0–2 in Local 3D Terrain, **Then** a steep concave stratovolcano cone (`10–25 km` basal diameter) with radial gullies and a summit crater or collapse caldera is rendered consistently across all zoom levels.
2. **Given** an emergent oceanic hotspot tile or thick extrusive mafic (`basalt`) tile, **When** the user views Levels 0–2, **Then** a broad, gentle-sloped shield volcano with a nested summit pit caldera is rendered.
3. **Given** a tile with extrusive felsic (`rhyolite`) surface rock and active subduction or a shallow intrusion, **When** the user views Levels 0–2, **Then** a wide collapse caldera with steep inner walls and a central resurgent dome is rendered.
4. **Given** an older non-subducting tile whose surface exposes intrusive rock (`granite`, `diorite`, or `gabbro`) with an emplaced intrusion or high toughness contrast, **When** the user views Levels 1–2, **Then** a steep resistant volcanic neck/plug with radiating dikes is rendered instead of a fresh cone.
5. **Given** a sedimentary craton tile with no subduction, no intrusion, and non-volcanic surface rock, **When** the user views Levels 0–3, **Then** zero volcanic edifices or calderas are synthesized.

---

### User Story 2 - Crater Lakes, Flank Cinder Cones, & Hydrology Composition (Priority: P2)

Volcanic edifices interact realistically with climate, surface hydrology, and rivers:
- Collapse calderas and deep summit craters naturally form **crater lakes** whose physical state (freshwater lake, frozen ice-capped lake, dry playa, or evaporitic salt flat) emerges from the tile's precipitation, temperature, evaporation, and rock permeability—never hard-coded to always be wet.
- On the flanks of shield volcanoes and stratovolcanoes, smaller **parasitic cinder / scoria cones** (`400–1,000 m` diameter) appear cleanly at Level 2 (`~32 m/hex`) and Level 3 (`1 m/hex`).
- Regional rivers compose cleanly with volcanic cones and calderas: rivers never climb uphill over a volcanic flank, never float as aqueducts, and incise clean valleys or deflect around edifice slopes without creating cliff dams or disconnected pools.

**Why this priority**: Crater lakes (like Crater Lake or Santorini) and flank cinder cones bring Level 1 and Level 2 views to life, while strict hydrology composition preserves all monotonicity and continuity guarantees established in Spec 008.

**Independent Test**: Inspect a humid-temperate caldera tile, a polar/alpine caldera tile, and an arid desert caldera tile; confirm the crater floor holds freshwater, frozen ice, or dry/salt crust respectively. Verify a river passing near a volcanic cone remains strictly non-increasing downstream with zero floating aqueducts.

**Acceptance Scenarios**:

1. **Given** a collapse caldera in a humid temperate climate with low-permeability volcanic bedrock, **When** viewed at Levels 1–3, **Then** the caldera floor holds a recessed freshwater crater lake (optionally surrounding a central resurgent island cone).
2. **Given** a collapse caldera in an arid or sub-freezing climate, **When** viewed at Levels 1–3, **Then** the caldera floor renders as a dry playa/salt crust or a frozen ice-capped crater lake respectively.
3. **Given** a shield or stratovolcano flank at Level 2 (`~32 m/hex`), **When** exploring around the main cone, **Then** deterministic parasitic cinder cones (`400–1,000 m` wide) with small summit craters appear on the flanks.
4. **Given** a tile containing both a routed river and a volcanic edifice, **When** evaluated from Level 0 to Level 3, **Then** the river's water surface remains strictly non-increasing downstream and every riverbank stays at or above local river water elevation.

---

### User Story 3 - Level 3 Subsurface Magmatic Plumbing, Sulfur Crusts, & Lava Tubes (Priority: P3)

When the user drills down to Level 3 (`1 m/hex` 3D cutaway & Z-level slicer) on a volcanic tile or clicks a `Volcano` / `Caldera` Point of Interest:
- Directly beneath volcanic cones, necks, and resurgent domes, a **vertical 3D feeder conduit** and **radial volcanic dikes** of solidified igneous rock cut cleanly upward through older horizontal or folded sedimentary/metamorphic strata.
- Active/young crater rims and caldera floors exhibit **fumarolic sulfur deposits** and shallow epithermal mineral enrichment near the conduit contact.
- Fresh basaltic shield flanks contain hollow **3D lava tubes** (tubular underground caves in basalt flows) visible when scrubbing the Z-slice slider downward.
- Click-to-inspect reports the exact volcanic rock, sulfur deposit, or lava-tube cavity at any `(q, r, z)` prism.

**Why this priority**: Level 3's Z-slicer is where underground geology becomes tangible; exposing real vertical feeder pipes, dikes, sulfur crusts, and lava tubes makes volcanic tiles genuinely distinct underground, not just on the surface.

**Independent Test**: Jump to a `Volcano` or `Caldera` POI, enter Level 3, scrub the Z-slice downward by 30–60 m, and verify that (a) a coherent igneous feeder conduit/dike cross-cuts surrounding strata, (b) sulfur deposits appear on active crater rims/floors, and (c) basaltic shield tiles exhibit underground tubular lava-tube voids.

**Acceptance Scenarios**:

1. **Given** a volcanic cone or eroded neck viewed at Level 3, **When** the user scrubs the Z-slice downward through the summit/center, **Then** a vertical igneous conduit and radiating dikes cut cleanly across surrounding host strata.
2. **Given** an active subduction or hotspot caldera/crater rim at Level 3, **When** the user inspects the crater rim or geothermal floor, **Then** surficial fumarolic sulfur deposits appear and are reported by click-to-inspect.
3. **Given** a basaltic shield volcano flank at Level 3, **When** the user scrubs 5–25 m below the surface, **Then** sinuous hollow lava tubes appear inside the basalt flow layer.

---

### User Story 4 - Volcanic Points of Interest (`Volcano` / `Caldera` / `Volcanic Neck`) (Priority: P4)

When a tile contains one or more synthesized volcanic landforms, the Local 3D Terrain `Jump to:` POI bar automatically lists the most prominent volcanic feature (`Stratovolcano`, `Shield Volcano`, `Caldera`, `Crater Lake`, or `Volcanic Neck`) so users can jump straight to the summit rim or caldera wall in one click.

**Why this priority**: Coarse tiles span `~200–350 km`; without a direct `Jump to:` POI, users would have to hunt manually across hundreds of kilometers to find a `15 km` stratovolcano or `6 km` caldera.

**Independent Test**: Open Local 3D Terrain on a volcanic tile, verify a volcanic POI chip appears in the `Jump to:` bar, click it, and confirm the view centers directly on the volcano rim/caldera at Level 1 or Level 2 with clear visibility of the cone/crater.

**Acceptance Scenarios**:

1. **Given** a tile with a synthesized stratovolcano, shield volcano, caldera, or volcanic neck, **When** the Local 3D Terrain modal opens, **Then** a descriptive volcanic POI chip appears in the `Jump to:` bar.
2. **Given** the user clicks the volcanic POI chip, **When** the view updates, **Then** the camera centers near the rim/summit so both the edifice slope and crater/caldera interior are immediately visible, and clicking `Jump to 1 m 3D` opens directly on the crater rim or caldera edge.

---

### Edge Cases

- **Airless / Hadean Tectonics tiles (`has_atmosphere == false`)**: Volcanoes, shield domes, calderas, lava tubes, and conduits MUST still synthesize normally (like lunar maria/domes or Mars/Io volcanoes), with zero crater lakes, zero water, zero soil, and zero fluvial erosion gullies.
- **Submarine volcanism (seamounts & guyots)**: On submarine tiles (`elevation < sea_level`) with active hotspot/subduction volcanism, submerged volcanic cones (seamounts) remain beneath sea level unless their synthesized summit genuinely breaches sea level as a volcanic island—never creating artificial dry levees or flat ocean-top artifacts.
- **Coarse-tile re-anchoring during lateral panning**: Volcanic edifices MUST be seeded deterministically from global tile IDs and world seed (projected via the 2-ring neighborhood) so panning across coarse-tile boundaries never causes a volcano or caldera to shift, pop in/out, or split along a tile seam.
- **Pentagon tiles**: All volcanic positioning, radial distance calculations, and POI discovery MUST work identically on the 12 pentagonal coarse tiles without index or bounds errors.
- **Steep caldera/cone height in Level 3**: Level 3's vertical Z-window auto-fit MUST encompass steep crater-rim walls inside the `56×56 m` embark window without decapitating the rim.
- **Backwards compatibility with existing `.tec11` saves**: All volcanic synthesis MUST work on previously saved `.tec11` snapshots (Tectonics, Prehistory, and History) with zero changes to the `.tec11` protobuf schema or existing simulation stages.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST deterministically derive a set of local volcanic edifices from persisted planetary tile geology (`subduction`, `intrusion`, top rock kind & `felsity`, `mountainosity`, `elevation`, and `world_seed`) across the 2-ring tile neighborhood so edifices are seamless across coarse-tile re-anchoring.
- **FR-002**: System MUST classify each volcanic edifice physically into one of four primary morphologies driven by magma composition (`felsity`) and tectonic/erosional state:
  - **Stratovolcano** (intermediate extrusive `andesite`, active subduction or elevated volcanic arc): `10–25 km` basal diameter, concave steep flanks, radial erosion gullies (when atmosphere is present), and a summit crater or collapse caldera.
  - **Shield Volcano** (mafic extrusive `basalt`, hotspot/oceanic island or thick mafic crust): `40–100 km` broad low-angle dome with a `3–6 km` nested summit pit caldera.
  - **Resurgent Caldera** (felsic extrusive `rhyolite` with subduction or intrusion): `20–55 km` steep-walled collapse basin with a central resurgent dome.
  - **Eroded Volcanic Neck** (exposed intrusive pluton or residual intrusion on older non-subducting terrain): `1–4 km` steep resistant central plug with radiating dike ridges.
- **FR-003**: System MUST synthesize secondary flank **parasitic cinder / scoria cones** (`400–1,000 m` diameter, `80–260 m` tall, with small summit craters) deterministically on shield and stratovolcano flanks, visible cleanly at Levels 2 and 3.
- **FR-004**: System MUST evaluate collapse calderas and deep summit craters through the existing climate- and permeability-gated lake classification so humid impermeable calderas hold recessed freshwater crater lakes, cold calderas hold frozen crater lakes, and arid/permeable calderas form dry playas or salt crusts (with zero liquid water on airless worlds).
- **FR-005**: System MUST compose volcanic relief with the existing Hydrology-First Vector Drainage Network so routed rivers remain strictly non-increasing downstream and riverbanks never dip below local river water elevation (zero floating aqueducts or uphill waterfalls).
- **FR-006**: System MUST synthesize 3D subsurface volcanic structures at Level 3 (`1 m/hex`), including:
  - Vertical igneous feeder conduits and radiating dikes cross-cutting surrounding bedrock strata beneath cones, calderas, and necks.
  - Surficial fumarolic sulfur deposits on young/active crater rims and resurgent caldera floors.
  - Sinuous hollow 3D lava tubes inside fresh basaltic shield flow layers.
- **FR-007**: System MUST surface prominent volcanic landforms (`Stratovolcano`, `Shield Volcano`, `Caldera`, `Crater Lake`, `Volcanic Neck`) in the Local 3D Terrain `Jump to:` POI list, positioned so jumping to the POI frames both the rim/slope and crater/caldera interior.
- **FR-008**: System MUST operate strictly on immutable snapshot data without mutating the primary simulation or changing the `.tec11` snapshot format, and MUST preserve interactive WASM frame times across Levels 0–3.

### Key Entities

- **VolcanicEdifice**: A deterministic, anchor-invariant volcanic landform positioned in local tangent-plane coordinates with morphology kind (`Stratovolcano`, `ShieldVolcano`, `ResurgentCaldera`, `VolcanicNeck`), magma composition (`felsity` / igneous material), basal radius, peak height, summit crater/caldera radius and depth, resurgent dome parameters, optional crater-lake state, and activity/erosion age factor.
- **ParasiticCone**: A smaller deterministic flank cinder/scoria cone associated with a parent shield or stratovolcano, with center position, basal radius (`200–500 m`), height, and summit crater radius.
- **VolcanicQueryResult**: The continuous spatial evaluation of nearby volcanic edifices and parasitic cones at `(x_m, y_m)`, returning additive edifice relief, caldera/crater depression, optional crater-lake water level and state, conduit/dike proximity (for 3D Level 3 cross-cutting), sulfur deposit intensity, and lava-tube eligibility.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Opening Local 3D Terrain on 100% of previously saved `.tec11` worlds (Tectonics, Prehistory, History) succeeds with zero schema migrations or regressions in existing tests.
- **SC-002**: Volcanic tiles with distinct geologies (mafic basalt hotspot, intermediate andesite subduction arc, felsic rhyolite province, and eroded intrusive plug) produce measurably distinct slope profiles and morphologies (shield slope < 8°, stratovolcano flank slope 12–30°, caldera floor strictly below rim height, neck aspect ratio steeper than surrounding terrain).
- **SC-003**: Across all volcanic tiles with routed rivers, 100% of river reaches preserve strict downstream monotonicity (`z_water_1 <= z_water_0`) and zero riverbanks dip below local `z_water`.
- **SC-004**: Panning across a coarse-tile boundary adjacent to a volcanic edifice produces identical surface elevation (`< 0.01 m` difference) at the same geographic `(lat, lon)` before and after re-anchoring.
- **SC-005**: Level 0–2 patch generation and Level 3 `56×56×72` volume meshing on volcanic tiles remain interactive (`< 100 ms` per update in browser WASM).

## Assumptions

- **Scope boundary for v1**: All volcanic cones, calderas, crater lakes, cinder cones, and 3D plumbing live in Local 3D Terrain (`Levels 0–3`) and its `Jump to:` POI bar. Adding volcanic markers or eruption mechanics to the coarse globe, Regional View, or History simulation is out of scope for v1.
- **No simulation/save-format changes**: Existing persisted fields on `TileGeology` (`subduction`, `intrusion`, `layers`, `mountainosity`, `bottom`, `elevation`) provide sufficient physical signal without altering `TectonicsSimulation`, `PrehistorySimulation`, `HistorySimulation`, or `.tec11` protobuf definitions.
- **Airless vs. atmospheric rendering**: Airless Tectonics worlds still form volcanic shields, cones, calderas, conduits, and lava tubes, but suppress crater lakes, water, soil, and fluvial barranco gullies.
