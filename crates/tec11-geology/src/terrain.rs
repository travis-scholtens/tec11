use std::collections::BTreeSet;

use tec11_math_grid::{
    DisplayFaceSource, DisplayMesh, FineParentage, SphericalGrid, SubdividedTerrainGrid,
};
use tec11_proto::PlanetSnapshotExt;

use crate::prehistory::PrehistorySimulation;
use crate::rivers::trace_coarse_rivers;
use crate::tile::TileGeology;

pub const TERRAIN_MIN_RIVER_ELEV: f32 = 5.0;
pub const TERRAIN_MIN_RIVER_PRECIP: f32 = 0.5;

/// Elevation colormap matching `color.py:elevationcolor(elevation)`.
#[inline]
pub fn elevation_color_rgb(elev_km: f32) -> [f32; 3] {
    if elev_km <= 0.0 {
        return [0.0, 0.0, 1.0];
    }
    let scale = (elev_km / 10.0).clamp(0.0, 1.0);
    if scale >= 0.5 {
        let v = (scale - 0.5) * 2.0;
        [1.0, v, v]
    } else {
        let v = scale * 2.0;
        [v, 1.0 - 0.5 * v, 0.0]
    }
}

/// Standalone Subdivided Terrain & Fine River Routing simulation
/// (`terrainsimulation.py`, `terraindisplay.py`, `terrainpresenter.py`).
pub struct TerrainSandbox {
    pub prehistory: PrehistorySimulation,
    pub terrain: SubdividedTerrainGrid,
    pub coarse_rivers: Vec<Vec<usize>>,
    pub river_routes: Vec<Vec<usize>>,
    pub river_fine_faces: BTreeSet<usize>,
}

impl TerrainSandbox {
    pub fn new_earth() -> Self {
        let prehistory = PrehistorySimulation::new_earth(42);
        Self::from_prehistory(prehistory)
    }

    pub fn from_prehistory(prehistory: PrehistorySimulation) -> Self {
        let grid = prehistory.grid();
        let coarse_adj = prehistory.adjacency();
        let tiles = prehistory.tiles();
        let coarse_elevs: Vec<f32> = tiles.iter().map(|t| t.elevation()).collect();
        let coarse_mounts: Vec<f32> = tiles.iter().map(|t| t.mountainosity).collect();

        let terrain = SubdividedTerrainGrid::from_coarse(grid, &coarse_elevs, &coarse_mounts);

        let koeppen_ids = prehistory.tile_koeppen_ids();
        let precip_norm = prehistory.tile_precip_norm();

        let coarse_rivers = trace_coarse_rivers(
            &coarse_elevs,
            precip_norm,
            koeppen_ids,
            coarse_adj,
            TERRAIN_MIN_RIVER_ELEV,
            TERRAIN_MIN_RIVER_PRECIP,
        );
        let river_routes = terrain.route_rivers(grid, &coarse_rivers);
        let river_fine_faces: BTreeSet<usize> =
            river_routes.iter().flatten().copied().collect();

        Self {
            prehistory,
            terrain,
            coarse_rivers,
            river_routes,
            river_fine_faces,
        }
    }

    pub fn load_earth(&mut self) {
        *self = Self::new_earth();
    }

    pub fn load_snapshot_bytes(&mut self, bytes: &[u8]) -> bool {
        let Ok(snap) = tec11_proto::PlanetSnapshot::decode_bytes(bytes) else {
            return false;
        };
        let Some(pre) = PrehistorySimulation::from_snapshot(&snap, 42) else {
            return false;
        };
        *self = Self::from_prehistory(pre);
        true
    }

    pub fn encode_snapshot(&self) -> Vec<u8> {
        self.prehistory.to_snapshot().encode_bytes()
    }

    pub fn grid(&self) -> &SphericalGrid {
        self.prehistory.grid()
    }

    pub fn tiles(&self) -> &[TileGeology] {
        self.prehistory.tiles()
    }

    pub fn is_custom_planet(&self) -> bool {
        self.prehistory.is_custom_planet()
    }

    pub fn display_face_count(&self) -> u32 {
        self.terrain.display_faces.len() as u32
    }

    pub fn fine_face_count(&self) -> u32 {
        self.terrain.terrain_grid.faces.len() as u32
    }

    pub fn coarse_tile_count(&self) -> u32 {
        self.prehistory.tile_count() as u32
    }

    pub fn river_count(&self) -> u32 {
        self.river_routes.len() as u32
    }

    pub fn land_percent(&self) -> u32 {
        self.prehistory.land_percent()
    }

    pub fn build_display_mesh(&self) -> DisplayMesh {
        self.terrain.build_display_mesh()
    }

    pub fn tile_colors_rgb(&self, show_rivers: bool, selected_face: i32) -> Vec<f32> {
        let sel_idx = (selected_face >= 0).then_some(selected_face as usize);
        self.terrain
            .display_faces
            .iter()
            .enumerate()
            .flat_map(|(d_idx, face)| {
                if sel_idx == Some(d_idx) {
                    return [1.0, 0.0, 0.0];
                }
                match face.source {
                    DisplayFaceSource::Fine(fine_idx) => {
                        if show_rivers && self.river_fine_faces.contains(&fine_idx) {
                            [0.0, 0.0, 1.0]
                        } else {
                            elevation_color_rgb(self.terrain.fine_elevations[fine_idx])
                        }
                    }
                    DisplayFaceSource::OceanCoarse(_) => [0.0, 0.0, 1.0],
                }
            })
            .collect()
    }

    pub fn inspect_face(&self, display_idx: usize) -> String {
        let Some(face) = self.terrain.display_faces.get(display_idx) else {
            return String::new();
        };
        let tiles = self.prehistory.tiles();
        match face.source {
            DisplayFaceSource::OceanCoarse(c_idx) => {
                let elev = tiles[c_idx].elevation();
                format!(
                    "Coarse Ocean Tile #{c_idx}\nBathymetry: {elev:.2} km (un-subdivided ocean face)"
                )
            }
            DisplayFaceSource::Fine(fine_idx) => {
                let fine_elev = self.terrain.fine_elevations[fine_idx];
                let on_river = self.river_fine_faces.contains(&fine_idx);
                let parentage_desc = match self.terrain.parentage[fine_idx] {
                    FineParentage::CoarseFace(c) => format!(
                        "Coarse Center Tile #{c} (base elev {:.2} km, mountainosity {:.2})",
                        tiles[c].elevation(),
                        tiles[c].mountainosity_norm()
                    ),
                    FineParentage::CoarseVertex([c0, c1, c2]) => {
                        format!("Coarse Vertex between Tiles #{c0}, #{c1}, #{c2}")
                    }
                    FineParentage::CoarseEdge { primary_coarse, .. } => format!(
                        "Level N+1 Edge Face (primary Coarse Tile #{primary_coarse}, base elev {:.2} km)",
                        tiles[primary_coarse].elevation()
                    ),
                };
                format!(
                    "Subdivided Fine Face #{fine_idx} (Level N+2 = Size {})\n\
                     Fine Elevation (with 7-octave Simplex noise): {fine_elev:.2} km\n\
                     Parentage: {parentage_desc}\n\
                     Routed River: {}",
                    self.prehistory.grid().size + 2,
                    if on_river {
                        "Yes (River Channel)"
                    } else {
                        "No"
                    }
                )
            }
        }
    }
}
