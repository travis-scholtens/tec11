# Quickstart Validation Guide: Spec 009 (Volcanoes & Calderas)

## Prerequisites
- Bazel with `--incompatible_strict_action_env --disk_cache=~/.cache/bazel-disk-cache`
- Existing `.tec11` planet save (e.g. `/home/travis/tec11-world (12).tec11`) or a newly generated planet.

## Validation Commands
1. **Run unit & integration tests**:
   ```bash
   bazel test -c opt --incompatible_strict_action_env --disk_cache=~/.cache/bazel-disk-cache \
     //crates/tec11-local-terrain/... //crates/tec11-geology/... //crates/tec11-proto/...
   ```
   Expected: All tests pass, including morphology slope/profile tests (`SC-002`), crater-lake climate gating (`FR-004`), river monotonicity over volcanic flanks (`SC-003`), cross-tile re-anchor invariance `< 0.01 m` (`SC-004`), and Level 3 conduit/dike/sulfur/lava-tube tests (`FR-006`).

2. **Build web bundle**:
   ```bash
   bazel build -c opt --incompatible_strict_action_env --disk_cache=~/.cache/bazel-disk-cache //:web_bundle
   ```
   Expected: Clean build of `bazel-bin/dist/{index.html,bundle.js,tec11_wasm_bg.wasm}`.

3. **Interactive verification in browser**:
   - Open Local 3D Terrain on a subduction-arc coastal mountain tile or oceanic hotspot island.
   - Click the `Stratovolcano` / `Shield Volcano` / `Caldera` / `Crater Lake` / `Volcanic Neck` chip in `Jump to:`.
   - Inspect Level 1 (full cone/caldera), Level 2 (summit crater & flank cinder cones), and Level 3 (scrub Z-slice downward to inspect the vertical igneous conduit, radial dikes, fumarolic sulfur crusts, and basalt lava tubes).
