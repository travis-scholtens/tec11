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
        let coarse_thicks: Option<Vec<f32>> = prehistory
            .is_custom_planet()
            .then(|| tiles.iter().map(|t| t.thickness()).collect());

        let terrain = SubdividedTerrainGrid::from_coarse_with_crust(
            grid,
            &coarse_elevs,
            &coarse_mounts,
            coarse_thicks.as_deref(),
        );

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
                let coarse_elevs = self.prehistory.tile_elevations();
                let primary_c = self.terrain.primary_coarse_tile(fine_idx, &coarse_elevs);
                let island_tag = if fine_elev > 0.0 && tiles[primary_c].elevation() <= 0.0 {
                    " [Emergent Volcanic / Atoll Island]"
                } else {
                    ""
                };
                format!(
                    "Subdivided Fine Face #{fine_idx} (Level N+2 = Size {}){island_tag}\n\
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

    pub fn inspect_face_stratigraphy_json(&self, display_idx: usize) -> String {
        let Some(face) = self.terrain.display_faces.get(display_idx) else {
            return "{\"hasColumn\":false}".to_string();
        };
        let tiles = self.prehistory.tiles();
        match face.source {
            DisplayFaceSource::OceanCoarse(c_idx) => tiles
                .get(c_idx)
                .map(|t| t.to_stratigraphy_json(c_idx, None))
                .unwrap_or_else(|| "{\"hasColumn\":false}".to_string()),
            DisplayFaceSource::Fine(fine_idx) => {
                let fine_elev = self.terrain.fine_elevations[fine_idx];
                let coarse_elevs = self.prehistory.tile_elevations();
                let primary_c = self.terrain.primary_coarse_tile(fine_idx, &coarse_elevs);
                tiles
                    .get(primary_c)
                    .map(|t| t.to_stratigraphy_json(primary_c, Some(fine_elev)))
                    .unwrap_or_else(|| "{\"hasColumn\":false}".to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::HistorySimulation;
    use crate::tectonics::TectonicsSimulation;
    use crate::tides_sandbox::FinePlanetBase;

    #[test]
    fn test_subgrid_volcanic_islands_emerge_across_terrain_history_and_fine_planet() {
        // 1. Canned ETOPO2 Earth leaves all submarine coarse centers un-subdivided (`OceanCoarse`)
        let earth_terrain = TerrainSandbox::new_earth();
        let earth_tiles = earth_terrain.tiles();
        for (c, tile) in earth_tiles.iter().enumerate() {
            if tile.elevation() <= 0.0 {
                assert!(
                    !earth_terrain
                        .terrain
                        .coarse_to_fine_children[c]
                        .iter()
                        .any(|&f| matches!(earth_terrain.terrain.parentage[f], FineParentage::CoarseFace(_))),
                    "Canned Earth submarine tile #{c} should not synthesize sub-grid volcanic summits"
                );
            }
        }

        // 2. Step a custom tectonics planet so mantle plumes and intra-oceanic arcs build submarine edifices
        let mut tec = TectonicsSimulation::new(4, 28.0, 2026);
        for _ in 0..45 {
            tec.step();
        }
        let snap = tec.to_snapshot();
        let mut pre = PrehistorySimulation::from_snapshot(&snap, 42).unwrap();
        for _ in 0..6 {
            pre.step();
        }
        let hist = HistorySimulation::from_prehistory(pre, 77);
        let pre_snap = hist.to_snapshot();
        let pre_for_terrain = PrehistorySimulation::from_snapshot(&pre_snap, 42).unwrap();
        let pre_for_fine = PrehistorySimulation::from_snapshot(&pre_snap, 42).unwrap();

        let terrain_sb = TerrainSandbox::from_prehistory(pre_for_terrain);
        let fine_base = FinePlanetBase::from_prehistory(pre_for_fine);

        let coarse_elevs = terrain_sb.prehistory.tile_elevations();
        let coarse_adj = terrain_sb.prehistory.adjacency();
        let mut emergent_submarine_parents = Vec::new();
        for (c, &ce) in coarse_elevs.iter().enumerate() {
            if ce <= 0.0 {
                let summit_face = terrain_sb.terrain.coarse_to_fine_children[c]
                    .iter()
                    .copied()
                    .find(|&f| {
                        matches!(terrain_sb.terrain.parentage[f], FineParentage::CoarseFace(_))
                            && terrain_sb.terrain.fine_elevations[f] > 0.0
                    });
                if let Some(sf) = summit_face {
                    let emergent_children: Vec<usize> = terrain_sb.terrain.coarse_to_fine_children[c]
                        .iter()
                        .copied()
                        .filter(|&f| terrain_sb.terrain.fine_elevations[f] > 0.0)
                        .collect();
                    emergent_submarine_parents.push((c, sf, emergent_children));
                }
            }
        }

        assert!(
            !emergent_submarine_parents.is_empty(),
            "Expected custom tectonic planet to subdivide shallow submarine volcanic seamounts/arcs into emergent fine-grid islands"
        );

        for &(vc, summit_f, ref island_faces) in &emergent_submarine_parents {
            // Exact summit elevation parity between `TerrainSandbox` and `FinePlanetBase` (`Weather` / `Tides`)
            let full_summit = fine_base.fine.coarse_to_fine_children[vc]
                .iter()
                .copied()
                .find(|&ff| matches!(fine_base.fine.parentage[ff], FineParentage::CoarseFace(_)))
                .expect("CoarseFace child in FullSphereFineGrid");
            assert!(fine_base.fine_is_land[full_summit]);
            assert!(
                (fine_base.fine.fine_elevations_km[full_summit]
                    - terrain_sb.terrain.fine_elevations[summit_f])
                    .abs()
                    < 1e-5
            );

            // Open-ocean volcanic tiles (no adjacent coarse continent) must keep a sub-grid island footprint (`1..=4` emergent faces)
            // and have identical emergent face counts across `SubdividedTerrainGrid` and `FullSphereFineGrid`.
            let is_pelagic = coarse_adj[vc].iter().all(|&nb| coarse_elevs[nb] <= 0.0);
            if is_pelagic {
                assert!(
                    island_faces.len() <= 4,
                    "Pelagic submarine volcanic tile #{vc} should produce a sub-grid island (1..=4 emergent faces), got {}",
                    island_faces.len()
                );
                let full_emergent_count = fine_base.fine.coarse_to_fine_children[vc]
                    .iter()
                    .filter(|&&ff| fine_base.fine_is_land[ff])
                    .count();
                assert_eq!(full_emergent_count, island_faces.len());
            }

            // Island Köppen climate comes directly from `ClimateModel::classify_emergent_island`
            let summit_elev = terrain_sb.terrain.fine_elevations[summit_f];
            let expected_fine_clim = fine_base
                .prehistory
                .climate_model()
                .classify_emergent_island(vc, summit_elev, true);
            let expected_hist_clim = hist
                .prehistory()
                .climate_model()
                .classify_emergent_island(vc, summit_elev, true);
            assert_ne!(expected_fine_clim.koeppen as u8, 0);
            assert_ne!(expected_hist_clim.koeppen as u8, 0);
            assert_eq!(
                fine_base.effective_coarse_koeppen[vc],
                expected_fine_clim.koeppen as u8
            );
            assert_eq!(hist.coarse_koeppen()[vc], expected_hist_clim.koeppen as u8);
            assert!(
                hist.continent_label_for_tile(vc, None)
                    .contains("Oceanic Island"),
                "Expected Oceanic Island region label for submarine parent #{vc}, got {}",
                hist.continent_label_for_tile(vc, None)
            );

            // Display face index for fine face `summit_f` equals `summit_f` in `SubdividedTerrainGrid`
            let desc = terrain_sb.inspect_face(summit_f);
            assert!(desc.contains("Emergent Volcanic / Atoll Island"), "{desc}");

            if hist.coarse_koeppen()[vc] != 12 {
                let view = hist.inspect_tile_structured(summit_f).unwrap();
                assert!(
                    !view.flora.is_empty() || !view.fauna.is_empty(),
                    "Expected dispersed island flora/fauna on habitable volcanic island face #{summit_f}"
                );
            }
        }

        // Verify at least one habitable emergent volcanic island is connected to the maritime sea-lane network
        let island_set: BTreeSet<usize> = emergent_submarine_parents
            .iter()
            .map(|&(vc, _, _)| vc)
            .collect();
        let any_island_route = hist
            .potential_travel_routes()
            .iter()
            .any(|r| island_set.contains(&r.from_coarse_tile) || island_set.contains(&r.to_coarse_tile));
        assert!(
            any_island_route,
            "Expected emergent oceanic islands to be linked into potential wind/current maritime routes"
        );
    }
}
