# Feature Specification: Local 3D Hex-Prism Terrain & Underground Zoom

**Feature Branch**: `008-local-3d-terrain`

**Created**: 2026-11-24

**Status**: Draft

**Input**: User description: "Explore zooming in from a fine/history tile down to ~0.5-1 m hexagonal prisms with varying elevation and a 3D underground (Minecraft / Dwarf Fortress style), using lazy hierarchical subdivision into a local tangent-plane chunked volume, viewable via both 3D cutaway/orbit and z-level slicing, exposed both as an isolated modal from a paused World tile and as a reusable Rust crate API consuming `.tec11` snapshots."

## Clarifications

### Session 2026-11-24

- Q: Which World simulation stages should support opening the Local 3D Terrain modal (when paused)? → A: All three World stages (Tectonics, Prehistory, and History) when paused.
- Q: How should the user navigate and choose which ~128 m patch inside a ~95–285 km planet tile to inspect inside the Local 3D Terrain modal? → A: Progressive multi-step drill-down through the hierarchical subdivision levels (clicking any location at the current scale to zoom in one step—showing all terrain, river, and geological detail available at each intermediate scale—and stepping back up as desired until reaching the ~1 m 3D hex-prism volume).
- Q: What level of material detail should individual underground `HexPrismCell` voxels store? → A: Full Tec11 rock types + surficial materials (soil, loess/till, sand, peat, water, ice, air) + discrete ore/mineral vein voxels derived from `TileGeology` occurrences.
- Q: How should the sparse chunk delta overlay (for player digging/building edits in downstream games) be serialized? → A: Self-contained versioned binary delta format inside `tec11-local-terrain` (separate from `.tec11` planetary snapshots).
- Q: How should the intermediate progressive drill-down steps (between the ~95–285 km parent tile and the ~1 m leaf volume) be rendered inside the Local 3D Terrain modal? → A: Same 3D WebGL hex-column renderer at every zoom level (click any hex to drill down; leaf ~1 m scale unlocks 3D underground cutaway & Z-level slicer).
- Q: How should underground geological strata vary within a tile and across tile boundaries? → A: Laterally continuous stratigraphic blending across the parent tile and its neighbors—correlating shared beds to smoothly interpolate thickness and depth, tapering non-shared beds into realistic geological pinch-outs/lenses/interfingering wedges, and applying continuous regional dip/folding—so different locations inside a single tile look genuinely distinct underground and crossing from tile to tile has zero abrupt vertical stratigraphic seams.
- Q: How should users discover and jump to notable physical features inside a large tile? → A: Automatic in-tile Points of Interest discovery (`LocalPointOfInterest`) scanning the tile's synthesized relief, rivers, coasts, karst, mineral outcrops, and impact craters to offer one-click jump pills in the modal (alongside lateral N/S/E/W + WASD panning at all zoom levels and a Depth Window shift at Level 3).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Lazy Hierarchical Zoom from Planet Tile to ~1 m Local Hex-Prism Volume (Priority: P1)

A user (or downstream game via the Rust crate API) selects a location on any `.tec11` planet (or a paused World tile) and zooms down from the coarse/History tile (~95 km spacing) through a lazy local-patch subdivision cascade onto a local tangent-plane **hexagonal-prism 3D grid** (~0.5–1.0 m horizontal spacing, discrete vertical z-levels). Only the local neighborhood along the zoom path and the requested local embark volume (e.g., 64×64 to 256×256 hex columns × 128–256 z-levels) are materialized, keeping memory within a few megabytes while preserving deterministic continuity with the parent world.

**Why this priority**: Solves the exponential memory barrier (10–30+ billion tiles per 95 km fine tile) and `f32` spherical precision loss, establishing the core data model and reusable Rust library foundation.

**Independent Test**: Load any `.tec11` snapshot in Rust unit tests, request a 128×128×128 local hex-prism volume at two different tile coordinates (e.g., a mountain belt vs. a river valley), and verify deterministic regeneration, bounded memory usage (< 16 MB), smooth boundary elevation continuity with neighboring coarse/fine tiles, and zero mutation to the source snapshot.

**Acceptance Scenarios**:

