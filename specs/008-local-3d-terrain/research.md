# Research & Architectural Decisions: Local 3D Hex-Prism Terrain & Underground Zoom

**Feature**: `008-local-3d-terrain` | **Date**: 2026-11-24

## 1. Progressive Hierarchical Drill-Down & Coordinate System

- **Decision**: Use a 4-level progressive drill-down cascade on a local tangent-plane axial hexagonal grid `(q, r)` centered at the selected planet tile's unit-sphere normal `N` with orthonormal East (`E`) and North (`U`) axes:
  - **Level 0 (Tile Scale)**: ~1.5–4.0 km hex spacing (covers the selected parent tile + 1-ring border, ~64×64 hex columns).
  - **Level 1 (District / Basin Scale)**: ~250–500 m hex spacing (~8× zoom into clicked hex, ~64×64 hex columns).
  - **Level 2 (Valley / Ridge Scale)**: ~25–50 m hex spacing (~10× zoom into clicked hex, ~64×64 hex columns).
  - **Level 3 (Local 3D Embark / Leaf Scale)**: configurable `0.5–1.0 m` hex spacing (default `1.0 m`, 128×128 hex columns × 128–256 vertical 1 m z-levels, chunked in `16×16×32` 3D chunks).
- **Rationale**:
  - A 95 km fine tile (or ~285 km coarse tile) cannot be subdivided globally to 1 m (that would require $>10^{10}$ columns per tile).
  - Projecting the parent tile and its 1-ring of 5 or 6 neighbors onto the tangent plane eliminates `f32` spherical precision loss below ~1 km and handles both hexagonal and the 12 pentagonal geodesic tiles identically via smooth inverse-distance / barycentric weighting over the neighbor set.
  - Exposing Levels 0–2 as interactive 3D hex-column patches using the same Three.js hex-column renderer lets the user visually inspect rivers, coasts, ridges, and geology at each scale and click anywhere to drill deeper (or step back up), naturally solving sub-tile navigation.
- **Alternatives Considered**:
  - *Global geodesic subdivision*: Exponentially intractable in memory and CPU.
  - *Single jump straight from 95 km tile center to 128 m*: Users miss rivers, cliffs, or coasts located away from the tile center.
  - *2D top-down canvas for intermediate levels*: Requires maintaining two separate renderers and feels less cohesive than a unified 3D hex-column view.

## 2. Laterally Continuous Stratigraphic Blending & Structural Warping

- **Decision**: Build a `RegionalStratigraphyField` from the anchor tile and its 1-ring neighbors (5 or 6 tiles):
  1. **Smooth spatial weights** $w_i(x, y)$ computed from tangent-plane distances to the anchor and neighbor centers (normalized Shepard / Wendland $C^2$ radial weights, ensuring $w_i$ varies smoothly with zero discontinuities across Voronoi tile edges).
  2. **Bed correlation**: Align layers across the center and neighbor `TileGeology.layers` stacks by matching `(RockKind, rock.name, felsity_band)` in top-to-bottom stratigraphic order.
  3. **Thickness interpolation & pinch-outs**: Shared correlated beds interpolate thickness as $T(x,y) = \sum_i w_i(x,y) T_i$. Non-shared beds (present in some tiles but absent in neighbors) taper smoothly to `0` as their weight drops, modulated by deterministic low-frequency geological pinch-out noise (`±15–25%` of local transition width) so they form natural thinning wedges, lenses, and interfingering tongues rather than straight vertical walls.
  4. **Continuous structural deformation**: Horizon depths are shifted by a continuous 3D structural field combining:
     - Regional elevation/bottom dip from interpolated neighbor columns,
     - Multi-octave folding (anticlines/synclines whose amplitude scales with interpolated `mountainosity`),
     - Fault offsets along deterministic fracture planes in high-`mountainosity` or high-`subduction` zones,
     - 3D intrusive domes/batholiths/dykes when `intrusion` or intrusive rocks (`granite`, `gabbro`, `kimberlite`) are present,
     - Bowl/rim excavation and ejecta breccia when `impact_scar > 0`.
- **Rationale**: Ensures two distant drill-down spots inside the same 95–285 km tile show genuinely different bed depths, thicknesses, and pinched-out strata, while traveling across tile boundaries produces zero abrupt vertical seams (SC-006).
- **Alternatives Considered**:
  - *Nearest-tile lookup + local noise*: Creates jarring vertical "cookie-cutter" rock walls at every tile border and makes the entire interior of a tile feel stratigraphically uniform.

## 3. Composable 6-Operator Physical Geomorphology Pipeline

