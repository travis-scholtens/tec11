//! Thin `wasm_bindgen` session wrapper for the Local 3D Terrain modal (`FR-008`).

use tec11_local_terrain::{
    xy_m_to_axial, DrillDownPatch, HexMeshBuffers, LocalPointOfInterest, LocalTerrainMesher,
    LocalTerrainVolume, LocalTerrainWorld, LocalVolumeConfig,
};
use wasm_bindgen::prelude::*;

const PATCH_RADIUS_HEX: i32 = 20;
const LEAF_HALF_EXTENT: i32 = 28; // 56x56 hex columns (~56m x 56m cutaway block, <50ms Z-slice in WASM)
const LEAF_Z_SPAN: i32 = 72; // 72 vertical 1m z-levels per loaded depth window

#[wasm_bindgen]
pub struct WasmLocalTerrainSession {
    world: LocalTerrainWorld,
    pois: Vec<LocalPointOfInterest>,
    /// Stack of `(level, center_xy_m)` for progressive drill-down & stepping back up.
    history: Vec<(u8, [f64; 2])>,
    zoom_level: u8,
    center_xy_m: [f64; 2],
    active_patch: Option<DrillDownPatch>,
    active_volume: Option<LocalTerrainVolume>,
    z_min: i32,
    z_max: i32,
    z_slice: i32,
    mesh: HexMeshBuffers,
}

#[wasm_bindgen]
impl WasmLocalTerrainSession {
    #[wasm_bindgen(constructor)]
    pub fn new(
        snapshot_bytes: &[u8],
        parent_tile_id: usize,
        surface_elev_override_km: Option<f32>,
    ) -> Result<WasmLocalTerrainSession, JsValue> {
        let mut anchor = tec11_local_terrain::LocalZoomAnchor::from_snapshot_bytes(
            snapshot_bytes,
            parent_tile_id,
        )
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
        if let Some(elev_km) = surface_elev_override_km {
            anchor.apply_surface_elevation_override(elev_km);
        }
        let world = LocalTerrainWorld::new(anchor, LocalVolumeConfig::default());

        let pois = world.find_points_of_interest();
        let mut session = Self {
            world,
            pois,
            history: Vec::new(),
            zoom_level: 0,
            center_xy_m: [0.0, 0.0],
            active_patch: None,
            active_volume: None,
            z_min: -48,
            z_max: 24,
            z_slice: 24,
            mesh: HexMeshBuffers::default(),
        };
        session.rebuild_for_current_level();
        Ok(session)
    }

    pub fn zoom_level(&self) -> u8 {
        self.zoom_level
    }

    pub fn z_min(&self) -> i32 {
        self.z_min
    }

    pub fn z_max(&self) -> i32 {
        self.z_max
    }

    pub fn z_slice(&self) -> i32 {
        self.z_slice
    }

    /// Drill one level deeper centered on Three.js mesh hit `(x_mesh, z_mesh)` from Levels 0–2.
    pub fn drill_down_at_mesh_xz(&mut self, x_mesh: f32, z_mesh: f32) -> bool {
        if self.zoom_level >= 3 {
            return false;
        }
        let Some(ref patch) = self.active_patch else {
            return false;
        };
        let span_xy = ((patch.radius_hex * 2 + 1) as f32) * patch.hex_spacing_m;
        let scale = 40.0 / span_xy.max(1.0);
        let rel_x = (x_mesh / scale) as f64;
        let rel_y = (z_mesh / scale) as f64;
        let (q, r) = xy_m_to_axial(rel_x, rel_y, patch.hex_spacing_m);
        let next_center = patch
            .columns
            .iter()
            .find(|c| c.q == q && c.r == r)
            .map(|c| c.local_xy_m)
            .unwrap_or([self.center_xy_m[0] + rel_x, self.center_xy_m[1] + rel_y]);

        self.history.push((self.zoom_level, self.center_xy_m));
        self.zoom_level += 1;
        self.center_xy_m = next_center;
        self.rebuild_for_current_level();
        true
    }

    /// Step back up one zoom level.
    pub fn step_up(&mut self) -> bool {
        let Some((prev_lvl, prev_center)) = self.history.pop() else {
            if self.zoom_level > 0 {
                self.zoom_level -= 1;
                self.rebuild_for_current_level();
                return true;
            }
            return false;
        };
        self.zoom_level = prev_lvl;
        self.center_xy_m = prev_center;
        self.rebuild_for_current_level();
        true
    }

    /// Jump straight to Level 3 (~1 m 3D cutaway & Z-level slicer) at the current center.
    pub fn jump_to_leaf(&mut self) {
        if self.zoom_level < 3 {
            self.history.push((self.zoom_level, self.center_xy_m));
            self.zoom_level = 3;
            self.rebuild_for_current_level();
        }
    }

