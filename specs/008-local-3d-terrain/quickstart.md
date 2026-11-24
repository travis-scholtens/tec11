# Quickstart & Validation Guide: Local 3D Hex-Prism Terrain & Underground Zoom

**Feature**: `008-local-3d-terrain`

## Prerequisites

- Bazel workspace in `/home/travis/tec11`
- Disk cache path: `~/.cache/bazel-disk-cache`

## 1. Pure-Rust Crate Unit & Integration Tests

Run the full test suite for `tec11-local-terrain` (and verify no regressions in `tec11-geology` and `tec11-proto`):

```bash
bazel test -c opt --incompatible_strict_action_env --disk_cache=~/.cache/bazel-disk-cache \
  //crates/tec11-local-terrain/... \
  //crates/tec11-geology/... \
  //crates/tec11-proto/...
```

### Expected Test Scenarios Validated
1. **Determinism & Isolation (`SC-004`)**: Loading the same `.tec11` snapshot bytes twice produces bit-for-bit identical `HexChunk` contents across all 3 stages (`Tectonics`, `Prehistory`, `History`), with zero mutation to source snapshots.
2. **Pentagon & Boundary Safety**: Selecting one of the 12 pentagonal tiles on the geodesic grid produces finite elevations and valid strata with zero NaNs.
3. **Lateral Stratigraphic Continuity (`SC-006`)**: Two distant points inside one coarse/fine tile exhibit different bed depths/thicknesses/pinch-outs; stepping across a neighbor tile boundary shows smooth thickness tapering with zero vertical cookie-cutter jumps.
4. **Geomorphology Contrasts (`US2`)**:
   - Flat alternating sedimentary plateau → stepped cliff-and-bench terraces.
   - Glaciated alpine mountain vs. unglaciated orogen → U-valley vs. V-gorge cross-sections.
   - Humid high-`calcity` limestone → underground karst caverns inside soluble beds.
   - Stripped vs. loess-enriched `soil_depth_m` → bare scoured bedrock vs. deep topsoil/loess horizon.
   - Airless Hadean Tectonics tile → bare rock/crater strata with zero water/soil/flora.
5. **Monotonic River Carving (`SC-005`)**: Every `RiverSplineSegment` has `z_water_1 <= z_water_0`, riverbed strictly below `z_water`, and seamless continuity across adjacent `ChunkCoord` borders.
6. **Memory & Performance (`SC-001`, `SC-002`)**: Generating a 128×128×128 volume completes in `< 500 ms` native, and a 128×128×256 volume stays `< 16 MB` (`estimated_memory_bytes()`).
7. **Sparse Delta Round-Trip (`US4`, `FR-007`)**: Excavating a tunnel mutates only touched chunks, exports a compact binary delta (`T11D`), reloads cleanly onto a fresh volume, and restores the exact excavated cells while untouched chunks take zero delta bytes.

## 2. WASM & Browser Bundle Build

```bash
bazel build -c opt --incompatible_strict_action_env --disk_cache=~/.cache/bazel-disk-cache //:web_bundle
```

## 3. Manual Browser Verification (`US3`)

1. Serve `bazel-bin/dist` locally and open a World simulation (test in Tectonics, Prehistory, and History).
2. Confirm **Local 3D Terrain** is disabled while unpaused; pause the simulation and click a land or coastal tile.
3. Open **Local 3D Terrain** → verify Level 0 3D hex-column view renders with relief, rivers, and coasts.
4. Click a hex to drill down Level 0 → Level 1 → Level 2 → Level 3 (~1 m 3D hex-prism cutaway). Use **Step Up** to confirm stepping back up works cleanly.
5. At Level 3, orbit the 3D cutaway block, scrub the **Z-Level Slider** down into bedrock to reveal folded strata, ore veins, and karst caves (`< 50 ms` per step), and click a hex prism to read its material/stratum/depth report.
6. Close the modal and verify the base World simulation is unchanged and responsive.
