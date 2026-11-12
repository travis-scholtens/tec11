use tec11_climate::{ClimateModel, EARTH_SIZE6_ELEVATIONS_M};
use tec11_math_grid::SphericalGrid;
use tec11_proto::PlanetSnapshotExt;

use crate::rock::{
    deposit_sedimentary, DepositClimate, ErodedMaterialBatch, Rock, RockKind, RockLayer,
};
use crate::tectonics::TectonicsSimulation;
use crate::tile::TileGeology;

/// Standalone erosion & sedimentary deposition sandbox (`erosionsimulation.py`).
pub struct ErosionSandbox {
    grid: SphericalGrid,
    adjacency: Vec<Vec<usize>>,
    tiles: Vec<TileGeology>,
    climate_model: ClimateModel,
    tile_lost: Vec<f32>,
    tile_gained: Vec<f32>,
    steps: u32,
    custom_geoticks: Option<u32>,
    custom_land_percent: Option<u32>,
}

impl ErosionSandbox {
    /// Initialize with the canned Earth (`ETOPO2`) topography and run 1 erosion step
    /// (`erosionsimulation.py:62-71`).
    pub fn new_earth() -> Self {
        let grid = SphericalGrid::with_size(6);
        let adjacency = grid.adjacency();
        let n = grid.faces.len();
        let mut tiles = Vec::with_capacity(n);

        for &elev_m in EARTH_SIZE6_ELEVATIONS_M.iter() {
            let elev_km = (elev_m as f32) / 900.0;
            if elev_km > 0.0 {
                let mut t = TileGeology {
                    bottom: -1.0,
                    mountainosity: (elev_km * 2.5).clamp(0.0, 20.0),
                    subduction: 0.0,
                    intrusion: None,
                    impact_scar: 0.0,
                    layers: vec![RockLayer {
                        rock: Rock {
                            kind: RockKind::Sedimentary,
                            name: "sandstone",
                            felsity: 0.5,
                            toughness: 0.5,
                            clasticity: Some(1.0),
                            calcity: None,
                            bogginess: None,
                        },
                        thickness: elev_km + 1.0,
                    }],
                };
                t.limit();
                tiles.push(t);
            } else {
                let bathy_km = ((elev_m as f32) / 1000.0).clamp(-7.0, -0.2);
                tiles.push(TileGeology {
                    bottom: bathy_km - 5.0,
                    mountainosity: 0.0,
                    subduction: 0.0,
                    intrusion: None,
                    impact_scar: 0.0,
                    layers: vec![RockLayer {
                        rock: Rock::extrusive(0.5),
                        thickness: 5.0,
                    }],
                });
            }
        }

        let climate_model = ClimateModel::new_for_tectonics(grid.clone(), 3, 1.0, 23.0);
        let mut sim = Self {
            grid,
            adjacency,
            tiles,
            climate_model,
            tile_lost: vec![0.0; n],
            tile_gained: vec![0.0; n],
            steps: 0,
            custom_geoticks: None,
            custom_land_percent: None,
        };
        sim.erode();
        sim
    }

    pub fn load_earth(&mut self) {
        *self = Self::new_earth();
    }

    /// Load a `.tec11` binary Protocol Buffer `PlanetSnapshot` into the erosion sandbox
    /// and run 1 erosion step (`erosionsimulation.py:73-76`).
    pub fn load_snapshot_bytes(&mut self, bytes: &[u8]) -> bool {
        let Ok(snap) = tec11_proto::PlanetSnapshot::decode_bytes(bytes) else {
            return false;
        };
        let Some(tec_sim) = TectonicsSimulation::from_snapshot(&snap, 42) else {
            return false;
        };
        let grid = tec_sim.grid().clone();
        let adjacency = grid.adjacency();
        let n = grid.faces.len();
        let tiles = tec_sim.tiles().to_vec();
        let geoticks = tec_sim.geoticks();
        let land_pct = tec_sim.land_percent();
        let climate_model = ClimateModel::new_for_tectonics(grid.clone(), 3, 1.0, 23.0);

        self.grid = grid;
        self.adjacency = adjacency;
        self.tiles = tiles;
        self.climate_model = climate_model;
        self.tile_lost = vec![0.0; n];
        self.tile_gained = vec![0.0; n];
        self.steps = 0;
        self.custom_geoticks = Some(geoticks);
        self.custom_land_percent = Some(land_pct);
        self.erode();
        true
    }