    /// Pan laterally in the local tangent plane at ANY zoom level (`0..=3`).
    /// `(dir_east, dir_north)` is a unit or cardinal direction vector; each step moves by
    /// ~half the current visible patch/embark width so consecutive views overlap seamlessly.
    pub fn pan_lateral(&mut self, dir_east: f32, dir_north: f32) {
        let len = (dir_east * dir_east + dir_north * dir_north).sqrt();
        if len < 1e-4 {
            return;
        }
        let spacing = self.world.hex_spacing_for_level(self.zoom_level) as f64;
        let step_m = if self.zoom_level < 3 {
            spacing * (PATCH_RADIUS_HEX as f64) * 0.65
        } else {
            spacing * (LEAF_HALF_EXTENT as f64)
        };
        let dx = ((dir_east / len) as f64) * step_m;
        let dy = ((dir_north / len) as f64) * step_m;
        self.center_xy_m[0] += dx;
        self.center_xy_m[1] += dy;

        if self.zoom_level < 3 {
            self.rebuild_for_current_level();
        } else {
            // Preserve the user's current depth offset relative to local surface elevation
            let prev_offset = self
                .active_volume
                .as_ref()
                .map(|v| self.z_slice - v.surface_z_center)
                .unwrap_or(LEAF_Z_SPAN - 48);
            let mut vol = self.world.create_leaf_volume(self.center_xy_m);
            self.z_min = vol.surface_z_center - 48;
            self.z_max = self.z_min + LEAF_Z_SPAN;
            self.z_slice = (vol.surface_z_center + prev_offset).clamp(self.z_min, self.z_max);
            self.mesh = LocalTerrainMesher::build_volume_mesh(
                &mut vol,
                -LEAF_HALF_EXTENT..LEAF_HALF_EXTENT,
                -LEAF_HALF_EXTENT..LEAF_HALF_EXTENT,
                self.z_min,
                self.z_slice,
            );
            self.active_volume = Some(vol);
        }
    }

    /// Jump directly to a discovered in-tile Point of Interest by index.
    /// Drills to at least Level 2 so the feature is clearly visible, or stays at Level 3 if already at leaf.
    pub fn jump_to_poi(&mut self, poi_index: usize) -> bool {
        let Some(poi) = self.pois.get(poi_index) else {
            return false;
        };
        self.history.push((self.zoom_level, self.center_xy_m));
        self.center_xy_m = poi.local_xy_m;
        if self.zoom_level < 2 {
            self.zoom_level = 2;
        }
        self.rebuild_for_current_level();
        true
    }

    /// Update the active Z-level slice (Level 3) and rebuild the exposed slice mesh.
    pub fn set_z_slice(&mut self, z_slice: i32) {
        if self.zoom_level < 3 {
            return;
        }
        let clamped = z_slice.clamp(self.z_min, self.z_max);
        if clamped == self.z_slice {
            return;
        }
        self.z_slice = clamped;
        if let Some(ref mut vol) = self.active_volume {
            self.mesh = LocalTerrainMesher::build_volume_mesh(
                vol,
                -LEAF_HALF_EXTENT..LEAF_HALF_EXTENT,
                -LEAF_HALF_EXTENT..LEAF_HALF_EXTENT,
                self.z_min,
                self.z_slice,
            );
        }
    }

    /// Shift the loaded Level 3 vertical Z-window deeper or shallower (e.g., `-100` or `+100` m)
    /// to stream deeper strata & ore contacts down toward the Moho on thick, many-layered tiles.
    pub fn shift_z_window(&mut self, delta_z: i32) {
        if self.zoom_level < 3 || delta_z == 0 {
            return;
        }
        self.z_min += delta_z;
        self.z_max += delta_z;
        self.z_slice = (self.z_slice + delta_z).clamp(self.z_min, self.z_max);
        if let Some(ref mut vol) = self.active_volume {
            self.mesh = LocalTerrainMesher::build_volume_mesh(
                vol,
                -LEAF_HALF_EXTENT..LEAF_HALF_EXTENT,
                -LEAF_HALF_EXTENT..LEAF_HALF_EXTENT,
                self.z_min,
                self.z_slice,
            );
        }
    }

    pub fn mesh_positions(&self) -> Vec<f32> {
        self.mesh.positions.clone()
    }

    pub fn mesh_normals(&self) -> Vec<f32> {
        self.mesh.normals.clone()
    }

    pub fn mesh_colors(&self) -> Vec<f32> {
        self.mesh.colors.clone()
    }