1. **Given** a valid `.tec11` snapshot and a target fine tile (plus optional sub-tile offset), **When** generating a local 3D hex-prism volume, **Then** the zoom cascade subdivides only the focused local patch at each intermediate level and transitions to local tangent-plane axial hex coordinates below ~1–2 km without floating-point jitter.
2. **Given** the same `(world_seed, planet_snapshot, target_location, chunk_coord)`, **When** generated repeatedly across sessions or platforms, **Then** every hex column's surface elevation and underground z-level material stack are bit-for-bit identical.
3. **Given** a 128×128×256 local volume (4.19M hex-prisms), **When** chunked and compressed in memory, **Then** total RAM usage stays comfortably within browser/WASM budgets (< 16 MB for chunks + surface mesh).

---

### User Story 2 - Emergent Geomorphology, Underground Stratigraphy, Rivers & Caves (Priority: P2)

The generated 3D volume is not generic mountainosity-scaled noise: every vertical hex column emerges from six composable physical operators driven directly by parent and neighbor planet tile data (`TileGeology.layers`, `RockKind`, `Rock::toughness`, `felsity`, `calcity`, `clasticity`, `bogginess`, `salinity`, `quartzity`, `sulfidity`, `mountainosity`, `subduction`, `intrusion`, `impact_scar`, surficial `soil_depth_m`/`soil_fertility` and bedrock scouring, Climate temperature/precipitation/winds, Tides, and routed Rivers):
1. **Laterally continuous stratigraphic blending & structural warping** (correlating shared rock layers across the parent tile and its 5–6 neighbors to smoothly interpolate thickness and horizon depth, tapering non-shared beds into natural geological pinch-outs/lenses/interfingering wedges, and applying continuous regional dip, folding, faulting, intrusive contacts, and `impact_scar` crater bowls/breccia so different spots inside one tile look genuinely distinct underground and crossing tile boundaries has zero abrupt vertical seams).
2. **Differential erosion by `Rock::toughness`** (naturally producing dissected plateaus/mesas like the Catskills, ridge-and-valley hogbacks like the Appalachians, sea cliffs, and waterfalls without storing a "mountain age").
3. **Glacial vs. fluvial carving** (Alps-style horns/arêtes/cirques/U-valleys/fjords where bedrock glacial scouring or alpine freezing is high vs. Himalayas-style V-gorges/floodplains/alluvial fans from hierarchical river splines).
4. **Soluble-rock dissolution** (karst sinkholes, limestone pavements/towers, and 3D underground caverns in high-`calcity` or evaporite layers).
5. **Surficial mantle & depositional overlays** (physical topsoil/loess/till depth from `soil_depth_m`/`soil_fertility`, windblown dunes & playas in arid regions, peat bogs from `bogginess`, volcanic basalt benches/domes, talus/scree).
6. **Coastal wave & tide beveling** (cliffs/wave-cut platforms/stacks vs. beaches/barrier islands/tidal flats/reefs around sea level).

**Why this priority**: Enforces Constitution Principle I & II—bringing Tec11's planetary geology, climate, and glacial history directly into recognizable, playable 3D landforms and underground detail.

**Independent Test**: Generate local volumes from contrasting parent tiles (e.g., a flat sedimentary plateau, a glaciated alpine peak, an active Himalayan-style metamorphic orogen, a humid limestone river valley, an arid evaporite basin, and a coastal tile with strong tides) and verify distinct, physically consistent surface landforms, stratigraphy, soil depth, ore/mineral presence, and water/cave features.

**Acceptance Scenarios**:

1. **Given** a parent tile with low `mountainosity` and flat-lying `Sedimentary` layers of alternating `toughness`, **When** generating the 3D volume, **Then** differential erosion produces stepped cliff-and-bench terraces and flat-topped ridges (dissected plateau / Catskills style) with gently undulating horizontal underground beds.
2. **Given** a parent tile with high `mountainosity`, thick metamorphic/intrusive crust, and elevated `sulfidity`/`quartzity`, **When** generating the 3D volume, **Then** strata exhibit strong folding/tilting with hydrothermal/quartz veins, sharp ridged peaks, and—when glacial scouring or alpine freezing is high—glacial U-shaped troughs, cirques, and arêtes (Alps style) rather than purely fluvial V-gorges (Himalayas style).
3. **Given** a parent tile with a routed fine river, **When** generating the 3D volume, **Then** the river centerline is hierarchically subdivided (meandering more in flat lowlands and running straighter in steep mountains) with a monotonically non-increasing water-surface elevation $Z_{\text{water}}$, carving a riverbed below $Z_{\text{water}}$ filled with water and alluvial sediment, blending a valley/floodplain envelope into surrounding terrain so the river never flows uphill, and exposing cut rock strata on canyon walls.
4. **Given** a parent tile with high `calcity` (`limestone`/`chalk`/`marble`) in a humid climate, **When** generating the 3D volume, **Then** karst features (sinkholes, fractured limestone surfaces, and interconnected 3D underground caverns/aquifers) emerge inside the soluble strata.
5. **Given** a parent tile with thin stripped `soil_depth_m` versus thick enriched `soil_depth_m`/`soil_fertility`, **When** generating surface mantles, **Then** scoured shield tiles exhibit thin soil over exposed bedrock (or fjords on steep coasts), whereas loess/till-enriched tiles exhibit deep fertile topsoil/silt horizons and moraine/till relief.
6. **Given** an arid, volcanic, coastal, or impact-scarred parent tile, **When** generating the 3D volume, **Then** the corresponding physical landforms emerge (wind-aligned dunes/salt flats for arid/high-`salinity`, basalt benches/volcanic domes for extrusives, cliffs/beaches/tidal flats around sea level, and raised crater rims/breccia for `impact_scar > 0`).