    /// Run one climate + erosion + sedimentary deposition pass (`erosionsimulation.py:43-56`).
    pub fn erode(&mut self) {
        let n = self.tiles.len();
        let land_elevs: Vec<f32> = self
            .tiles
            .iter()
            .map(|t| t.elevation().max(0.0))
            .collect();
        self.climate_model.update_for_tectonics(&land_elevs, true);

        let mut tile_life = vec![0.0f32; n];
        let mut tile_temp_norm = vec![0.5f32; n];
        let mut tile_temp_c = vec![15.0f32; n];
        let mut tile_precip_norm = vec![0.5f32; n];

        for (v, c) in self.climate_model.classifications().iter().enumerate() {
            tile_life[v] = if c.life { 1.0 } else { 0.0 };
            let tn = c.mean_temperature as f32;
            tile_temp_norm[v] = tn;
            tile_temp_c[v] = tn * 75.0 - 20.0;
            tile_precip_norm[v] = c.mean_precipitation as f32;
        }

        // Build downhill erosion graph (`erodemethod.py:21-38`).
        let mut outgoing_dests: Vec<Vec<(usize, f32)>> = vec![Vec::new(); n];
        let mut max_erosion_cap: Vec<f32> = vec![0.0; n];

        for v in 0..n {
            let elev_v = self.tiles[v].elevation();
            if elev_v > 0.0 {
                let d = (elev_v / 10.0)
                    * tile_precip_norm[v]
                    * (1.0 - tile_temp_norm[v]).max(0.0);
                if d > 0.0 {
                    let mut min_nb_elev = elev_v;
                    for &u in &self.adjacency[v] {
                        let nb_elev = self.tiles[u].elevation().max(0.0);
                        if nb_elev < elev_v {
                            min_nb_elev = min_nb_elev.min(nb_elev);
                            outgoing_dests[v].push((u, d));
                        }
                    }
                    if min_nb_elev < elev_v {
                        let base_level = min_nb_elev.max(0.02);
                        max_erosion_cap[v] = (0.5 * (elev_v - base_level)).max(0.0);
                    }
                }
            }
        }

        self.tile_lost.fill(0.0);
        self.tile_gained.fill(0.0);

        let mut incoming_batches: Vec<Vec<ErodedMaterialBatch>> = vec![Vec::new(); n];
        let erode_dt = 1.0f32;

        for v in 0..n {
            if outgoing_dests[v].is_empty() {
                continue;
            }
            let total_degree: f32 = outgoing_dests[v].iter().map(|&(_, d)| d).sum();
            // `erosiondisplay.py:64`: `t = sum([d.degree for d in tile.eroding.destinations])`
            self.tile_lost[v] = total_degree;

            if total_degree <= 1e-7 || max_erosion_cap[v] <= 1e-6 {
                continue;
            }
            let avg_degree = total_degree / (outgoing_dests[v].len() as f32);
            let (total_eroded, stripped_layers) =
                self.tiles[v].erode_column(avg_degree, erode_dt, max_erosion_cap[v]);
            if total_eroded > 1e-6 && !stripped_layers.is_empty() {
                for &(dest_u, deg_u) in &outgoing_dests[v] {
                    let share = total_eroded * (deg_u / total_degree);
                    incoming_batches[dest_u].push(ErodedMaterialBatch {
                        amount: share,
                        total: total_eroded,
                        substance: stripped_layers.clone(),
                    });
                }
            }
        }

        for t in 0..n {
            if incoming_batches[t].is_empty() {
                continue;
            }
            let total_gained: f32 = incoming_batches[t].iter().map(|m| m.amount).sum();
            // `erosiondisplay.py:69`: `t = sum([m.amount for m in tile.eroding.materials])`
            self.tile_gained[t] = total_gained;

            let dep_climate = DepositClimate {
                temperature_c: tile_temp_c[t],
                precipitation_norm: tile_precip_norm[t],
                life: tile_life[t],
            };
            let is_sea = self.tiles[t].elevation() <= 0.0;
            let layer = deposit_sedimentary(&incoming_batches[t], true, is_sea, Some(dep_climate));
            self.tiles[t].deposit(layer);
        }

        for tile in &mut self.tiles {
            tile.limit();
        }
        self.steps += 1;
    }

