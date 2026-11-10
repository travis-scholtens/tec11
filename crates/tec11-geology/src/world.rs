use tec11_math_grid::{Pcg64Mcg, SphericalGrid};
use tec11_proto::PlanetSnapshotExt;

use crate::history::HistorySimulation;
use crate::population::tile_population_thousands;
use crate::prehistory::PrehistorySimulation;
use crate::racination::Heritage;
use crate::tectonics::TectonicsSimulation;
use crate::tile::TileGeology;

/// Köppen climate full descriptive names (`worldpresenter.py:12-25`).
pub fn koeppen_climate_name(code: u8) -> &'static str {
    match code {
        1 => "Af (rainforest)",
        2 => "Am (monsoon)",
        3 => "Aw (savanna)",
        4 => "BS (steppe)",
        5 => "BW (desert)",
        6 => "Cf (temperate)",
        7 => "Cs (temperate, dry summer)",
        8 => "Cw (temperate, dry winter)",
        9 => "Df (cold)",
        10 => "Ds (cold, dry summer)",
        11 => "Dw (cold, dry winter)",
        12 => "EF (ice cap)",
        13 => "ET (tundra)",
        _ => "Ocean",
    }
}

/// Köppen RGB color `[r, g, b]` in `[0.0, 1.0]` (`koeppencolor.py`).
pub fn koeppen_rgb(code: u8) -> [f32; 3] {
    let (r, g, b) = match code {
        1 => (0, 0, 255),
        2 => (0, 120, 255),
        3 => (70, 170, 250),
        4 => (245, 165, 0),
        5 => (255, 0, 0),
        6 => (200, 255, 80),
        7 => (255, 255, 0),
        8 => (150, 255, 150),
        9 => (0, 255, 255),
        10 => (255, 0, 255),
        11 => (170, 175, 255),
        12 => (100, 100, 100),
        13 => (175, 175, 175),
        _ => (0, 0, 0),
    };
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
}

/// Format integer with comma thousands separators (`worldsimulation.py:56`).
pub fn format_with_commas(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, &b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(b as char);
    }
    out
}

/// Triangular-weighted random integer in `[min_val..=max_val]` stepping by `step` and favoring `mode_val`
/// (`randomspinvalue` in `worldpresenter.py:29-41`).
pub fn random_spin_value(min_val: i32, max_val: i32, step: i32, mode_val: i32, rng: &mut Pcg64Mcg) -> i32 {
    let mut vals = Vec::new();
    let mut v = min_val;
    while v <= max_val {
        vals.push(v);
        v += step.max(1);
    }
    if vals.is_empty() {
        return mode_val;
    }
    let first = vals[0] as f64;
    let last = *vals.last().unwrap() as f64;
    let m = mode_val as f64;
    let mut pw = 0.0;
    let mut ws = Vec::with_capacity(vals.len());
    for &val in &vals {
        let vf = val as f64;
        let dw = if vf < m {
            (vf - first) / (m - first).max(1.0)
        } else {
            (last - vf) / (last - m).max(1.0)
        };
        let w = pw + (1.0 + dw);
        ws.push(w);
        pw = w;
    }
    let f = rng.uniform(0.0, pw);
    for (i, &w) in ws.iter().enumerate() {
        if f < w {
            return vals[i];
        }
    }
    *vals.last().unwrap()
}

/// Geometric distribution `5 * (1 + geom(0.05))` in Myr (`randomtime` in `worldpresenter.py:43-51`).
pub fn random_time_myr(rng: &mut Pcg64Mcg) -> u32 {
    let mut n = 0u32;
    while n < 199 {
        if rng.uniform(0.0, 1.0) < 0.05 {
            break;
        }
        n += 1;
    }
    5 * (1 + n)
}

#[derive(Clone, Copy, Debug)]
pub struct WorldCreateParams {
    pub grid_size: u32,
    pub day_hours: u32,
    pub tilt_deg: u32,
    pub land_pct: u32,
    pub atm_dt_myr: u32,
    pub life_dt_myr: u32,
    pub people_dt_myr: u32,
}