---

### User Story 3 - Progressive Drill-Down, 3D Cutaway / Orbit View & Z-Level Slicing in World Modal (Priority: P3)

While the main World simulation is paused (in Tectonics, Prehistory, or History), a user selects a tile and opens an isolated **Local 3D Terrain** modal. Inside the modal, the user can progressively drill down through the hierarchical subdivision levels—clicking any location on the current zoom level to step one level deeper (showing whatever relief, rivers, coasts, and geology are materialized at that scale, with a control to step back up)—until reaching the ~1 m hex-prism chunk, where they can explore using both:
- **3D Orbit / Cutaway Camera**: Rotating, tilting, and zooming around the 3D hex-prism block with visible exposed stratigraphic cross-sections on the chunk edges.
- **Dwarf Fortress–Style Z-Level Slicer**: Scrubbing a vertical `Z` slider (or stepping up/down 1 m per level) to slice away upper layers and inspect underground caves, aquifers, ore veins, and rock beds layer by layer, clicking any hex prism to inspect its rock/mineral/soil type and depth.

Closing the modal disposes all WebGL and WASM resources and leaves the base World simulation completely untouched.

**Why this priority**: Makes the 3D local terrain immediately inspectable and verifiable inside Tec11 while serving as a live reference frontend for the reusable Rust crate.

**Independent Test**: Pause a World simulation, select a land tile, open the Local 3D Terrain modal, orbit the 3D cutaway block, scrub the Z-level slider from sky down into deep bedrock, click a hex prism to read its material/stratum report, close the modal, and verify the base World simulation is unchanged.

**Acceptance Scenarios**:

1. **Given** a paused World simulation with a selected tile, **When** the user opens the Local 3D Terrain modal, **Then** an isolated snapshot copy initializes the local 3D hex-prism volume without mutating or subdividing the base World simulation.
2. **Given** an open Local 3D Terrain modal, **When** the user drags the Z-level slider down below surface elevation, **Then** all hex prisms above `z_max` are clipped/hidden and exposed top faces of the sliced layer render cleanly with material colors (highlighting rock strata, veins, soil, water, and open caverns).
3. **Given** a running (unpaused) World simulation, **When** viewing tile controls, **Then** the Local 3D Terrain button is disabled until paused; closing the modal frees the WASM session and WebGL buffers cleanly.

---

### User Story 4 - Reusable Rust Crate API & Sparse Delta Edits for Downstream Games (Priority: P4)

A downstream 3D tactical combat, colony-sim, or voxel game imports a standalone pure-Rust crate (e.g., `tec11-local-terrain`) that depends only on `tec11-proto`, `tec11-math-grid`, and `tec11-geology` (no browser/WASM dependencies). The crate provides chunk streaming by `(chunk_q, chunk_r, chunk_z)`, read/write hex-prism queries (material, solid/fluid/air, elevation, biome/flora, climate/weather context), and sparse delta storage so player digging or building only persists modified chunks while untouched chunks regenerate deterministically on demand.

**Why this priority**: Enables building full tactical combat or Dwarf Fortress–style games directly on top of any Tec11-generated world without coupling game engines to the web UI.

**Independent Test**: In a standalone Rust integration test, load a `.tec11` file through the crate API, stream adjacent 3D chunks, excavate a tunnel (mutating cells), serialize only the sparse delta, reload the world + delta, and verify the tunnel persists while untouched chunks remain zero-storage.

**Acceptance Scenarios**:

1. **Given** a `.tec11` snapshot loaded via the crate API, **When** a caller requests adjacent chunks `(cq, cr, cz)` across a chunk boundary, **Then** surface elevation, strata, rivers, and caves match seamlessly with zero seams or discontinuities at chunk borders.
2. **Given** a caller modifies hex prisms in one chunk (e.g., digging a tunnel or placing stone), **When** saving edits, **Then** only modified chunks/cells are stored as a sparse delta overlay, and untouched chunks consume zero persistent storage.

---

### Edge Cases

- **Coastal & Underwater Tiles**: Selecting a coastal or shallow-shelf tile must cleanly render sea level, bathymetric slope, beach/sediment transition, and submerged bedrock rather than clamping ocean to flat dry land.
- **Pentagonal Parent Tiles**: Because 12 tiles on the geodesic sphere are pentagons, the local tangent-plane transition must handle 5-neighbor parent boundaries cleanly without NaN interpolation or missing neighbors.
- **Extreme Relief (Cliffs / High Peaks)**: Steep mountain slopes within a 128 m × 128 m area must stay within the vertical chunk window (automatically centering the Z-window around the local surface elevation, e.g., `surface_z - 96 .. surface_z + 64`, rather than hardcoding sea level `z = 0`).
- **Pre-Atmosphere Hadean Tiles**: Opening Local 3D Terrain on an airless Tectonics tile must render bare volcanic/cratered rock strata with no soil, rivers, water, or vegetation.
- **River Waterfalls, Steep Gorges & Tributaries**: Where a subdivided river crosses steep elevation drops or resistant rock beds, the water-surface profile may step down cleanly (rapids/waterfalls) while remaining monotonically non-increasing; smaller tributaries spawned at intermediate zoom levels must snap their mouths cleanly to the parent river's local $Z_{\text{water}}$.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST provide a pure-Rust, browser-independent crate (`tec11-local-terrain`) that loads planetary context from `PlanetSnapshot` (`.tec11`) bytes across all three simulation stages (`TectonicsStage`, `PrehistoryStage`, and `HistoryStage`) without mutating the source simulation.
- **FR-002**: System MUST implement lazy hierarchical local-patch subdivision from coarse/History scale (~95 km) down to a local tangent-plane **hexagonal-prism** 3D grid with configurable horizontal hex spacing (default ~1.0 m, supporting 0.5–1.0 m) and discrete vertical z-levels (default 1.0 m height).
- **FR-003**: System MUST organize the 3D local volume into streamable 3D chunks (e.g., 16×16×32 or 32×32×32 hex-prisms) with compact cell storage (`u16` material ID covering exact Tec11 rock types, surficial materials such as soil/loess/till/sand/peat/water/ice/air, and discrete ore/mineral vein voxels derived from `TileGeology` occurrences, plus flags and palette/RLE chunk compression) and seamless cross-chunk continuity.
- **FR-004**: System MUST synthesize surface landforms and underground geology using composable physical operators (laterally continuous stratigraphic blending with neighbor bed correlation, pinch-outs/lenses/interfingering wedges, and continuous structural folding/faulting/intrusions/impact bowls; differential erosion by `Rock::toughness`; glacial vs. fluvial carving; karst dissolution; surficial depositional mantles; and coastal wave/tide beveling) driven strictly by parent and neighbor `TileGeology`, `Rock` properties, glacial history, climate, and tides—never by artificial "mountain age", hardcoded biome-shape labels, or abrupt per-tile cookie-cutter strata boundaries.
- **FR-005**: System MUST NOT merely drape rivers onto coherent-noise terrain; instead, it MUST hierarchically subdivide parent river centerlines (with slope-dependent meandering and discharge-scaled channel width/depth), enforce a monotonically non-increasing water-surface elevation $Z_{\text{water}}$, carve the riverbed below $Z_{\text{water}}$ with alluvial bed sediment, blend a valley/floodplain envelope into surrounding terrain (exposing deeper rock strata in canyons), and produce identical river geometry across adjacent chunks.
- **FR-006**: System MUST consume physical surficial soil state (`soil_depth_m` and `soil_fertility`, persisted on `TileGeology` in `PlanetSnapshot`) to determine topsoil/loess/till depth vs. exposed bedrock, falling back gracefully to bedrock `Rock::soil_fertility()` and current climate when loading older snapshots that omit surficial soil fields.
- **FR-007**: System MUST support sparse delta overlays via a self-contained, versioned binary delta format inside `tec11-local-terrain` (separate from `.tec11` planetary snapshots) so downstream games can mutate individual hex prisms (digging/building) and persist only modified chunks.
- **FR-008**: System MUST expose a thin `tec11-wasm` session wrapper and an isolated, paused-only **Local 3D Terrain** modal in the World UI (available across Tectonics, Prehistory, and History stages) using the same 3D WebGL hex-column renderer at every progressive drill-down level (rendering the available relief, rivers, coasts, and surface geology; clicking a hex drills one level deeper, with controls to step back up, pan laterally at any zoom level via N/S/E/W buttons or WASD/Arrow keys, and jump directly to automatically discovered in-tile Points of Interest such as rivers, peaks/cliffs, coastlines, karst caverns, mineral outcrops, and impact craters), and unlocking full 3D underground cutaway/orbit navigation, Dwarf Fortress–style interactive Z-level slicing, depth-window shifting (`-100 m` / `+100 m`), and click-to-inspect hex-prism details at the ~1 m leaf scale.
- **FR-009**: Closing the Local 3D Terrain modal MUST dispose all WebGL geometry/materials and free WASM memory while leaving the underlying World simulation untouched.