    pub fn grid(&self) -> &SphericalGrid {
        &self.grid
    }

    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    pub fn steps(&self) -> u32 {
        self.steps
    }

    pub fn is_custom_planet(&self) -> bool {
        self.custom_geoticks.is_some()
    }

    pub fn loaded_geoticks(&self) -> u32 {
        self.custom_geoticks.unwrap_or(0)
    }

    pub fn loaded_land_percent(&self) -> u32 {
        self.custom_land_percent.unwrap_or(29)
    }

    pub fn tile_elevations(&self) -> Vec<f32> {
        self.tiles.iter().map(|t| t.elevation()).collect()
    }

    pub fn tile_lost(&self) -> &[f32] {
        &self.tile_lost
    }

    pub fn tile_gained(&self) -> &[f32] {
        &self.tile_gained
    }

    pub fn inspect_tile(&self, idx: usize) -> String {
        if idx >= self.tiles.len() {
            return "Invalid tile".to_string();
        }
        let t = &self.tiles[idx];
        let mut out = format!(
            "Tile #{idx}: elev {:.2} km | lost {:.3} | gained {:.3} km\n",
            t.elevation(),
            self.tile_lost[idx],
            self.tile_gained[idx],
        );
        for (i, l) in t.layers.iter().enumerate().rev() {
            out.push_str(&format!(
                "  [{i}] {} ({:?}): {:.2} km (toughness {:.2})\n",
                l.rock.name, l.rock.kind, l.thickness, l.rock.toughness
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_erosion_sandbox_earth() {
        let mut e = ErosionSandbox::new_earth();
        assert_eq!(e.tile_count(), 7292);
        assert_eq!(e.steps(), 1);
        let max_lost = e.tile_lost().iter().copied().fold(0.0f32, f32::max);
        let max_gained = e.tile_gained().iter().copied().fold(0.0f32, f32::max);
        assert!(max_lost > 0.01, "Expected nonzero erosion degree on Earth mountains");
        assert!(max_gained > 0.001, "Expected nonzero sediment deposition on Earth");

        e.erode();
        assert_eq!(e.steps(), 2);
    }

    #[test]
    fn test_tectonics_erosion_rivers_load_all_snapshot_stages() {
        use crate::history::HistorySimulation;
        use crate::prehistory::PrehistorySimulation;
        use crate::rivers::RiversSandbox;

        let mut tec = TectonicsSimulation::new(4, 29.0, 42);
        tec.step();
        let tec_bytes = tec.encode_snapshot();

        let pre = PrehistorySimulation::from_snapshot(&tec.to_snapshot(), 42)
            .expect("PrehistorySimulation should decode tectonics snapshot");
        let pre_bytes = pre.encode_snapshot();

        let mut hist = HistorySimulation::new_earth(42);
        assert!(hist.load_snapshot_bytes(&pre_bytes));
        let hist_bytes = hist.encode_snapshot();

        for (label, bytes) in [
            ("tectonics", &tec_bytes),
            ("prehistory", &pre_bytes),
            ("history", &hist_bytes),
        ] {
            let loaded_tec = TectonicsSimulation::decode_snapshot(bytes, 99)
                .unwrap_or_else(|| panic!("TectonicsSimulation should load {label} snapshot"));
            assert_eq!(loaded_tec.grid().size, 4);
            assert_eq!(loaded_tec.geoticks(), 1);

            let mut erosion = ErosionSandbox::new_earth();
            assert!(
                erosion.load_snapshot_bytes(bytes),
                "ErosionSandbox should load {label} snapshot"
            );
            assert!(erosion.is_custom_planet());
            assert_eq!(erosion.loaded_geoticks(), 1);
            assert_eq!(erosion.tile_count(), 812);

            let mut rivers = RiversSandbox::new_earth(3.0, 0.5);
            assert!(
                rivers.load_snapshot_bytes(bytes),
                "RiversSandbox should load {label} snapshot"
            );
            assert!(rivers.is_custom_planet());
            assert_eq!(rivers.loaded_geoticks(), 1);
            assert_eq!(rivers.tile_count(), 812);
        }
    }
}