impl Default for WorldCreateParams {
    fn default() -> Self {
        Self {
            grid_size: 5,
            day_hours: 24,
            tilt_deg: 23,
            land_pct: 29,
            atm_dt_myr: 125,
            life_dt_myr: 125,
            people_dt_myr: 125,
        }
    }
}

pub enum WorldStageState {
    Tectonics(TectonicsSimulation),
    Prehistory(PrehistorySimulation),
    History(HistorySimulation),
}

/// Complete World Simulation (`worldsimulation.py`, `worlddisplay.py`, `worldpresenter.py`).
pub struct WorldSimulation {
    params: WorldCreateParams,
    stage: WorldStageState,
    people_ticks_remaining: u32,
    tectonics_ticks: u32,
    prehistory_ticks: u32,
    history_ticks: u32,
    rng: Pcg64Mcg,
}

impl WorldSimulation {
    pub fn new(params: WorldCreateParams, seed: u64) -> Self {
        let mut sim = Self {
            params,
            stage: WorldStageState::Tectonics(TectonicsSimulation::new_with_params(
                params.grid_size,
                params.day_hours as f64,
                params.tilt_deg as f64,
                params.land_pct as f64,
                params.atm_dt_myr,
                params.life_dt_myr,
                seed,
            )),
            people_ticks_remaining: (params.people_dt_myr / 5).max(1),
            tectonics_ticks: 0,
            prehistory_ticks: 0,
            history_ticks: 0,
            rng: Pcg64Mcg::from_seed_u64(seed.wrapping_add(1000)),
        };
        sim.sync_counters();
        sim
    }

    pub fn create(&mut self, mut params: WorldCreateParams, randomize: bool) -> WorldCreateParams {
        if randomize {
            let day_choices = [8u32, 12, 12, 24, 24, 24, 48, 48];
            params.day_hours = day_choices[(self.rng.next_u64() as usize) % day_choices.len()];
            params.tilt_deg = random_spin_value(0, 45, 1, 23, &mut self.rng) as u32;
            params.land_pct = random_spin_value(1, 50, 1, 29, &mut self.rng) as u32;
            params.atm_dt_myr = random_time_myr(&mut self.rng);
            params.life_dt_myr = random_time_myr(&mut self.rng);
            params.people_dt_myr = random_time_myr(&mut self.rng);
        }
        let seed = self.rng.next_u64();
        self.params = params;
        self.stage = WorldStageState::Tectonics(TectonicsSimulation::new_with_params(
            params.grid_size,
            params.day_hours as f64,
            params.tilt_deg as f64,
            params.land_pct as f64,
            params.atm_dt_myr,
            params.life_dt_myr,
            seed,
        ));
        self.people_ticks_remaining = (params.people_dt_myr / 5).max(1);
        self.tectonics_ticks = 0;
        self.prehistory_ticks = 0;
        self.history_ticks = 0;
        params
    }

    /// Advance the world simulation by 1 tick (`worldsimulation.py:77-87`).
    pub fn step(&mut self) {
        let mut transition_to_prehistory = false;
        let mut transition_to_history = false;

        match &mut self.stage {
            WorldStageState::Tectonics(tec) => {
                tec.step();
                self.tectonics_ticks = tec.geoticks();
                if tec.has_life() {
                    if self.people_ticks_remaining == 0 {
                        transition_to_prehistory = true;
                    } else {
                        self.people_ticks_remaining -= 1;
                    }
                }
            }
            WorldStageState::Prehistory(pre) => {
                pre.step();
                self.prehistory_ticks = pre.prehistory_ticks();
                if self.prehistory_ticks >= 32 && pre.active_agricultural_race_count() > 0 {
                    transition_to_history = true;
                }
            }
            WorldStageState::History(hist) => {
                hist.step();
                self.history_ticks = hist.history_ticks();
            }
        }

        if transition_to_prehistory {
            self.enter_prehistory_stage();
        } else if transition_to_history {
            self.enter_history_stage();
        }
    }

