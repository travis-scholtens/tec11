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
    pub moon_count: u32,
    pub moon_mass: f32,
    pub moon_dist_radii: u32,
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
            moon_count: 1,
            moon_mass: 1.0,
            moon_dist_radii: 60,
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
            stage: WorldStageState::Tectonics(TectonicsSimulation::new_with_full_params(
                params.grid_size,
                params.day_hours as f64,
                params.tilt_deg as f64,
                params.land_pct as f64,
                params.atm_dt_myr,
                params.life_dt_myr,
                params.people_dt_myr,
                params.moon_count,
                params.moon_mass as f64,
                params.moon_dist_radii as f64,
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
            params.tilt_deg = random_spin_value(0, 45, 1, 23, &mut self.rng) as u32;
            params.land_pct = random_spin_value(1, 50, 1, 29, &mut self.rng) as u32;
            params.atm_dt_myr = random_time_myr(&mut self.rng);
            params.life_dt_myr = random_time_myr(&mut self.rng);
            params.people_dt_myr = random_time_myr(&mut self.rng);

            // Randomize number of moons (0 or more, weighted toward 1 Earth-like moon or 2 harmonic moons):
            let moon_count_choices = [0u32, 0, 1, 1, 1, 1, 2, 2, 3, 4];
            params.moon_count =
                moon_count_choices[(self.rng.next_u64() as usize) % moon_count_choices.len()];
            if params.moon_count == 0 {
                params.moon_mass = 0.0;
                params.moon_dist_radii = 60;
                // Moonless worlds experience no lunar rotational braking, favoring faster rotation (6..=28h, mode 12h):
                params.day_hours = random_spin_value(6, 28, 1, 12, &mut self.rng) as u32;
            } else {
                // Reasonably sized moon mass in Lunar masses (0.2x..=2.5x, mode 1.0x):
                let mass_tenths = random_spin_value(2, 25, 1, 10, &mut self.rng);
                params.moon_mass = (mass_tenths as f32) / 10.0;
                // Target moon distance at Prehistory/History start in planetary radii (30..=90 Rp, mode 60 Rp):
                params.moon_dist_radii = random_spin_value(30, 90, 5, 60, &mut self.rng) as u32;
                // Continuous target day length (6..=48h) weighted by the moon's angular momentum braking:
                let h_ang =
                    TectonicsSimulation::harmonic_angular_momentum_multiplier(params.moon_count);
                let braking_factor = (params.moon_mass as f64)
                    * h_ang
                    * ((params.moon_dist_radii as f64) / 60.0).sqrt();
                let mode_day_h = (12.0 + 12.0 * braking_factor).round().clamp(8.0, 44.0) as i32;
                params.day_hours = random_spin_value(6, 48, 1, mode_day_h, &mut self.rng) as u32;
            }
        }
        let seed = self.rng.next_u64();
        self.params = params;
        self.stage = WorldStageState::Tectonics(TectonicsSimulation::new_with_full_params(
            params.grid_size,
            params.day_hours as f64,
            params.tilt_deg as f64,
            params.land_pct as f64,
            params.atm_dt_myr,
            params.life_dt_myr,
            params.people_dt_myr,
            params.moon_count,
            params.moon_mass as f64,
            params.moon_dist_radii as f64,
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

    pub fn current_day_hours(&self) -> f32 {
        match &self.stage {
            WorldStageState::Tectonics(tec) => tec.current_day_hours() as f32,
            _ => self.params.day_hours as f32,
        }
    }

    pub fn current_tilt_deg(&self) -> f32 {
        match &self.stage {
            WorldStageState::Tectonics(tec) => tec.current_tilt_deg() as f32,
            _ => self.params.tilt_deg as f32,
        }
    }

    pub fn current_moon_dist_radii(&self) -> f32 {
        match &self.stage {
            WorldStageState::Tectonics(tec) => tec.current_moon_dist_radii() as f32,
            _ => {
                if self.params.moon_count == 0 {
                    0.0
                } else {
                    self.params.moon_dist_radii as f32
                }
            }
        }
    }

    pub fn current_tidal_strength(&self) -> f32 {
        match &self.stage {
            WorldStageState::Tectonics(tec) => tec.current_tidal_strength() as f32,
            _ => {
                if self.params.moon_count == 0 || self.params.moon_mass <= 0.0 {
                    0.15
                } else {
                    let a = (self.params.moon_dist_radii.max(12)) as f64;
                    let h_tide =
                        TectonicsSimulation::harmonic_tidal_multiplier(self.params.moon_count);
                    let lunar = (self.params.moon_mass as f64) * (60.0 / a).powi(3) * h_tide;
                    (0.15 + 0.85 * lunar) as f32
                }
            }
        }
    }

    pub fn impact_count(&self) -> u32 {
        match &self.stage {
            WorldStageState::Tectonics(tec) => tec.impact_count(),
            WorldStageState::Prehistory(pre) => pre
                .tiles()
                .iter()
                .filter(|t| t.impact_signature() >= 0.65)
                .count() as u32,
            WorldStageState::History(hist) => {
                let snap = hist.to_snapshot();
                if let Some(pre) = PrehistorySimulation::from_snapshot(&snap, 42) {
                    pre.tiles()
                        .iter()
                        .filter(|t| t.impact_signature() >= 0.65)
                        .count() as u32
                } else {
                    0
                }
            }
        }
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
        let sig = tile.impact_signature();
        if sig >= 0.65 {
            lines.push("Meteor Scar: Impact Crater Basin (Shocked Hornfels / Melt)".to_string());
        } else if sig >= 0.35 {
            lines.push("Meteor Scar: Impact Ejecta Rim (Breccia Conglomerate)".to_string());
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

    /// Compute RGB vertex/tile colors for `"color"`, `"climate"`, `"population"`, `"nations"`, or `"meteors"`
    /// (`worlddisplay.py:7-62`).
    pub fn tile_colors_rgb(&self, aspect: &str, selected_tile: Option<usize>) -> Vec<f32> {
        let mut out = match &self.stage {
            WorldStageState::Tectonics(tec) => {
                let n = tec.tile_count();
                let tiles = tec.tiles();
                let koeppen = tec.tile_koeppen_ids();
                let life = tec.tile_life();
                let scars = tec.tile_impact_scars();
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
                        "meteors" => {
                            let base = Self::natural_color(
                                h,
                                tiles[t].top_felsity(),
                                koeppen[t],
                                life[t],
                                has_atm,
                            );
                            let scar = scars[t].max(tiles[t].impact_signature());
                            Self::blend_impact_scar_color(base, scar)
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
                        "meteors" => {
                            let base = Self::natural_color(
                                h,
                                tiles[t].top_felsity(),
                                koeppen[t],
                                life[t],
                                true,
                            );
                            Self::blend_impact_scar_color(base, tiles[t].impact_signature())
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
                "meteors" => {
                    let pre_snap = hist.to_snapshot();
                    if let Some(pre) = PrehistorySimulation::from_snapshot(&pre_snap, 42) {
                        let coarse_elevs = pre.tile_elevations();
                        let tiles = pre.tiles();
                        let koeppen = pre.tile_koeppen_ids();
                        let life = pre.tile_life();
                        let terrain = hist.terrain();
                        let mut buf = Vec::with_capacity(terrain.display_faces.len() * 3);
                        for (_, _, src) in &terrain.display_faces {
                            match *src {
                                tec11_math_grid::DisplayFaceSource::Fine(f_idx) => {
                                    let c = terrain.primary_coarse_tile(f_idx, &coarse_elevs);
                                    let h = terrain.fine_elevations[f_idx];
                                    let base = Self::natural_color(
                                        h,
                                        tiles[c].top_felsity(),
                                        koeppen[c],
                                        life[c],
                                        true,
                                    );
                                    let rgb = Self::blend_impact_scar_color(
                                        base,
                                        tiles[c].impact_signature(),
                                    );
                                    buf.extend_from_slice(&rgb);
                                }
                                tec11_math_grid::DisplayFaceSource::OceanCoarse(c_idx) => {
                                    let h = tiles[c_idx].elevation();
                                    let base = Self::natural_color(
                                        h,
                                        tiles[c_idx].top_felsity(),
                                        koeppen[c_idx],
                                        life[c_idx],
                                        true,
                                    );
                                    let rgb = Self::blend_impact_scar_color(
                                        base,
                                        tiles[c_idx].impact_signature(),
                                    );
                                    buf.extend_from_slice(&rgb);
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

    fn blend_impact_scar_color(base: [f32; 3], scar: f32) -> [f32; 3] {
        if scar <= 0.05 {
            return [base[0] * 0.72, base[1] * 0.72, base[2] * 0.72];
        }
        if scar >= 0.60 {
            // Crater basin bullseye (shocked hornfels / impact melt): glowing crimson-orange
            let w = ((scar - 0.50) / 0.50).clamp(0.65, 0.96);
            [
                base[0] * (1.0 - w) + 0.98 * w,
                base[1] * (1.0 - w) + 0.24 * w,
                base[2] * (1.0 - w) + 0.10 * w,
            ]
        } else if scar >= 0.32 {
            // Raised ejecta breccia rim (conglomerate): bright amber-gold
            let w = ((scar - 0.25) / 0.35).clamp(0.55, 0.90);
            [
                base[0] * (1.0 - w) + 0.96 * w,
                base[1] * (1.0 - w) + 0.76 * w,
                base[2] * (1.0 - w) + 0.10 * w,
            ]
        } else {
            // Outer shock ring / weathered crater halo: warm copper
            let w = (scar * 2.5).clamp(0.25, 0.75);
            [
                base[0] * (1.0 - w) + 0.85 * w,
                base[1] * (1.0 - w) + 0.45 * w,
                base[2] * (1.0 - w) + 0.20 * w,
            ]
        }
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
            moon_count: 1,
            moon_mass: 1.0,
            moon_dist_radii: 60,
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