- **Decision**: Evaluate each local `(x, y)` column through six pure, deterministic physical operators parameterized strictly by interpolated parent/neighbor Tec11 fields:
  1. **Stratigraphic & Structural Operator**: Base elevation + folded/dipped strata + intrusive/impact relief.
  2. **Differential Erosion Operator**: Computes local resistance from near-surface `Rock::toughness` and layer alternation; soft beds (`claystone`, `siltstone`, `chalk`) erode faster than hard caprocks (`sandstone`, `basalt`, `granite`, `quartzite`), producing stepped cliffs/benches on low-`mountainosity` plateaus (Catskills/mesa style) and ridge-and-valley hogbacks on folded strata (Appalachian style).
  3. **Glacial vs. Fluvial Carving Operator**: Driven by physical `ClimateModel::is_tile_glaciated` / glacier cover, alpine freezing temperatures, and thin stripped `soil_depth_m` vs. humid precipitation; glaciated alpine regions carve parabolic U-valleys, cirques, horns, and arêtes (Alps/fjord style), while unglaciated high-relief orogens carve steep fluvial V-gorges (Himalayas style).
  4. **Karst Dissolution Operator**: Where `calcity > 0.65` (`limestone`, `chalk`, `marble`) or high `salinity` evaporites coincide with humid precipitation (`precip_norm > 0.25`), carves surface sinkholes/dolines and 3D underground cavern/aquifer networks strictly inside the soluble strata using 3D worm/ridged noise constrained to the soluble bed Z-bounds.
  5. **Surficial Mantle Operator**: Uses persisted `TileGeology::soil_depth_m` and `soil_fertility` directly (FR-006) to determine topsoil/loess/till thickness vs. exposed scoured bedrock; adds arid wind-aligned dunes/playas (low precip / high `salinity`), peat bogs (`bogginess`), volcanic benches/domes (extrusive igneous), and talus/scree on steep slopes (`slope > 32°`).
  6. **Coastal Wave & Tide Beveling Operator**: Near eustatic sea level (`z ≈ 0`), tough rock coasts carve sea cliffs and wave-cut platforms while soft sedimentary/low-slope coasts form beaches, barrier bars, and tidal flats scaled by local tidal range.
- **Rationale**: Directly enforces Constitution Principles I & II without reifying "mountain age" or hardcoded biome templates.

## 4. Hydrology-First Vector Drainage Network & Non-Negative Relief Envelope

- **Decision**: Replace signed-noise + spline-stamping (which causes aqueducts and uphill waterfalls) and finite-tile D6 pyramids (which suffer tile-boundary seams and 1 m single-cell collapse) with a **Hydrology-First Vector Drainage Network (`DrainageNetwork`) + Non-Negative Relief Envelope** inspired by Génevaux et al. (2013):
  1. **Stage-Faithful Grounding**: In `History` stage, `PlanetaryTerrainContext` reconstructs History's `SubdividedTerrainGrid` (`from_coarse_with_crust` + `route_rivers`), grounding Level 0–3 coastlines and mainstem rivers 1:1 on the Regional View. In `Tectonics` and `Prehistory`, grounding uses the 2-ring coarse geology grid + `ClimateModel` + `trace_coarse_rivers`.
  2. **Anchor-Invariant Mainstem & Multi-Order Tributary Tree**:
     - Subdivide each mainstem edge with slope- and `Rock::toughness`-dependent meandering followed by 3 Chaikin $C^1$ corner-cutting passes, keyed purely on edge-local coordinates so curves never shift during unlimited pan re-anchoring.
     - Assign strictly non-increasing $Z_{\text{water}}$ downstream across the entire graph (allowing a river to incise a deep canyon through a higher coarse tile without contradicting tile average elevation).
     - On humid valley walls (`precip_norm > 0.22`), spawn deterministic 1st/2nd-order `DrainageReach` tributaries branching uphill from mainstem reaches, snapping their mouths to the parent's exact $Z_{\text{water}}$ at the confluence and rising monotonically up the valley slopes.
     - Scale active channel width and depth via Leopold-Maddock hydraulic geometry ($W \propto Q^{0.52}$, `10 m` creeks up to `2,500+ m` wet-basin mainstems; $D \propto Q^{0.40}$), with rock-toughness gorge narrowing, coastal estuary flaring near sea level, asymmetric outer cut-bank / inner point-bar profiles, and streamlined braided sandbars in wide low-slope channels.
  3. **Non-Negative Relief Envelope ($z = z_{\text{valley}} + R(d, x, y)$)**:
     - Inside the active drainage corridor (`valley_blend > 0`), construct surrounding land elevation **strictly upward** from the river's $Z_{\text{water}}$:
       - At the channel bank ($d = W/2$): $R = 0$, so bank elevation equals $Z_{\text{water}}$ cleanly.
       - Moving away from the bank ($d > W/2$): $R(d, x, y) \ge 0$ rises using a **glacial parabolic U-valley** profile ($\propto u^2$) where `is_glaciated || glacier_cover > 0.2` or a **fluvial V-valley** profile ($\propto u$) otherwise, scaled by `mountainosity` and modulated by **non-negative** ridge/macro/meso noise + `Rock::toughness` stepped cliff-and-bench plateaus, blending smoothly into the regional elevation field outside the valley envelope.
     - Because $R \ge 0$ everywhere inside the valley corridor, terrain **never** dips below $Z_{\text{water}}$ — eliminating floating aqueducts and uphill waterfalls by mathematical construction, with zero clamps and zero tile seams across all 4 zoom levels (`6 km` → `750 m` → `32 m` → `1 m`).
  4. **Climate- & Rock-Gated Structural Lakes (`LakeBasin`)**:
     - Detect structural basins (impact craters where `impact_scar > 0.25`, glaciated alpine cirques/troughs, and inland depressions) as flat-water nodes (`z_lake`) in the drainage graph.
     - Gate surface water physically:
       - **Evaporation vs. inflow**: Hot, arid basins where `evap(annual_temp_c, precip_norm) > inflow` dry into `SALT_CRUST` playas or dry bowls.
       - **Bedrock permeability**: High-`calcity` karst (`calcity > 0.65`) and coarse permeable sandstone drain underground into aquifers/caverns rather than holding surface lakes; impermeable granite/basalt/claystone/glacially scoured shields retain surface lakes.
       - **Freezing**: Lakes with `annual_temp_c < -2 °C` freeze over with `ICE` / `SNOW`.