### Key Entities

- **LocalZoomAnchor**: Target location on a `PlanetSnapshot` (coarse/fine tile ID, sub-tile tangent coordinates, world seed, and interpolated local planetary context: elevation, slope, mountainosity, strata stack, mineral fields, soil fertility, river entry/exit segments, climate, sea level).
- **RiverSplineSegment**: Deterministically subdivided river centerline segment carrying upstream/downstream endpoints, monotonically non-increasing water-surface elevation $Z_{\text{water}}$, channel width $W$, channel depth $D$, valley/floodplain half-width $V$, and discharge.
- **HexPrismCell**: Compact per-cell voxel record (`material_id: u16` encoding exact Tec11 rock types, surficial soil/loess/till/sand/peat/water/ice/air, and discrete ore/mineral vein voxels from `TileGeology` occurrences; `fluid_state`; and `flags` such as natural vs. excavated, exposed surface, flora/groundcover).
- **HexChunk**: Fixed-size 3D chunk of hex prisms indexed by `(chunk_q, chunk_r, chunk_z)` with palette/RLE compression and dirty-mesh tracking.
- **LocalTerrainVolume**: Streamed collection of `HexChunk`s around a `LocalZoomAnchor`, plus an optional sparse `DeltaOverlay` for player modifications.
- **LocalTerrainMesher**: Face-culled hex-prism mesh builder generating exposed side/top/bottom faces (skipping occluded interior faces) and supporting fast dynamic `z_max` slicing.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Generating a 128×128×128 local hex-prism volume from a `.tec11` snapshot completes in under 500 ms in native Rust and under 1.5 seconds in WASM.
- **SC-002**: Peak memory for a 128×128×256 chunked local volume (including culled surface/cutaway mesh) stays under 16 MB in WASM.
- **SC-003**: Scrubbing the Z-level slicer in the browser updates the visible sliced layer interactively (< 50 ms per slice step) with zero missing faces or visual seams across chunk boundaries.
- **SC-004**: Repeatedly loading the same anchor from a `.tec11` snapshot produces 100% identical hex-prism contents, and opening/closing the World modal causes zero mutation to the base World simulation.
- **SC-005**: Along every subdivided river segment passing through a local volume, water-surface elevation $Z_{\text{water}}$ is strictly non-increasing downstream, riverbed elevation lies strictly below $Z_{\text{water}}$ across the channel interior, and surrounding valley banks slope into the channel with zero chunk-boundary seams.
- **SC-006**: Drilling down at two distant points within the same coarse/fine tile yields visibly distinct underground bed depths, thicknesses, and pinched-out strata, while traversing across the boundary between two neighboring tiles transitions smoothly via bed thickness tapering/interfingering with zero abrupt vertical stratigraphic discontinuities.

## Assumptions

- Initial v1 focuses on terrain, underground strata/caves/ores, water/rivers, and static groundcover/flora markers; dynamic fluid simulation, lighting propagation, AI pathfinding, and combat rules belong to downstream game layers consuming `tec11-local-terrain`.
- Default horizontal hex-prism spacing is 1.0 m and vertical z-level height is 1.0 m (configurable in the crate API for 0.5 m tactical scales).
- Planetary curvature over a single loaded local embark (< 1 km) is negligible (< 1 cm sag) and is cleanly approximated by a flat local tangent plane oriented to the parent tile's local north/east/up frame.