    /// Inspect a Three.js raycaster hit `(x_mesh, y_mesh, z_mesh)` at Level 3 (or hover/click at Levels 0–2).
    pub fn inspect_hit_json(&mut self, x_mesh: f32, y_mesh: f32, z_mesh: f32) -> String {
        if self.zoom_level < 3 {
            let Some(ref patch) = self.active_patch else {
                return "{\"summary\":\"\"}".to_string();
            };
            let span_xy = ((patch.radius_hex * 2 + 1) as f32) * patch.hex_spacing_m;
            let scale = 40.0 / span_xy.max(1.0);
            let (q, r) = xy_m_to_axial((x_mesh / scale) as f64, (z_mesh / scale) as f64, patch.hex_spacing_m);
            if let Some(col) = patch.columns.iter().find(|c| c.q == q && c.r == r) {
                let river_tag = if col.is_river { " · River" } else { "" };
                let summary = format!(
                    "{} · Elev {:.0} m · Soil {:.1} m{} (click to drill down)",
                    col.surface_material.name(),
                    col.surface_elev_m,
                    col.soil_depth_m,
                    river_tag
                );
                return format!(
                    "{{\"q\":{},\"r\":{},\"z\":0,\"materialName\":\"{}\",\"summary\":\"{}\"}}",
                    q,
                    r,
                    escape_json(col.surface_material.name()),
                    escape_json(&summary)
                );
            }
            return "{\"summary\":\"Click any hex to drill down\"}".to_string();
        }

        let Some(ref mut vol) = self.active_volume else {
            return "{\"summary\":\"\"}".to_string();
        };
        let spacing = vol.config().hex_spacing_m;
        let z_step = vol.config().z_step_m;
        let (q, r) = xy_m_to_axial(x_mesh as f64, z_mesh as f64, spacing);
        let iz = ((y_mesh / z_step) - 0.05).floor() as i32;
        let z = (self.z_min + iz).clamp(self.z_min, self.z_slice);
        let insp = vol.inspect_column(q, r, z);
        format!(
            "{{\"q\":{},\"r\":{},\"z\":{},\"materialName\":\"{}\",\"rockKind\":\"{}\",\"depthBelowSurfaceM\":{:.1},\"elevationM\":{:.1},\"soilDepthM\":{:.1},\"isFluid\":{},\"isOre\":{},\"summary\":\"{}\"}}",
            insp.q,
            insp.r,
            insp.z,
            escape_json(insp.cell.material.name()),
            escape_json(insp.cell.material.category_name()),
            insp.depth_below_surface_m,
            insp.elevation_m,
            insp.soil_depth_m,
            if insp.cell.material.is_fluid() { "true" } else { "false" },
            if insp.cell.material.is_ore() { "true" } else { "false" },
            escape_json(&insp.summary)
        )
    }

    pub fn status_json(&self) -> String {
        let spacing = self.world.hex_spacing_for_level(self.zoom_level);
        let surface_elev = self
            .active_volume
            .as_ref()
            .map(|v| v.surface_z_center)
            .unwrap_or_else(|| self.world.anchor().center().elevation_m.round() as i32);
        let level_label = match self.zoom_level {
            0 => format!("Level 0 · Tile Scale (~{:.1} km/hex)", spacing / 1000.0),
            1 => format!("Level 1 · District (~{:.0} m/hex)", spacing),
            2 => format!("Level 2 · Valley/Ridge (~{:.0} m/hex)", spacing),
            _ => format!("Level 3 · Local 3D Embark ({:.1} m/hex)", spacing),
        };
        let pois_json: Vec<String> = self
            .pois
            .iter()
            .map(|p| {
                format!(
                    "{{\"kind\":\"{}\",\"label\":\"{}\",\"description\":\"{}\"}}",
                    escape_json(p.kind),
                    escape_json(&p.label),
                    escape_json(&p.description)
                )
            })
            .collect();
        format!(
            "{{\"zoomLevel\":{},\"hexSpacingM\":{:.2},\"zMin\":{},\"zMax\":{},\"zSlice\":{},\"surfaceElevM\":{},\"canStepUp\":{},\"isLeaf\":{},\"levelLabel\":\"{}\",\"stage\":\"{}\",\"tileId\":{},\"pois\":[{}]}}",
            self.zoom_level,
            spacing,
            self.z_min,
            self.z_max,
            self.z_slice,
            surface_elev,
            if self.zoom_level > 0 { "true" } else { "false" },
            if self.zoom_level >= 3 { "true" } else { "false" },
            escape_json(&level_label),
            self.world.anchor().stage.name(),
            self.world.anchor().parent_tile_id,
            pois_json.join(",")
        )
    }

    fn rebuild_for_current_level(&mut self) {
        if self.zoom_level < 3 {
            let patch = self.world.generate_drilldown_patch(
                self.zoom_level,
                self.center_xy_m,
                PATCH_RADIUS_HEX,
            );
            self.mesh = LocalTerrainMesher::build_patch_mesh(&patch);
            self.active_patch = Some(patch);
            self.active_volume = None;
        } else {
            let mut vol = self.world.create_leaf_volume(self.center_xy_m);
            // Center vertical window around local surface so both relief & ~50 m underground show cleanly
            self.z_min = vol.surface_z_center - 48;
            self.z_max = self.z_min + LEAF_Z_SPAN;
            self.z_slice = self.z_max;
            self.mesh = LocalTerrainMesher::build_volume_mesh(
                &mut vol,
                -LEAF_HALF_EXTENT..LEAF_HALF_EXTENT,
                -LEAF_HALF_EXTENT..LEAF_HALF_EXTENT,
                self.z_min,
                self.z_slice,
            );
            self.active_volume = Some(vol);
            self.active_patch = None;
        }
    }
}

fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