- **Rationale**:
  - Gives the scale consistency of an ideal 1 m globe in $O(1)$ per column (`< 50 ms` per update in WASM), preserves true physical river widths at Level 3 (`1 m/hex`), supports unlimited panning with zero tile seams, and upholds Constitution Principles I & II.
- **Alternatives Considered**:
  - *Signed fBM + clamped river splines (Pass 1)*: Produced floating aqueducts and uphill waterfalls whenever noise dipped below $Z_{\text{water}}$.
  - *Tiled Priority-Flood + D6 erosion pyramid (Pass 2)*: Suffered border-portal seams across tiles, circular upsampling beads, and 1-pixel D6 collapse at `1 m/hex`.

## 5. Compact Chunk Storage, Material Palette & Dynamic Z-Slicing Mesher

- **Decision**:
  - **Chunk size**: `16 × 16 × 32` hex prisms (`8,192` cells per chunk). A `128 × 128 × 128` embark is `8 × 8 × 4 = 256` chunks; `128 × 128 × 256` is `512` chunks.
  - **Cell representation**: `HexPrismCell` packed into `u32` (or `u16 material_id` + `u8 fluid_state` + `u8 flags`), with per-chunk palette + run-length encoding (RLE) for cold/uniform chunks (air above surface and solid deep bedrock compress >20×, keeping a 128×128×256 volume well under 4–8 MB in RAM, comfortably inside the 16 MB SC-002 budget).
  - **Material palette (`MaterialId(u16)`)**: Exact 1-to-1 entries for all Tec11 igneous/sedimentary/metamorphic rocks, surficial materials (`Topsoil`, `LoessTill`, `AlluvialSilt`, `Sand`, `Gravel`, `Peat`, `Talus`, `Water`, `Ice`, `Snow`, `Air`), and discrete ore/mineral vein voxels (`NativeCopper`, `IronOre`, `GoldVein`, `SilverVein`, `TinOre`, `CoalSeam`, `GemstoneVein`, `QuartzVein`, `SaltEvaporite`, `SulfurDeposit`, `KimberliteDiamond`) spawned inside host beds where `TileGeology::derive_layer_occurrences` indicates mineralized strata/interfaces.
  - **Mesher & Z-Slicing**: Build face-culled hex-prism geometry (6 vertical side faces + top/bottom caps, emitting only faces adjacent to `Air`/`Water` or the chunk cutaway boundary). For fast `< 50 ms` Z-level slicing (SC-003), organize emitted geometry or slice rebuilds per z-band (only regenerating the single newly exposed `z = z_max` horizontal cap layer and hiding/clipping geometry above `z_max`), keeping slice scrubbing instantaneous in WebGL.

## 6. Self-Contained Binary Delta Overlay (`DeltaOverlay`)

- **Decision**: Implement a versioned, little-endian binary codec directly in `tec11-local-terrain` (`magic "T11D"`, `version: u16`, `anchor_hash: u64`, list of `(chunk_q: i32, chunk_r: i32, chunk_z: i32)` entries each containing sparse `(local_idx: u16, HexPrismCell)` overrides).
- **Rationale**: Zero changes to `proto/tec11.proto`, zero extra crate dependencies, compact byte size (only mutated cells stored), and deterministic round-trip for downstream games (FR-007).