    /// Explicitly advance to the next evolutionary stage (`Tectonics -> Prehistory -> History`).
    pub fn advance_stage(&mut self) {
        match &self.stage {
            WorldStageState::Tectonics(_) => self.enter_prehistory_stage(),
            WorldStageState::Prehistory(_) => self.enter_history_stage(),
            WorldStageState::History(_) => {}
        }
    }

    fn enter_prehistory_stage(&mut self) {
        if let WorldStageState::Tectonics(tec) = &self.stage {
            let snap = tec.to_snapshot();
            let sub_seed = self.rng.next_u64();
            if let Some(mut pre) = PrehistorySimulation::from_snapshot(&snap, sub_seed) {
                pre.step();
                self.prehistory_ticks = pre.prehistory_ticks();
                self.stage = WorldStageState::Prehistory(pre);
            }
        }
    }

    fn enter_history_stage(&mut self) {
        if let WorldStageState::Prehistory(pre) = &self.stage {
            let snap = pre.to_snapshot();
            let sub_seed = self.rng.next_u64();
            if let Some(pre_copy) = PrehistorySimulation::from_snapshot(&snap, sub_seed) {
                let hist = HistorySimulation::from_prehistory(pre_copy, sub_seed as u128);
                self.history_ticks = hist.history_ticks();
                self.stage = WorldStageState::History(hist);
            }
        }
    }

    pub fn tectonics_ticks(&self) -> u32 {
        self.tectonics_ticks
    }

    pub fn prehistory_ticks(&self) -> u32 {
        self.prehistory_ticks
    }

    pub fn history_ticks(&self) -> u32 {
        self.history_ticks
    }

    fn sync_counters(&mut self) {
        match &self.stage {
            WorldStageState::Tectonics(tec) => {
                self.tectonics_ticks = tec.geoticks();
            }
            WorldStageState::Prehistory(pre) => {
                self.tectonics_ticks = pre.tectonics_ticks();
                self.prehistory_ticks = pre.prehistory_ticks();
            }
            WorldStageState::History(hist) => {
                self.history_ticks = hist.history_ticks();
            }
        }
    }

    pub fn stage_index(&self) -> u32 {
        match &self.stage {
            WorldStageState::Tectonics(_) => 0,
            WorldStageState::Prehistory(_) => 1,
            WorldStageState::History(_) => 2,
        }
    }

    pub fn stage_name(&self) -> &'static str {
        match &self.stage {
            WorldStageState::Tectonics(tec) => {
                if tec.has_life() {
                    "Tectonics (Biosphere Active)"
                } else if tec.has_atmosphere() {
                    "Tectonics (Atmosphere Active)"
                } else {
                    "Tectonics (Hadean / Early Crust)"
                }
            }
            WorldStageState::Prehistory(_) => "Prehistory (Ice Ages & Migration)",
            WorldStageState::History(_) => "History (States, Trade & Languages)",
        }
    }

    pub fn grid(&self) -> &SphericalGrid {
        match &self.stage {
            WorldStageState::Tectonics(tec) => tec.grid(),
            WorldStageState::Prehistory(pre) => pre.grid(),
            WorldStageState::History(hist) => hist.grid(),
        }
    }

    pub fn tile_count(&self) -> usize {
        match &self.stage {
            WorldStageState::Tectonics(tec) => tec.tile_count(),
            WorldStageState::Prehistory(pre) => pre.tile_count(),
            WorldStageState::History(hist) => hist.tile_count(),
        }
    }

    pub fn stage(&self) -> &WorldStageState {
        &self.stage
    }

    pub fn total_years(&self) -> u64 {
        (self.tectonics_ticks as u64) * 5_000_000
            + (self.prehistory_ticks as u64) * 4_000
            + (self.history_ticks as u64) * 200
    }

    pub fn years_formatted(&self) -> String {
        format_with_commas(self.total_years())
    }

    pub fn total_population_thousands(&self) -> f32 {
        match &self.stage {
            WorldStageState::Tectonics(_) => 0.0,
            WorldStageState::Prehistory(pre) => pre.total_population_thousands(),
            WorldStageState::History(hist) => hist.total_population_thousands(),
        }
    }

    pub fn params(&self) -> WorldCreateParams {
        self.params
    }

    pub fn inspect_tile(&self, tile_idx: usize) -> String {
        match &self.stage {
            WorldStageState::Tectonics(tec) => {
                let Some(tile) = tec.tiles().get(tile_idx) else {
                    return String::new();
                };
                Self::format_geology_and_climate_tree(
                    tile_idx,
                    tile,
                    tec.has_atmosphere(),
                    tec.tile_koeppen_ids()[tile_idx],
                    Some(tec.tile_temp_c()[tile_idx]),
                    Some(tec.tile_precip_norm()[tile_idx]),
                )
            }
            WorldStageState::Prehistory(pre) => {
                let Some(tile) = pre.tiles().get(tile_idx) else {
                    return String::new();
                };
                let mut text = Self::format_geology_and_climate_tree(
                    tile_idx,
                    tile,
                    true,
                    pre.tile_koeppen_ids()[tile_idx],
                    None,
                    Some(pre.tile_precip_norm()[tile_idx]),
                );
                if let Some(&hid) = pre.populated().get(&tile_idx) {
                    let is_ag = pre.agricultural_heritages().contains(&hid);
                    let pop_k = tile_population_thousands(pre.tile_koeppen_ids()[tile_idx], is_ag);
                    text.push_str(&format!(
                        "\nPopulation: {:.1}K ({})\n",
                        pop_k,
                        if is_ag { "Agricultural" } else { "Paleolithic" }
                    ));
                    text.push_str(&Self::format_heritage_ancestry(hid, pre.heritages(), 0));
                }
                text
            }
            WorldStageState::History(hist) => hist.inspect_tile(tile_idx),
        }
    }

    fn format_geology_and_climate_tree(
        tile_idx: usize,
        tile: &TileGeology,
        has_atm: bool,
        koeppen: u8,
        temp_c: Option<f32>,
        precip_norm: Option<f32>,
    ) -> String {
        let mut lines = Vec::new();
        let elev = tile.elevation();
        lines.push(format!(
            "Tile #{tile_idx} | Elev: {elev:+.2} km | Crust: {:.1} km",
            tile.thickness()
        ));
        if has_atm && elev > 0.0 {
            let mut clim_line = format!("Climate: {}", koeppen_climate_name(koeppen));
            if let Some(tc) = temp_c {
                clim_line.push_str(&format!(" | Temp: {tc:+.1} °C"));
            }
            if let Some(p) = precip_norm {
                clim_line.push_str(&format!(" | Precip: {:.0}%", p * 100.0));
            }
            lines.push(clim_line);
        }
        lines.push("Layers (surface -> Moho):".to_string());
        for layer in tile.layers.iter().rev() {
            let r = &layer.rock;
            lines.push(format!(
                "  • {} ({:.2} km, {}, felsity {:.2}, tough {:.2})",
                r.name,
                layer.thickness,
                r.kind.code(),
                r.felsity,
                r.toughness
            ));
        }
        lines.join("\n")
    }

    fn format_heritage_ancestry(
        hid: u32,
        heritages: &std::collections::BTreeMap<u32, Heritage>,
        depth: usize,
    ) -> String {
        if depth > 5 {
            return String::new();
        }
        let indent = "  ".repeat(depth);
        let Some(h) = heritages.get(&hid) else {
            return format!("{indent}• people_{}", hid + 1);
        };
        let mut out = format!("{indent}• {}", h.name);
        for &p in &h.ancestry {
            let child = Self::format_heritage_ancestry(p, heritages, depth + 1);
            if !child.is_empty() {
                out.push('\n');
                out.push_str(&child);
            }
        }
        out
    }

    /// Compute RGB vertex/tile colors for `"color"`, `"climate"`, `"population"`, or `"nations"`
    /// (`worlddisplay.py:7-62`).
    pub fn tile_colors_rgb(&self, aspect: &str, selected_tile: Option<usize>) -> Vec<f32> {
        let mut out = match &self.stage {
            WorldStageState::Tectonics(tec) => {
                let n = tec.tile_count();
                let tiles = tec.tiles();
                let koeppen = tec.tile_koeppen_ids();
                let life = tec.tile_life();
                let has_atm = tec.has_atmosphere();
                let mut buf = Vec::with_capacity(n * 3);
                for t in 0..n {
                    let h = tiles[t].elevation();
                    let rgb = match aspect {
                        "climate" => {
                            if !has_atm {
                                if h > 0.0 {
                                    [0.5, 0.5, 0.5]
                                } else {
                                    [0.0, 0.0, 0.0]
                                }
                            } else if h > 0.0 {
                                koeppen_rgb(koeppen[t])
                            } else {
                                [0.0, 0.0, 0.0]
                            }
                        }
                        _ => Self::natural_color(h, tiles[t].top_felsity(), koeppen[t], life[t], has_atm),
                    };
                    buf.extend_from_slice(&rgb);
                }
                buf
            }
            WorldStageState::Prehistory(pre) => {
                let n = pre.tile_count();
                let tiles = pre.tiles();
                let koeppen = pre.tile_koeppen_ids();
                let life = pre.tile_life();
                let overlay = pre.population_overlay_colors();
                let mut buf = Vec::with_capacity(n * 3);
                for t in 0..n {
                    let h = tiles[t].elevation();
                    let rgb = match aspect {
                        "climate" => {
                            if h > 0.0 {
                                koeppen_rgb(koeppen[t])
                            } else {
                                [0.0, 0.0, 0.0]
                            }
                        }
                        "population" | "nations" => {
                            if overlay[t * 3] >= 0.0 {
                                [overlay[t * 3], overlay[t * 3 + 1], overlay[t * 3 + 2]]
                            } else {
                                Self::natural_color(h, tiles[t].top_felsity(), koeppen[t], life[t], true)
                            }
                        }
                        _ => Self::natural_color(h, tiles[t].top_felsity(), koeppen[t], life[t], true),
                    };
                    buf.extend_from_slice(&rgb);
                }
                buf
            }
            WorldStageState::History(hist) => match aspect {
                "climate" => {
                    let pre_snap = hist.to_snapshot();
                    if let Some(pre) = PrehistorySimulation::from_snapshot(&pre_snap, 42) {
                        let coarse_elevs = pre.tile_elevations();
                        let koeppen = pre.tile_koeppen_ids();
                        let terrain = hist.terrain();
                        let mut buf = Vec::with_capacity(terrain.display_faces.len() * 3);
                        for (_, _, src) in &terrain.display_faces {
                            match *src {
                                tec11_math_grid::DisplayFaceSource::Fine(f_idx) => {
                                    if terrain.fine_elevations[f_idx] > 0.0 {
                                        let c = terrain.primary_coarse_tile(f_idx, &coarse_elevs);
                                        buf.extend_from_slice(&koeppen_rgb(koeppen[c]));
                                    } else {
                                        buf.extend_from_slice(&[0.0, 0.0, 0.0]);
                                    }
                                }
                                tec11_math_grid::DisplayFaceSource::OceanCoarse(_) => {
                                    buf.extend_from_slice(&[0.0, 0.0, 0.0]);
                                }
                            }
                        }
                        buf
                    } else {
                        hist.tile_colors_rgb("nations", true, selected_tile)
                    }
                }
                "population" => hist.tile_colors_rgb("population", true, selected_tile),
                _ => hist.tile_colors_rgb("nations", true, selected_tile),
            },
        };

        if let Some(sel) = selected_tile {
            if sel * 3 + 2 < out.len() {
                out[sel * 3] = 1.0;
                out[sel * 3 + 1] = 0.0;
                out[sel * 3 + 2] = 0.0;
            }
        }
        out
    }

    fn natural_color(h: f32, felsity: f32, koeppen: u8, life: f32, has_atm: bool) -> [f32; 3] {
        if h <= 0.0 {
            if has_atm {
                let d = (-h / 6.0).clamp(0.0, 1.0);
                [0.02 * (1.0 - d), 0.12 * (1.0 - d), 0.35 + 0.35 * (1.0 - d)]
            } else {
                let s = ((7.5 + h) / 7.5).clamp(0.12, 1.0);
                [0.24 * s, 0.27 * s, 0.34 * s]
            }
        } else if has_atm && (koeppen == 12 || koeppen == 13) {
            [1.0, 1.0, 1.0]
        } else {
            let r = (0.5 * 128.0) / 255.0;
            let g = (0.5 * (64.0 + 64.0 * felsity + 255.0 * life)) / 255.0;
            let b = (0.5 * (128.0 * felsity)) / 255.0;
            [r, g, b]
        }
    }

    pub fn encode_snapshot(&self) -> Vec<u8> {
        match &self.stage {
            WorldStageState::Tectonics(tec) => tec.encode_snapshot(),
            WorldStageState::Prehistory(pre) => pre.encode_snapshot(),
            WorldStageState::History(hist) => hist.encode_snapshot(),
        }
    }

    pub fn load_snapshot_bytes(&mut self, bytes: &[u8]) -> bool {
        let Ok(snap) = tec11_proto::PlanetSnapshot::decode_bytes(bytes) else {
            return false;
        };
        match &snap.stage {
            Some(tec11_proto::PlanetStageOneof::Tectonics(_)) => {
                let Some(tec) = TectonicsSimulation::from_snapshot(&snap, 42) else {
                    return false;
                };
                self.tectonics_ticks = tec.geoticks();
                self.prehistory_ticks = 0;
                self.history_ticks = 0;
                self.stage = WorldStageState::Tectonics(tec);
                true
            }
            Some(tec11_proto::PlanetStageOneof::Prehistory(pre_proto)) => {
                let Some(pre) = PrehistorySimulation::from_snapshot(&snap, 42) else {
                    return false;
                };
                self.tectonics_ticks = pre_proto.tectonics_ticks;
                self.prehistory_ticks = pre_proto.prehistory_ticks;
                self.history_ticks = 0;
                self.stage = WorldStageState::Prehistory(pre);
                true
            }
            Some(tec11_proto::PlanetStageOneof::History(hist_proto)) => {
                let mut hist = HistorySimulation::new_earth(42);
                if !hist.load_snapshot_bytes(bytes) {
                    return false;
                }
                if let Some(pre_base) = &hist_proto.prehistory_base {
                    self.tectonics_ticks = pre_base.tectonics_ticks;
                    self.prehistory_ticks = pre_base.prehistory_ticks;
                }
                self.history_ticks = hist_proto.history_ticks;
                self.stage = WorldStageState::History(hist);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_world_simulation_stages_and_snapshot() {
        let params = WorldCreateParams {
            grid_size: 4,
            day_hours: 24,
            tilt_deg: 23,
            land_pct: 29,
            atm_dt_myr: 5,
            life_dt_myr: 5,
            people_dt_myr: 5,
        };
        let mut world = WorldSimulation::new(params, 42);
        assert_eq!(world.stage_index(), 0);

        // Step through atmosphere, life, and people countdown to enter Prehistory
        for _ in 0..5 {
            world.step();
        }
        assert_eq!(world.stage_index(), 1);
        assert!(world.total_years() >= 15_000_000);

        // Advance into History stage
        world.advance_stage();
        assert_eq!(world.stage_index(), 2);
        world.step();
        assert!(world.total_population_thousands() > 0.0);

        let snap = world.encode_snapshot();
        let mut restored = WorldSimulation::new(WorldCreateParams::default(), 99);
        assert!(restored.load_snapshot_bytes(&snap));
        assert_eq!(restored.stage_index(), 2);
    }
}
