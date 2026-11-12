use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::PI;

use tec11_climate::ClimateModel;
use tec11_math_grid::{
    cross, dot, length, normal, rotate, GreatCircleArc, Pcg64Mcg, PointTree, SphericalGrid, Vec3,
};
use tec11_proto::{MetamorphicFaciesExt, PlanetSnapshotExt};

use crate::movement::{center_tile, farthest_tile, move_group, split_group};
use crate::pangea::PangeaWorld;
use crate::rock::{
    deposit_sedimentary, metamorphic_contact, metamorphic_regional, DepositClimate,
    ErodedMaterialBatch, Rock, RockKind, RockLayer,
};
use crate::tile::{TileGeology, TileSourceColumn};

/// A moving continental / lithospheric group (`Group` in `tile.py:17-20`).
#[derive(Clone, Debug)]
pub struct LithosphericGroup {
    pub tiles: Vec<usize>,
    pub velocity: Vec3,
}

/// An active mid-ocean spreading ridge segment on the unit sphere (`Section 5.4` of the design doc).
#[derive(Clone, Debug)]
pub struct SpreadingRidgeArc {
    pub start: Vec3,
    pub end: Vec3,
    pub velocity: Vec3,
    pub age_ticks: u32,
}

#[derive(Clone, Debug)]
struct TileMovement {
    sources: Vec<TileSourceColumn>,
    speed: f64,
}

/// Complete full-crust tectonic simulation (`PlanetSimulation` in `planetsimulation.py`
/// + Section 5.4 of the `tec11` design doc).
pub struct TectonicsSimulation {
    grid: SphericalGrid,
    adjacency: Vec<Vec<usize>>,
    tree: PointTree,
    radius_km: f64,
    dp: f64,
    build_rate: f64,
    erode_rate: f64,
    splitnum: f64,
    atmosphere_ticks_remaining: u32,
    life_ticks_remaining: u32,
    total_target_ticks: u32,
    geoticks: u32,
    target_day_hours: f64,
    base_tilt_deg: f64,
    moon_count: u32,
    moon_mass: f64,
    target_moon_dist_radii: f64,
    tiles: Vec<TileGeology>,
    groups: Vec<LithosphericGroup>,
    ridges: Vec<SpreadingRidgeArc>,
    climate_model: ClimateModel,
    tile_koeppen: Vec<u8>,
    tile_life: Vec<f32>,
    tile_temp_norm: Vec<f32>,
    tile_temp_c: Vec<f32>,
    tile_precip_norm: Vec<f32>,
    tile_impact_scars: Vec<f32>,
    meteor_freq: f64,
    impact_count: u32,
    impact_winter_ticks: u32,
    sterilization_delays: u32,
    last_impact_tile: Option<usize>,
    last_impact_diameter_km: f32,
    rng: Pcg64Mcg,
}

impl TectonicsSimulation {
    /// Create a new tectonic simulation initialized with a random Pangaea supercontinent
    /// and surrounding ocean floor (`TectonicsPresenter` defaults: `grid_size = 6`, `land_pct = 29.0`,
    /// `dt = 5 Myr`, `atmdt = 125 Myr`, `lifedt = 125 Myr`, and 1 Earth-like Moon).
    pub fn new(grid_size: u32, land_percent: f64, seed: u64) -> Self {
        Self::new_with_params(grid_size, 24.0, 23.0, land_percent, 125, 125, seed)
    }

    pub fn new_with_params(
        grid_size: u32,
        day_hours: f64,
        tilt_deg: f64,
        land_percent: f64,
        atm_dt_myr: u32,
        life_dt_myr: u32,
        seed: u64,
    ) -> Self {
        Self::new_with_full_params(
            grid_size,
            day_hours,
            tilt_deg,
            land_percent,
            atm_dt_myr,
            life_dt_myr,
            125,
            1,
            1.0,
            60.0,
            seed,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_full_params(
        grid_size: u32,
        day_hours: f64,
        tilt_deg: f64,
        land_percent: f64,
        atm_dt_myr: u32,
        life_dt_myr: u32,
        people_dt_myr: u32,
        moon_count: u32,
        moon_mass: f64,
        moon_dist_radii: f64,
        seed: u64,
    ) -> Self {
        let mut rng = Pcg64Mcg::from_seed_u64(seed);
        let pangea = PangeaWorld::generate(grid_size, land_percent, &mut rng);
        let radius_km = pangea.radius_km;
        let grid = pangea.grid;
        let adjacency = grid.adjacency();
        let n = grid.faces.len();

        let indexed_pts: Vec<(Vec3, usize)> = grid
            .faces
            .iter()
            .enumerate()
            .map(|(i, (c, _))| (*c, i))
            .collect();
        let tree = PointTree::new(&indexed_pts);

        let target_day_hours = day_hours.max(1.0);
        let base_tilt_deg = tilt_deg.clamp(0.0, 85.0);
        let moon_mass = if moon_count == 0 {
            0.0
        } else {
            moon_mass.max(0.0)
        };
        let target_moon_dist_radii = moon_dist_radii.clamp(15.0, 160.0);

        let atmosphere_ticks_remaining = (atm_dt_myr / 5).max(1);
        let life_ticks_remaining = (life_dt_myr / 5).max(1);
        let people_ticks = (people_dt_myr / 5).max(1);
        let total_target_ticks =
            (atmosphere_ticks_remaining + life_ticks_remaining + people_ticks).max(1);

        let spin = 24.0 / target_day_hours;
        let cells = tec11_climate::nominal_cells_for_spin(spin);
        let dt = 5.0;
        let dp = (100.0 / radius_km) * dt;
        let build_rate = dt / 5.0;
        let erode_rate = dt;

        // Scale split threshold proportionally to planet surface area so a continent covering
        // ~9.7% of the globe (~50M km^2 on Earth, r = 6400 km) has a 50/50 chance of rifting
        // on any planet size (`planetsimulation.py:86`).
        let earth_area_km2 = 4.0 * PI * 6400.0 * 6400.0;
        let splitnum = (25.0e6 / earth_area_km2) * (n as f64);

        let groups = vec![LithosphericGroup {
            tiles: pangea.continent_tiles,
            velocity: [0.0, 0.0, 0.0],
        }];

        let ridges = pangea
            .ridge_arcs
            .into_iter()
            .map(|arc| SpreadingRidgeArc {
                start: arc.start,
                end: arc.end,
                velocity: [0.0, 0.0, 0.0],
                age_ticks: 8,
            })
            .collect();

        let climate_model =
            ClimateModel::new_for_tectonics(grid.clone(), cells, spin, base_tilt_deg);

        let mut sim = Self {
            grid,
            adjacency,
            tree,
            radius_km,
            dp,
            build_rate,
            erode_rate,
            splitnum,
            atmosphere_ticks_remaining,
            life_ticks_remaining,
            total_target_ticks,
            geoticks: 0,
            target_day_hours,
            base_tilt_deg,
            moon_count,
            moon_mass,
            target_moon_dist_radii,
            tiles: pangea.tiles,
            groups,
            ridges,
            climate_model,
            tile_koeppen: vec![0; n],
            tile_life: vec![0.0; n],
            tile_temp_norm: vec![0.5; n],
            tile_temp_c: vec![15.0; n],
            tile_precip_norm: vec![0.5; n],
            tile_impact_scars: vec![0.0; n],
            meteor_freq: 1.0,
            impact_count: 0,
            impact_winter_ticks: 0,
            sterilization_delays: 0,
            last_impact_tile: None,
            last_impact_diameter_km: 0.0,
            rng,
        };
        sim.climate_model
            .set_spin_and_tilt_for_tectonics(sim.current_spin(), sim.current_tilt_deg());
        sim
    }

    /// Tidal amplitude multiplier for `moon_count` moons in a 2:1 Laplace mean-motion
    /// orbital period resonance chain (`P_k = 2^(k-1) P_1 => a_k^3 = 4^(k-1) a_1^3`).
    /// Because tidal force scales as `M_k / a_k^3`, each successive harmonic moon out
    /// contributes `1 / 4^(k-1)` of the innermost moon's tide (`1 + 1/4 + 1/16 + ...`).
    pub fn harmonic_tidal_multiplier(moon_count: u32) -> f64 {
        let mut sum = 0.0;
        for k in 0..moon_count.min(8) {
            sum += 4.0f64.powi(-(k as i32));
        }
        sum
    }

    /// Orbital angular momentum multiplier for `moon_count` moons in a 2:1 Laplace
    /// resonance chain (`L_k ~ M_k sqrt(a_k) = M_1 sqrt(a_1) * 2^((k-1)/3)`).
    pub fn harmonic_angular_momentum_multiplier(moon_count: u32) -> f64 {
        let mut sum = 0.0;
        for k in 0..moon_count.min(8) {
            sum += 2.0f64.powf((k as f64) / 3.0);
        }
        sum
    }

    pub fn moon_count(&self) -> u32 {
        self.moon_count
    }

    pub fn moon_mass(&self) -> f64 {
        self.moon_mass
    }

    pub fn target_moon_dist_radii(&self) -> f64 {
        self.target_moon_dist_radii
    }

    /// Initial Hadean (`t = 0`) orbital distance of the innermost moon in planetary radii `R_p`,
    /// backward-calibrated via the `a^(13/2)` tidal torque recession law so that the moon
    /// reaches `target_moon_dist_radii` when Prehistory/History begins (`t = total_target_ticks`).
    pub fn initial_moon_dist_radii(&self) -> f64 {
        if self.moon_count == 0 || self.moon_mass <= 0.0 {
            return 0.0;
        }
        let h_tide = Self::harmonic_tidal_multiplier(self.moon_count);
        let factor = (1.0 + 450.0 * self.moon_mass * h_tide).powf(2.0 / 13.0);
        (self.target_moon_dist_radii / factor).clamp(14.0, self.target_moon_dist_radii)
    }

    /// Current orbital distance of the innermost moon in planetary radii `R_p` at `geoticks`.
    pub fn current_moon_dist_radii(&self) -> f64 {
        if self.moon_count == 0 || self.moon_mass <= 0.0 {
            return 0.0;
        }
        let a0 = self.initial_moon_dist_radii();
        let a_tgt = self.target_moon_dist_radii;
        let tau = ((self.geoticks as f64) / (self.total_target_ticks.max(1) as f64)).clamp(0.0, 1.25);
        let a0_p = a0.powf(6.5);
        let at_p = a_tgt.powf(6.5);
        (a0_p + (at_p - a0_p) * tau).max(1.0).powf(1.0 / 6.5)
    }

    /// Current tidal amplitude relative to modern Earth (`1.0` for 1 Lunar-mass moon at `60 R_p`).
    pub fn current_tidal_strength(&self) -> f64 {
        if self.moon_count == 0 || self.moon_mass <= 0.0 {
            0.15 // Solar tide only
        } else {
            let a = self.current_moon_dist_radii().max(12.0);
            let h_tide = Self::harmonic_tidal_multiplier(self.moon_count);
            let lunar_tide = self.moon_mass * (60.0 / a).powi(3) * h_tide;
            0.15 + 0.85 * lunar_tide
        }
    }

    /// Current planetary rotation rate `spin = 24.0 / current_day_hours`, starting faster
    /// in the Hadean and braking via angular momentum transfer to the receding moon(s)
    /// so that `current_day_hours == target_day_hours` when Prehistory/History begins.
    pub fn current_spin(&self) -> f64 {
        let target_spin = 24.0 / self.target_day_hours.max(1.0);
        if self.moon_count == 0 || self.moon_mass <= 0.0 {
            target_spin
        } else {
            let a_cur = self.current_moon_dist_radii();
            let a_tgt = self.target_moon_dist_radii;
            let h_ang = Self::harmonic_angular_momentum_multiplier(self.moon_count);
            let delta_spin = 3.10
                * self.moon_mass
                * h_ang
                * ((a_tgt / 60.0).sqrt() - (a_cur / 60.0).sqrt()).max(-0.2);
            (target_spin + delta_spin).clamp(0.1, 6.0)
        }
    }

    /// Current length of day in hours at `geoticks`.
    pub fn current_day_hours(&self) -> f64 {
        24.0 / self.current_spin()
    }

    /// Current axial tilt (obliquity) in degrees at `geoticks` (Laskar, Joutel & Robutel 1993):
    /// stabilized within `+-1.2 deg` when lunar tidal torque is strong, or wandering
    /// quasi-periodically by up to `+-14 deg` on moonless/tiny-moon worlds.
    pub fn current_tilt_deg(&self) -> f64 {
        let h_tide = Self::harmonic_tidal_multiplier(self.moon_count);
        let s_obliq = if self.moon_count == 0 || self.moon_mass <= 0.0 {
            0.0
        } else {
            self.moon_mass * (60.0 / self.target_moon_dist_radii.max(15.0)).powi(3) * h_tide
        };
        let t = self.geoticks as f64;
        let wave = 0.65 * (0.19 * t).sin() + 0.35 * (0.31 * t + 1.1).cos();
        let amp = if s_obliq >= 0.25 {
            (1.2 / s_obliq.sqrt()).clamp(0.2, 2.4)
        } else {
            14.0 * (1.0 - 3.3 * s_obliq).clamp(0.2, 1.0)
        };
        (self.base_tilt_deg + amp * wave).clamp(0.0, 85.0)
    }

    pub fn grid(&self) -> &SphericalGrid {
        &self.grid
    }

    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    pub fn geoticks(&self) -> u32 {
        self.geoticks
    }

    pub fn continent_count(&self) -> usize {
        self.groups.len()
    }

    pub fn land_percent(&self) -> u32 {
        let land_tiles = self.tiles.iter().filter(|t| t.elevation() > 0.0).count();
        let pct = 100.0 * (land_tiles as f64) / (self.tiles.len().max(1) as f64);
        (pct + 0.5) as u32
    }

    pub fn has_atmosphere(&self) -> bool {
        self.atmosphere_ticks_remaining == 0
    }

    pub fn has_life(&self) -> bool {
        self.life_ticks_remaining == 0
    }

    pub fn tiles(&self) -> &[TileGeology] {
        &self.tiles
    }

    pub fn groups(&self) -> &[LithosphericGroup] {
        &self.groups
    }

    pub fn tile_koeppen_ids(&self) -> &[u8] {
        &self.tile_koeppen
    }

    pub fn tile_life(&self) -> &[f32] {
        &self.tile_life
    }

    pub fn tile_temp_c(&self) -> &[f32] {
        &self.tile_temp_c
    }

    pub fn tile_precip_norm(&self) -> &[f32] {
        &self.tile_precip_norm
    }

    pub fn tile_impact_scars(&self) -> &[f32] {
        &self.tile_impact_scars
    }

    pub fn impact_count(&self) -> u32 {
        self.impact_count
    }

    pub fn meteor_freq(&self) -> f64 {
        self.meteor_freq
    }

    pub fn set_meteor_freq(&mut self, freq: f64) {
        self.meteor_freq = freq.clamp(0.0, 10.0);
    }

    pub fn impact_winter_ticks(&self) -> u32 {
        self.impact_winter_ticks
    }

    pub fn last_impact_tile(&self) -> Option<usize> {
        self.last_impact_tile
    }

    pub fn last_impact_diameter_km(&self) -> f32 {
        self.last_impact_diameter_km
    }

    /// Apply a bolide/asteroid impact centered at `center` with final crater diameter `crater_diameter_km`.
    /// - Excavates upper strata at `center`, shock-metamorphoses the crater floor (`hornfels`),
    ///   and extrudes an impact-melt `basalt` sheet for large basins (`>= 240 km`).
    /// - Deposits an impact-breccia `conglomerate` ejecta blanket (`clasticity = 0.96`) onto
    ///   the surrounding ring of neighbor tiles (`adjacency[center]`), raising the crater rim.
    /// - If `crater_diameter_km >= 320 km` during early pre-biotic epochs (`life_ticks_remaining in 1..=10`),
    ///   ocean-vaporizing sterilization delays `life_ticks_remaining` by up to 2 ticks.
    /// - If `has_life()` and `crater_diameter_km >= 150 km`, triggers a 1-tick impact winter
    ///   (`impact_winter_ticks = 1`) that suppresses biogenic carbonate/peat deposition.
    pub fn apply_bolide_impact(&mut self, center: usize, crater_diameter_km: f64) {
        if self.tiles.is_empty() {
            return;
        }
        let center = center.min(self.tiles.len() - 1);
        let d_km = crater_diameter_km.clamp(50.0, 1200.0);

        self.impact_count = self.impact_count.saturating_add(1);
        self.last_impact_tile = Some(center);
        self.last_impact_diameter_km = d_km as f32;
        self.tile_impact_scars[center] = 1.0;

        // 1. Excavate and shock-metamorphose the crater bullseye (`center`):
        let pre_elev = self.tiles[center].elevation();
        let excavate_km = (0.35 * (d_km / 180.0)).clamp(0.20, 1.25) as f32;
        if pre_elev > 0.0 {
            let cap = (self.tiles[center].thickness() - 1.2).max(0.0).min(excavate_km);
            if cap > 0.05 {
                let _ = self.tiles[center].erode_column(cap, 1.0, cap);
            }
            // Shock-metamorphosed hornfels crater floor:
            self.tiles[center].deposit(RockLayer {
                rock: Rock {
                    kind: RockKind::Metamorphic,
                    name: "hornfels",
                    felsity: 0.55,
                    toughness: 0.88,
                    clasticity: None,
                    calcity: None,
                    bogginess: None,
                },
                thickness: 0.18,
            });
            // Large multi-ring basins (>= 240 km, e.g. Sudbury/Vredefort) extrude an impact-melt basalt sheet:
            if d_km >= 240.0 {
                self.tiles[center].deposit(RockLayer {
                    rock: Rock::extrusive(0.48),
                    thickness: 0.22,
                });
            }
        // Depress the crater basin floor relative to surrounding crust while keeping inland tiles
            // above 0.08 km so interior continents never develop artificial ocean pinholes:
            let depressed_elev = (pre_elev - 0.55 * excavate_km).clamp(0.08, pre_elev.max(0.08));
            self.tiles[center].bottom = depressed_elev - self.tiles[center].thickness();
            self.tiles[center].mountainosity *= 0.35;
            self.tiles[center].limit();
        } else {
            // Marine impact: shock-metamorphose seafloor basement and deposit tsunami breccia:
            let sea_elev = pre_elev.clamp(-6.5, -0.15);
            self.tiles[center].deposit(RockLayer {
                rock: Rock {
                    kind: RockKind::Metamorphic,
                    name: "hornfels",
                    felsity: 0.50,
                    toughness: 0.85,
                    clasticity: None,
                    calcity: None,
                    bogginess: None,
                },
                thickness: 0.15,
            });
            self.tiles[center].bottom = sea_elev - self.tiles[center].thickness();
            self.tiles[center].limit();
        }
        self.tiles[center].impact_scar = 1.0;
        self.tile_impact_scars[center] = 1.0;

        // 2. Deposit volume-conserving coarse impact-breccia `conglomerate` ejecta blanket on the surrounding rim:
        let ring1 = self.adjacency[center].clone();
        let rim_thick = (excavate_km / (ring1.len().max(1) as f32)).clamp(0.04, 0.16);
        for &nb in &ring1 {
            let nb_elev = self.tiles[nb].elevation();
            if nb_elev > 0.0 {
                self.tiles[nb].deposit(RockLayer {
                    rock: Rock {
                        kind: RockKind::Sedimentary,
                        name: "conglomerate",
                        felsity: 0.58,
                        toughness: 0.55,
                        clasticity: Some(TileGeology::IMPACT_BRECCIA_CLASTICITY),
                        calcity: None,
                        bogginess: None,
                    },
                    thickness: rim_thick,
                });
                self.tiles[nb].isostasize(-0.85 * rim_thick);
                if self.tiles[nb].elevation() > 5.4 {
                    self.tiles[nb].bottom = 5.4 - self.tiles[nb].thickness();
                }
                self.tiles[nb].mountainosity = (self.tiles[nb].mountainosity + 0.6).min(20.0);
                self.tiles[nb].limit();
            }
            self.tiles[nb].impact_scar = self.tiles[nb].impact_scar.max(0.55);
            self.tile_impact_scars[nb] = self.tiles[nb].impact_signature();
        }

        // Multi-ring Hadean/Archean mega-basins (>= 380 km) leave an outer ejecta/shock ring:
        if d_km >= 380.0 {
            for &nb in &ring1 {
                let ring2 = self.adjacency[nb].clone();
                for &nb2 in &ring2 {
                    if nb2 != center {
                        self.tiles[nb2].impact_scar = self.tiles[nb2].impact_scar.max(0.30);
                        self.tile_impact_scars[nb2] = self.tiles[nb2].impact_signature();
                    }
                }
            }
        }

        // 3. Biosphere & Early Life Sterilization effects:
        if !self.has_life()
            && self.life_ticks_remaining > 0
            && self.life_ticks_remaining <= 10
            && d_km >= 320.0
            && self.sterilization_delays < 2
        {
            // Ocean-boiling Hadean/Archean giant impact delays nascent surface biosphere emergence:
            self.life_ticks_remaining += 2;
            self.sterilization_delays += 1;
        } else if self.has_life() && d_km >= 150.0 {
            // Chicxulub-class Phanerozoic extinction strike triggers a 1-tick impact winter:
            self.impact_winter_ticks = 1;
            self.tile_life[center] = 0.0;
            for &nb in &ring1 {
                self.tile_life[nb] = 0.0;
            }
        }
    }

    /// Advance the tectonic simulation by one 5 Myr timestep (`PlanetSimulation.update`
    /// in `planetsimulation.py:211-381` + full ocean-floor bathymetry from Section 5.4).
    pub fn step(&mut self) {
        let n = self.tiles.len();

        // 0. Clear previous tick's transient subduction & intrusion markers, and weather impact scars
        //    on the crustal columns themselves (so scars move with continental drift).
        for tile in &mut self.tiles {
            tile.clear_temp();
            if tile.elevation() > 0.0 {
                tile.impact_scar *= 0.96;
                if tile.impact_scar < 0.03 {
                    tile.impact_scar = 0.0;
                }
            } else {
                // Oceanic impacts are rapidly buried by abyssal sediment & recycled by seafloor spreading:
                tile.impact_scar *= 0.45;
                if tile.impact_scar < 0.05 {
                    tile.impact_scar = 0.0;
                }
            }
        }

        // 1. Determine tile movements for each continental group.
        let mut old_continental = vec![false; n];
        for g in &self.groups {
            for &t in &g.tiles {
                old_continental[t] = true;
            }
        }

        let mut new_movements: BTreeMap<usize, Vec<TileMovement>> = BTreeMap::new();
        let mut overlapping: Vec<Vec<usize>> = vec![Vec::new(); n];

        for i in 0..self.groups.len() {
            let speed = length(self.groups[i].velocity);
            let (moved_map, next_v) = move_group(
                &self.grid,
                &self.adjacency,
                &self.tree,
                &self.groups[i].tiles,
                self.groups[i].velocity,
            );

            let new_group_tiles: Vec<usize> = moved_map.keys().copied().collect();
            self.groups[i] = LithosphericGroup {
                tiles: new_group_tiles,
                velocity: next_v,
            };

            for (dest, src_indices) in moved_map {
                let sources: Vec<TileSourceColumn> = src_indices
                    .into_iter()
                    .map(|s| {
                        let st = &self.tiles[s];
                        TileSourceColumn {
                            bottom: st.bottom,
                            mountainosity: st.mountainosity,
                            impact_scar: st.impact_scar,
                            layers: st.layers.clone(),
                        }
                    })
                    .collect();
                new_movements
                    .entry(dest)
                    .or_default()
                    .push(TileMovement { sources, speed });
                overlapping[dest].push(i);
            }
        }

        // Close any 1-to-3 tile inter-plate pinholes completely enclosed by `new_movements`
        // (and any 1-tile `>= deg - 1` pinhole) without inflating `group.tiles` at rifts.
        let mut visited_non_land = vec![false; n];
        for start in 0..n {
            if visited_non_land[start] || new_movements.contains_key(&start) {
                continue;
            }
            let mut comp = Vec::new();
            let mut q = std::collections::VecDeque::new();
            visited_non_land[start] = true;
            q.push_back(start);
            while let Some(u) = q.pop_front() {
                comp.push(u);
                for &nb in &self.adjacency[u] {
                    if !visited_non_land[nb] && !new_movements.contains_key(&nb) {
                        visited_non_land[nb] = true;
                        q.push_back(nb);
                    }
                }
            }
            if comp.len() <= 3 {
                // Fill this tiny 1..=3 tile enclosed pocket from its surrounding `new_movements` neighbors
                for _pass in 0..3 {
                    for &h in &comp {
                        if new_movements.contains_key(&h) {
                            continue;
                        }
                        let nb_in_cont: Vec<usize> = self.adjacency[h]
                            .iter()
                            .copied()
                            .filter(|&nb| new_movements.contains_key(&nb))
                            .collect();
                        if !nb_in_cont.is_empty() {
                            let mut group_counts: BTreeMap<usize, usize> = BTreeMap::new();
                            for &nb in &nb_in_cont {
                                for &g_idx in &overlapping[nb] {
                                    *group_counts.entry(g_idx).or_insert(0) += 1;
                                }
                            }
                            if let Some((&best_g, _)) = group_counts.iter().max_by_key(|&(_, c)| c) {
                                let mut sources = Vec::new();
                                for &nb in &nb_in_cont {
                                    if let Some(ms) = new_movements.get(&nb) {
                                        if let Some(first_m) = ms.first() {
                                            if let Some(first_src) = first_m.sources.first() {
                                                sources.push(first_src.clone());
                                            }
                                        }
                                    }
                                }
                                if !sources.is_empty() {
                                    let speed = length(self.groups[best_g].velocity);
                                    new_movements
                                        .entry(h)
                                        .or_default()
                                        .push(TileMovement { sources, speed });
                                    overlapping[h].push(best_g);
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut in_new_continent = vec![false; n];
        let mut tile_vel = vec![[0.0, 0.0, 0.0]; n];
        for &dest in new_movements.keys() {
            in_new_continent[dest] = true;
            let g_indices = &overlapping[dest];
            if !g_indices.is_empty() {
                let mut sum_v = [0.0, 0.0, 0.0];
                for &g_idx in g_indices {
                    let gv = self.groups[g_idx].velocity;
                    sum_v[0] += gv[0];
                    sum_v[1] += gv[1];
                    sum_v[2] += gv[2];
                }
                let inv = 1.0 / (g_indices.len() as f64);
                tile_vel[dest] = [sum_v[0] * inv, sum_v[1] * inv, sum_v[2] * inv];
            }
        }

        // 2. Apply tile movements, directional leading-edge subduction uplift, and accretionary
        //    sediment scraping.
        let mut collisions: BTreeMap<(usize, usize), usize> = BTreeMap::new();

        for (&dest, movements) in &new_movements {
            let is_leading_edge = !old_continental[dest];
            let avg_speed: f64 =
                movements.iter().map(|m| m.speed).sum::<f64>() / (movements.len() as f64);
            let speed_norm = (avg_speed / self.dp.max(1e-6)).clamp(0.0, 1.6);
            let v_vec = tile_vel[dest];
            let v_len = length(v_vec);
            let p_dest = self.grid.faces[dest].0;
            let mut cos_theta_max = 0.0f64;
            let mut has_ocean_nb = false;
            if v_len > 1e-9 {
                let v_hat = [v_vec[0] / v_len, v_vec[1] / v_len, v_vec[2] / v_len];
                for &nb in &self.adjacency[dest] {
                    if !in_new_continent[nb] {
                        has_ocean_nb = true;
                        let p_nb = self.grid.faces[nb].0;
                        let d_hat = normal([
                            p_nb[0] - p_dest[0],
                            p_nb[1] - p_dest[1],
                            p_nb[2] - p_dest[2],
                        ]);
                        cos_theta_max = cos_theta_max.max(dot(v_hat, d_hat));
                    }
                }
            }
            let raw_build = if has_ocean_nb && is_leading_edge && cos_theta_max > 0.20 {
                (1.60 * self.build_rate * speed_norm * cos_theta_max) as f32
            } else if has_ocean_nb && cos_theta_max > 0.45 {
                (0.45 * self.build_rate * speed_norm * cos_theta_max) as f32
            } else {
                0.0
            };

            // Scrape off any marine sedimentary layers from the overridden ocean tile
            // before replacing it with the advancing continental column (Accretionary Wedge).
            let mut scraped_sediments: Vec<RockLayer> = Vec::new();
            if is_leading_edge {
                for l in &self.tiles[dest].layers {
                    if l.rock.kind == RockKind::Sedimentary {
                        scraped_sediments.push(l.clone());
                    }
                }
            }

            let group_sources: Vec<Vec<TileSourceColumn>> =
                movements.iter().map(|m| m.sources.clone()).collect();
            self.tiles[dest].merge_sources(&group_sources);

            for sed in scraped_sediments {
                self.tiles[dest].deposit(sed);
            }

            if raw_build > 0.0 {
                let cur_h = self.tiles[dest].elevation().max(0.0);
                let max_sub_h = if is_leading_edge { 6.8 } else { 6.2 };
                let taper = ((max_sub_h - cur_h) / 5.8).clamp(0.0, 1.0);
                let build_amount = raw_build * taper;
                if build_amount > 0.01 {
                    self.tiles[dest].build(build_amount, Rock::seafloor());
                }
            }

            // Record pairwise collisions between overlapping groups at `dest`.
            let ov = &overlapping[dest];
            for a_idx in 0..ov.len() {
                for b_idx in (a_idx + 1)..ov.len() {
                    let g1 = ov[a_idx].min(ov[b_idx]);
                    let g2 = ov[a_idx].max(ov[b_idx]);
                    *collisions.entry((g1, g2)).or_insert(0) += 1;
                }
            }
        }

        // 3. Mid-ocean spreading ridge advection, retirement, and dynamic ocean floor bathymetry:
        //    - Advect active spreading ridges by their parent drift velocity and retire overridden ones.
        //    - Compute Parsons & Sclater thermal subsidence (-2.5 km ridge axis -> -5.5 km abyssal plain
        //      over 2,600 km) from active spreading ridges.
        //    - Evaluate coastal ocean tiles directionally: active convergent trenches (cos_theta > 0.35)
        //      flex downward to -5.5..-6.8 km, while passive/trailing shelves sit at -1.15 km.
        for ridge in &mut self.ridges {
            let speed = length(ridge.velocity);
            if speed > 1e-9 {
                let mid = normal([
                    ridge.start[0] + ridge.end[0],
                    ridge.start[1] + ridge.end[1],
                    ridge.start[2] + ridge.end[2],
                ]);
                let axis = normal(cross(mid, ridge.velocity));
                if length(axis) > 1e-9 {
                    ridge.start = rotate(ridge.start, axis, speed);
                    ridge.end = rotate(ridge.end, axis, speed);
                }
            }
            ridge.age_ticks = ridge.age_ticks.saturating_add(1);
        }

        self.ridges.retain(|r| {
            if r.age_ticks <= 6 {
                return true;
            }
            let mid = normal([
                r.start[0] + r.end[0],
                r.start[1] + r.end[1],
                r.start[2] + r.end[2],
            ]);
            let s_in = in_new_continent[self.tree.nearest(r.start)];
            let m_in = in_new_continent[self.tree.nearest(mid)];
            let e_in = in_new_continent[self.tree.nearest(r.end)];
            !(s_in && m_in && e_in)
        });

        let active_arcs: Vec<(GreatCircleArc, f64)> = self
            .ridges
            .iter()
            .map(|r| {
                let maturity = ((r.age_ticks as f64) / 6.0).clamp(0.25, 1.0);
                (GreatCircleArc::new(r.start, r.end), maturity)
            })
            .collect();

        for v in 0..n {
            if in_new_continent[v] {
                continue;
            }

            let p_v = self.grid.faces[v].0;
            let mut z_ridge = -5.5f64;
            for &(ref arc, maturity) in &active_arcs {
                let d_km = arc.distance_to_point(p_v) * self.radius_km;
                let u = (d_km / 2600.0).clamp(0.0, 1.0);
                let z_arc = -5.5 + maturity * 3.0 * (1.0 - u.sqrt());
                if z_arc > z_ridge {
                    z_ridge = z_arc;
                }
            }

            let mut has_cont_nb = false;
            let mut cos_theta_max = -1.0f64;
            let mut max_sub = 0.0f64;
            for &nb in &self.adjacency[v] {
                if in_new_continent[nb] {
                    has_cont_nb = true;
                    let v_nb = tile_vel[nb];
                    let v_len = length(v_nb);
                    if v_len > 1e-9 {
                        let v_hat = [v_nb[0] / v_len, v_nb[1] / v_len, v_nb[2] / v_len];
                        let p_nb = self.grid.faces[nb].0;
                        let d_hat = normal([
                            p_v[0] - p_nb[0],
                            p_v[1] - p_nb[1],
                            p_v[2] - p_nb[2],
                        ]);
                        let ct = dot(v_hat, d_hat);
                        if ct > cos_theta_max {
                            cos_theta_max = ct;
                        }
                        if ct > 0.25 {
                            max_sub = max_sub.max(self.tiles[nb].subduction as f64);
                        }
                    } else {
                        cos_theta_max = cos_theta_max.max(0.0);
                    }
                }
            }

            let target_elev = if !has_cont_nb {
                z_ridge as f32
            } else if cos_theta_max > 0.35 && max_sub > 0.0 {
                // Active convergent subduction trench ahead of an advancing continental margin:
                let trench_deflection = 1.35 * cos_theta_max * max_sub.clamp(0.25, 1.2);
                (z_ridge - trench_deflection).clamp(-6.8, -4.8) as f32
            } else if cos_theta_max <= 0.15 {
                // Passive / trailing continental shelf:
                -1.15f32
            } else {
                // Oblique coastal margin: smooth transition between passive shelf and open ocean / ridge depth:
                let w = ((cos_theta_max - 0.15) / 0.20).clamp(0.0, 1.0);
                ((-1.15 * (1.0 - w)) + z_ridge * w) as f32
            };

            if old_continental[v]
                || self.tiles[v].elevation() > 0.0
                || (self.tiles[v].impact_scar == 0.0
                    && self.tiles[v].stratigraphic_impact_signature() > 0.0)
            {
                self.tiles[v] = TileGeology::empty_ocean_with_depth(Rock::seafloor(), target_elev);
            } else {
                self.tiles[v].bottom = target_elev.min(-0.15) - self.tiles[v].thickness();
            }
        }

        // 4. Record each continent's pre-erosion total above-sea height (`planetsimulation.py:273`).
        let pre_erosion_heights: Vec<f32> = self
            .groups
            .iter()
            .map(|g| {
                g.tiles
                    .iter()
                    .map(|&t| self.tiles[t].elevation().max(0.0))
                    .sum()
            })
            .collect();

        // 5. Simulate coupled climate and erosion if the atmosphere has formed (`planetsimulation.py:275-315`).
        if self.has_atmosphere() {
            let has_life = self.has_life() && self.impact_winter_ticks == 0;
            self.refresh_climate_fields();
            if self.impact_winter_ticks > 0 {
                for l in &mut self.tile_life {
                    *l *= 0.25;
                }
                self.impact_winter_ticks -= 1;
            }

            // Build downhill erosion graph (`erodemethod.py:21-38` with physical local base-level bounds):
            // Each land tile (elevation > 0) erodes only into strictly lower neighbors (`h_u < h_v`),
            // weighted by the elevation drop, and capped at half the relief above its lowest neighbor
            // (and above sea level 0.05 km) so rivers never carve pits below their own outlet.
            let mut outgoing_dests: Vec<Vec<(usize, f32)>> = vec![Vec::new(); n];
            let mut incoming_sources: Vec<Vec<usize>> = vec![Vec::new(); n];
            let mut max_erosion_cap: Vec<f32> = vec![0.0; n];
            let tidal_factor = self.current_tidal_strength().sqrt().clamp(0.2, 3.5) as f32;

            for v in 0..n {
                let elev_v = self.tiles[v].elevation();
                if elev_v > 0.05 {
                    let base_climate = self.tile_precip_norm[v]
                        * (1.0 - self.tile_temp_norm[v]).max(0.0);
                    let high_relief_weathering = if elev_v > 2.2 {
                        0.08 * ((elev_v - 2.2) / 6.0).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let climate_factor = (base_climate + high_relief_weathering).max(0.0);
                    let has_ocean_nb = self.adjacency[v]
                        .iter()
                        .any(|&u| self.tiles[u].elevation() <= 0.0);
                    if climate_factor > 0.0 || has_ocean_nb {
                        let mut min_nb_elev = elev_v;
                        for &u in &self.adjacency[v] {
                            let raw_nb_elev = self.tiles[u].elevation();
                            let nb_elev = raw_nb_elev.max(0.0);
                            if nb_elev < elev_v {
                                min_nb_elev = min_nb_elev.min(nb_elev);
                                let drop = elev_v - nb_elev;
                                let tidal_scour = if raw_nb_elev <= 0.0 {
                                    0.008 * tidal_factor
                                } else {
                                    0.0
                                };
                                let d = (0.34 * (elev_v + drop) / 10.0) * climate_factor
                                    + tidal_scour;
                                if d > 0.0 {
                                    outgoing_dests[v].push((u, d));
                                    incoming_sources[u].push(v);
                                }
                            }
                        }
                        if min_nb_elev < elev_v {
                            let base_level = min_nb_elev.max(0.05);
                            max_erosion_cap[v] = (0.38 * (elev_v - base_level)).max(0.0);
                        }
                    }
                }
            }

            let mut incoming_batches: Vec<Vec<ErodedMaterialBatch>> = vec![Vec::new(); n];
            let erode_dt = self.erode_rate as f32;

            for v in 0..n {
                if outgoing_dests[v].is_empty() || max_erosion_cap[v] <= 1e-6 {
                    continue;
                }
                let total_degree: f32 = outgoing_dests[v].iter().map(|&(_, d)| d).sum();
                if total_degree <= 1e-7 {
                    continue;
                }
                let avg_degree = total_degree / (outgoing_dests[v].len().max(1) as f32);
                let (total_eroded, stripped_layers) =
                    self.tiles[v].erode_column(avg_degree, erode_dt, max_erosion_cap[v]);
                if total_eroded > 1e-6 && !stripped_layers.is_empty() {
                    // Local Airy isostatic root rebound: removing crustal mass from an eroding
                    // mountain unloads its root locally (`60%` rebound of bottom, `40%` net lowering
                    // of surface elevation), exhuming deep metamorphic/plutonic rocks without
                    // jacking up un-eroded lowland tiles across the rest of the continent.
                    self.tiles[v].isostasize(0.60 * total_eroded);

                    // Distribute eroded material across lower neighbors proportional to slope:
                    // - On land neighbors, 35% is deposited locally in terrestrial basins while 65%
                    //   is exported to the sea as dissolved/suspended fluvial load.
                    // - On coastal ocean neighbors, the full share enters the coastal sea.
                    for &(dest_u, deg_u) in &outgoing_dests[v] {
                        let share = total_eroded * (deg_u / total_degree);
                        let retention = if !overlapping[dest_u].is_empty() {
                            0.35
                        } else {
                            1.0
                        };
                        let amount_u = share * retention;
                        incoming_batches[dest_u].push(ErodedMaterialBatch {
                            amount: amount_u,
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
                let koeppen_group = match self.tile_koeppen[t] {
                    1..=3 => 'A',
                    4..=5 => 'B',
                    6..=8 => 'C',
                    9..=11 => 'D',
                    12..=13 => 'E',
                    _ => ' ',
                };
                let clim_summary = DepositClimate {
                    temperature_c: self.tile_temp_c[t],
                    precipitation_norm: self.tile_precip_norm[t],
                    koeppen_group,
                };

                if !overlapping[t].is_empty() {
                    let dep = deposit_sedimentary(
                        &incoming_batches[t],
                        has_life,
                        false,
                        Some(clim_summary),
                    );
                    let dep_t = dep.thickness;
                    self.tiles[t].deposit(dep);
                    // Local Airy basin subsidence under the weight of newly deposited sediment:
                    self.tiles[t].isostasize(-0.75 * dep_t);
                } else {
                    let total_amount: f32 = incoming_batches[t].iter().map(|b| b.amount).sum();
                    let adj_continent_count = self.adjacency[t]
                        .iter()
                        .filter(|&&nb| !overlapping[nb].is_empty())
                        .count();
                    if total_amount > 1.5 && adj_continent_count >= 3 {
                        // Prograde continent into coastal sea when deposition exceeds 1.5 km (`planetsimulation.py:301-310`)
                        let dep = deposit_sedimentary(
                            &incoming_batches[t],
                            has_life,
                            true,
                            Some(clim_summary),
                        );
                        self.tiles[t].deposit(dep);
                        if self.tiles[t].elevation() <= 0.05 {
                            self.tiles[t].bottom = (total_amount - 1.4).max(0.1) - self.tiles[t].thickness();
                        }

                        let mut group_counts: BTreeMap<usize, usize> = BTreeMap::new();
                        for &src_v in &incoming_sources[t] {
                            for &g_idx in &overlapping[src_v] {
                                *group_counts.entry(g_idx).or_insert(0) += 1;
                            }
                        }
                        if let Some((&best_g, _)) = group_counts.iter().max_by_key(|&(_, c)| c) {
                            if !self.groups[best_g].tiles.contains(&t) {
                                self.groups[best_g].tiles.push(t);
                            }
                            overlapping[t].push(best_g);
                        }
                    }
                }
            }

            if self.life_ticks_remaining > 0 {
                self.life_ticks_remaining -= 1;
            }
        } else {
            self.atmosphere_ticks_remaining -= 1;
        }

        // 6. Craton buoyancy safeguard & post-orogenic denudation (`planetsimulation.py:318-321`):
        //    - Local Airy root rebound and basin subsidence are applied per-tile during erosion above.
        //    - Ancient inactive mountain belts (`!is_active_orogen` and not adjacent to an active
        //      orogenic front) slowly relax/denude toward lowland craton elevations (`2.0 km`).
        for (i, group) in self.groups.iter().enumerate() {
            if group.tiles.is_empty() {
                continue;
            }
            let n_tiles = group.tiles.len() as f32;
            let pre_h = pre_erosion_heights.get(i).copied().unwrap_or(0.0);
            let post_h: f32 = group
                .tiles
                .iter()
                .map(|&t| self.tiles[t].elevation().max(0.0))
                .sum();
            let pre_mean = pre_h / n_tiles;
            let post_mean = post_h / n_tiles;
            let eroded_loss = (pre_mean - post_mean).max(0.0);
            let dh = if post_mean < 0.45 {
                (0.45 - post_mean).min(eroded_loss.max(0.05))
            } else {
                0.0
            };

            for &t in &group.tiles {
                if dh > 0.0 {
                    self.tiles[t].isostasize(dh);
                }
                let is_active_orogen =
                    self.tiles[t].subduction > 0.0 || overlapping[t].len() > 1;
                if !is_active_orogen {
                    let elev = self.tiles[t].elevation();
                    if elev > 1.3 {
                        let relax = 0.048 * (elev - 1.3);
                        self.tiles[t].isostasize(-relax);
                    }
                    self.tiles[t].mountainosity = (self.tiles[t].mountainosity * 0.92).max(0.0);
                }
                // Keep continental tiles emergent above sea level
                if self.tiles[t].elevation() < 0.05 {
                    self.tiles[t].bottom = 0.05 - self.tiles[t].thickness();
                }
            }
        }

        // 7. Random plutonic intrusions and contact metamorphism (`planetsimulation.py:325-329`).
        for t in 0..n {
            if self.tiles[t].subduction > 0.0 && self.rng.next_f64() < 0.1 {
                let silica = self.rng.gauss(0.85, 0.15).clamp(0.0, 1.0) as f32;
                self.tiles[t].intrude(Rock::intrusive(silica));
                let transformed =
                    metamorphic_contact(&self.tiles[t].layers, self.tiles[t].intrusion);
                self.tiles[t].transform(transformed);
            }
        }

        // 8. Regional metamorphism (`planetsimulation.py:333-335`).
        for t in 0..n {
            let sub = self.tiles[t].subduction > 0.0;
            let transformed = metamorphic_regional(&self.tiles[t].layers, sub);
            self.tiles[t].transform(transformed);
        }

        // 9. Merge overlapping continent shapes (`planetsimulation.py:342-370`).
        let mut merge_clusters: Vec<BTreeSet<usize>> = Vec::new();
        for (&(g1, g2), &count) in &collisions {
            let min_len = self.groups[g1].tiles.len().min(self.groups[g2].tiles.len());
            if count > min_len / 10 {
                let mut found = None;
                for (c_idx, cluster) in merge_clusters.iter_mut().enumerate() {
                    if cluster.contains(&g1) || cluster.contains(&g2) {
                        cluster.insert(g1);
                        cluster.insert(g2);
                        found = Some(c_idx);
                        break;
                    }
                }
                if found.is_none() {
                    let mut set = BTreeSet::new();
                    set.insert(g1);
                    set.insert(g2);
                    merge_clusters.push(set);
                }
            }
        }

        if !merge_clusters.is_empty() {
            let mut to_remove = BTreeSet::new();
            for cluster in merge_clusters {
                let largest = *cluster
                    .iter()
                    .max_by_key(|&&idx| self.groups[idx].tiles.len())
                    .unwrap();
                let mut combined_tiles: BTreeSet<usize> =
                    self.groups[largest].tiles.iter().copied().collect();
                let largest_len = self.groups[largest].tiles.len() as f64;
                let mut weighted_v = [
                    self.groups[largest].velocity[0] * largest_len,
                    self.groups[largest].velocity[1] * largest_len,
                    self.groups[largest].velocity[2] * largest_len,
                ];

                for &other in &cluster {
                    if other != largest && !to_remove.contains(&other) {
                        let o_len = self.groups[other].tiles.len() as f64;
                        weighted_v[0] += self.groups[other].velocity[0] * o_len;
                        weighted_v[1] += self.groups[other].velocity[1] * o_len;
                        weighted_v[2] += self.groups[other].velocity[2] * o_len;
                        combined_tiles.extend(self.groups[other].tiles.iter().copied());
                        to_remove.insert(other);
                    }
                }

                let total_len = combined_tiles.len().max(1) as f64;
                self.groups[largest].tiles = combined_tiles.into_iter().collect();
                self.groups[largest].velocity = [
                    weighted_v[0] / total_len,
                    weighted_v[1] / total_len,
                    weighted_v[2] / total_len,
                ];
            }

            let mut kept_groups = Vec::with_capacity(self.groups.len());
            for (idx, g) in self.groups.drain(..).enumerate() {
                if !to_remove.contains(&idx) {
                    kept_groups.push(g);
                }
            }
            self.groups = kept_groups;
        }

        // 10. Randomly split large continent shapes (`planetsimulation.py:374-377`).
        let min_splittable_len = (self.splitnum * 0.35).ceil() as usize;
        let mut next_groups = Vec::with_capacity(self.groups.len() + 2);
        for g in self.groups.drain(..) {
            let g_len = g.tiles.len();
            if g_len >= min_splittable_len.max(24)
                && self.rng.uniform(0.0, 1.0) > self.splitnum / (g_len as f64)
            {
                if let Some(((a_tiles, va), (b_tiles, vb))) =
                    split_group(&self.grid, &self.adjacency, &g.tiles, &mut self.rng)
                {
                    // Spawn a new mid-ocean spreading ridge along the rift seam between `a_tiles` and `b_tiles`.
                    let b_set: BTreeSet<usize> = b_tiles.iter().copied().collect();
                    let seam_tiles: Vec<usize> = a_tiles
                        .iter()
                        .copied()
                        .filter(|&u| self.adjacency[u].iter().any(|nb| b_set.contains(nb)))
                        .collect();
                    if seam_tiles.len() >= 2 {
                        let m_tile = center_tile(&self.grid, &seam_tiles);
                        let p_m = self.grid.faces[m_tile].0;
                        let e1_tile = farthest_tile(&self.grid, &seam_tiles, p_m);
                        let p_e1 = self.grid.faces[e1_tile].0;
                        let e2_tile = farthest_tile(&self.grid, &seam_tiles, p_e1);
                        let p_e2 = self.grid.faces[e2_tile].0;
                        if e1_tile != m_tile && m_tile != e2_tile {
                            self.ridges.push(SpreadingRidgeArc {
                                start: p_e1,
                                end: p_m,
                                velocity: g.velocity,
                                age_ticks: 0,
                            });
                            self.ridges.push(SpreadingRidgeArc {
                                start: p_m,
                                end: p_e2,
                                velocity: g.velocity,
                                age_ticks: 0,
                            });
                        } else if e1_tile != e2_tile {
                            self.ridges.push(SpreadingRidgeArc {
                                start: p_e1,
                                end: p_e2,
                                velocity: g.velocity,
                                age_ticks: 0,
                            });
                        }
                        while self.ridges.len() > 18 {
                            self.ridges.remove(0);
                        }
                    }

                    let dp = self.dp;
                    let max_speed = 1.35 * dp;
                    let mut va_full = [
                        g.velocity[0] + va[0] * dp,
                        g.velocity[1] + va[1] * dp,
                        g.velocity[2] + va[2] * dp,
                    ];
                    let sa = length(va_full);
                    if sa > max_speed {
                        let scale = max_speed / sa;
                        va_full = [va_full[0] * scale, va_full[1] * scale, va_full[2] * scale];
                    }
                    let mut vb_full = [
                        g.velocity[0] + vb[0] * dp,
                        g.velocity[1] + vb[1] * dp,
                        g.velocity[2] + vb[2] * dp,
                    ];
                    let sb = length(vb_full);
                    if sb > max_speed {
                        let scale = max_speed / sb;
                        vb_full = [vb_full[0] * scale, vb_full[1] * scale, vb_full[2] * scale];
                    }
                    next_groups.push(LithosphericGroup {
                        tiles: a_tiles,
                        velocity: va_full,
                    });
                    next_groups.push(LithosphericGroup {
                        tiles: b_tiles,
                        velocity: vb_full,
                    });
                    continue;
                }
            }
            next_groups.push(g);
        }
        self.groups = next_groups;

        // 11. Stochastic meteor / bolide impacts (Hadean-Archean Late Heavy Bombardment decay
        //     followed by steady Phanerozoic Chicxulub/Sudbury-scale bolide flux), scaled by `meteor_freq`:
        if self.meteor_freq > 0.0 && n > 0 {
            let base_p = if self.geoticks < 45 {
                0.28 * (-(self.geoticks as f64) / 16.0).exp()
            } else {
                0.055
            };
            let mut rem = base_p * self.meteor_freq;
            let mut impact_rng = Pcg64Mcg::from_seed_u64(
                (self.geoticks as u64)
                    .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                    .wrapping_add((n as u64).wrapping_mul(0x517C_C1B7_2722_0A95)),
            );
            while rem > 0.0 {
                let p_impact = rem.min(1.0);
                if impact_rng.next_f64() < p_impact {
                    let center = (impact_rng.next_u64() as usize) % n;
                    let u = impact_rng.next_f64();
                    let crater_d_km = if self.geoticks < 45 {
                        160.0 + 420.0 * u * u
                    } else {
                        140.0 + 180.0 * u * u
                    };
                    self.apply_bolide_impact(center, crater_d_km);
                }
                rem -= 1.0;
            }
        }

        // Synchronize `tile_impact_scars` with the advected/weathered `TileGeology` columns:
        for v in 0..n {
            self.tile_impact_scars[v] = self.tiles[v].impact_signature();
        }

        self.geoticks += 1;
    }

    /// Find the tile index nearest to a 3D unit vector in grid coordinates.
    pub fn nearest_tile(&self, vec_grid: Vec3) -> usize {
        self.tree.nearest( normal(vec_grid) )
    }

    /// Format a multi-line stratigraphic inspection report for `tile_idx`
    /// (surface to Moho, matching `TectonicsPresenter.selecttile` in `tectonicspresenter.py:45-53`).
    pub fn inspect_tile(&self, tile_idx: usize) -> String {
        let Some(tile) = self.tiles.get(tile_idx) else {
            return String::new();
        };
        let mut lines = Vec::new();
        let elev = tile.elevation();
        let thick = tile.thickness();
        let kind_str = if elev > 0.0 { "Land" } else { "Ocean" };
        let koeppen_str = if self.has_atmosphere() && elev > 0.0 {
            let code = match self.tile_koeppen[tile_idx] {
                1 => "Af",
                2 => "Am",
                3 => "Aw",
                4 => "BS",
                5 => "BW",
                6 => "Cf",
                7 => "Cs",
                8 => "Cw",
                9 => "Df",
                10 => "Ds",
                11 => "Dw",
                12 => "EF",
                13 => "ET",
                _ => "Ocean",
            };
            format!(" | Köppen: {code}")
        } else {
            String::new()
        };

        lines.push(format!(
            "Tile #{tile_idx} ({kind_str}) | Elev: {elev:+.2} km | Crust: {thick:.1} km{koeppen_str}"
        ));
        if tile.subduction > 0.0 {
            lines.push(format!(
                "Active Subduction: {:.2} km | Mountainosity: {:.0}%",
                tile.subduction,
                tile.mountainosity_norm() * 100.0
            ));
        }
        let scar = self.tile_impact_scars[tile_idx];
        if scar > 0.05 {
            let zone = if scar > 0.60 {
                "Impact Crater Basin (Shocked Hornfels / Melt)"
            } else if scar > 0.32 {
                "Impact Ejecta Rim (Breccia Conglomerate)"
            } else {
                "Weathered Impact Ring"
            };
            lines.push(format!("Meteor Scar: {zone} (intensity {:.0}%)", scar * 100.0));
        }

        // Layers from surface (top) down to Moho (bottom):
        for layer in tile.layers.iter().rev() {
            let r = &layer.rock;
            let mut attrs = vec![
                format!("type: {}", r.kind.code()),
                format!("felsity: {:.2}", r.felsity),
                format!("tough: {:.2}", r.toughness),
            ];
            if let Some(c) = r.clasticity {
                attrs.push(format!("clast: {c:.1}"));
            }
            if let Some(c) = r.calcity {
                attrs.push(format!("calc: {c:.2}"));
            }
            if let Some(b) = r.bogginess {
                attrs.push(format!("bog: {b:.2}"));
            }
            lines.push(format!(
                "{} ({:.2} km) [{}]",
                r.name,
                layer.thickness,
                attrs.join(", ")
            ));
        }

        lines.join("\n")
    }

    fn refresh_climate_fields(&mut self) {
        self.climate_model
            .set_spin_and_tilt_for_tectonics(self.current_spin(), self.current_tilt_deg());
        let land_elevs: Vec<f32> = self
            .tiles
            .iter()
            .map(|t| t.elevation().max(0.0))
            .collect();
        let has_life = self.has_life();
        self.climate_model
            .update_for_tectonics(&land_elevs, has_life);

        for (v, c) in self.climate_model.classifications().iter().enumerate() {
            self.tile_koeppen[v] = c.koeppen as u8;
            self.tile_life[v] = if c.life { 1.0 } else { 0.0 };
            let temp_norm = c.mean_temperature as f32;
            self.tile_temp_norm[v] = temp_norm;
            // Convert normalized temperature to Celsius using Tectonics temprange (-20..55 C at glaciation 0.5)
            self.tile_temp_c[v] = temp_norm * 75.0 - 20.0;
            self.tile_precip_norm[v] = c.mean_precipitation as f32;
        }
    }

    /// Convert the current tectonic simulation state into a `tec11_proto::PlanetSnapshot`
    /// (`Section 7` of the design document).
    pub fn to_snapshot(&self) -> tec11_proto::PlanetSnapshot {
        let proto_tiles: Vec<tec11_proto::GeologicalTile> = self
            .tiles
            .iter()
            .map(|t| {
                let layers = t
                    .layers
                    .iter()
                    .map(|l| {
                        let r = &l.rock;
                        let kind = match r.kind {
                            RockKind::Igneous => {
                                let is_intrusive =
                                    matches!(r.name, "gabbro" | "andesite" | "granite");
                                tec11_proto::RockKindOneof::Igneous(tec11_proto::IgneousRock {
                                    is_intrusive,
                                    felsity: r.felsity,
                                })
                            }
                            RockKind::Sedimentary => {
                                tec11_proto::RockKindOneof::Sedimentary(
                                    tec11_proto::SedimentaryRock {
                                        clasticity: r.clasticity.unwrap_or(1.0),
                                        felsity: Some(r.felsity),
                                        calcity: r.calcity,
                                        bogginess: r.bogginess,
                                    },
                                )
                            }
                            RockKind::Metamorphic => {
                                tec11_proto::RockKindOneof::Metamorphic(
                                    tec11_proto::MetamorphicRock {
                                        facies: tec11_proto::MetamorphicFacies::from_rock_name(
                                            r.name,
                                        ) as i32,
                                        toughness: r.toughness,
                                        felsity: Some(r.felsity),
                                        calcity: r.calcity,
                                        bogginess: r.bogginess,
                                    },
                                )
                            }
                        };
                        tec11_proto::RockLayer {
                            rock: Some(tec11_proto::Rock { kind: Some(kind) }),
                            thickness: l.thickness,
                        }
                    })
                    .collect();
                tec11_proto::GeologicalTile {
                    bottom: t.bottom,
                    mountainosity: t.mountainosity,
                    layers,
                }
            })
            .collect();

        let proto_groups: Vec<tec11_proto::LithosphericGroup> = self
            .groups
            .iter()
            .map(|g| tec11_proto::LithosphericGroup {
                tile_ids: g.tiles.iter().map(|&id| id as u32).collect(),
                velocity: Some(tec11_proto::Vec3 {
                    x: g.velocity[0] as f32,
                    y: g.velocity[1] as f32,
                    z: g.velocity[2] as f32,
                }),
            })
            .collect();

        let proto_ridges: Vec<tec11_proto::SpreadingRidgeArc> = self
            .ridges
            .iter()
            .map(|r| tec11_proto::SpreadingRidgeArc {
                start: Some(tec11_proto::Vec3 {
                    x: r.start[0] as f32,
                    y: r.start[1] as f32,
                    z: r.start[2] as f32,
                }),
                end: Some(tec11_proto::Vec3 {
                    x: r.end[0] as f32,
                    y: r.end[1] as f32,
                    z: r.end[2] as f32,
                }),
                velocity: Some(tec11_proto::Vec3 {
                    x: r.velocity[0] as f32,
                    y: r.velocity[1] as f32,
                    z: r.velocity[2] as f32,
                }),
                age_ticks: r.age_ticks,
            })
            .collect();

        tec11_proto::PlanetSnapshot {
            rng_state_16b: self.rng.to_bytes().to_vec(),
            grid_size: self.grid.size,
            radius_km: self.radius_km as f32,
            spin: (24.0 / self.target_day_hours.max(1.0)) as f32,
            tilt_deg: self.base_tilt_deg as f32,
            stage: Some(tec11_proto::PlanetStageOneof::Tectonics(
                tec11_proto::TectonicsStage {
                    tectonics_ticks: self.geoticks,
                    atmosphere_ticks_remaining: self.atmosphere_ticks_remaining,
                    life_ticks_remaining: self.life_ticks_remaining,
                    people_ticks_remaining: self.total_target_ticks,
                    tiles: proto_tiles,
                    groups: proto_groups,
                    ridges: proto_ridges,
                },
            )),
        }
    }

    /// Encode the current tectonic simulation state to `.tec11` binary Protocol Buffer bytes.
    pub fn encode_snapshot(&self) -> Vec<u8> {
        self.to_snapshot().encode_bytes()
    }

    /// Reconstruct a `TectonicsSimulation` from a `tec11_proto::PlanetSnapshot`.
    pub fn from_snapshot(snap: &tec11_proto::PlanetSnapshot, fallback_seed: u64) -> Option<Self> {
        let grid_size = snap.grid_size;
        if !(2..=7).contains(&grid_size) {
            return None;
        }
        let grid = SphericalGrid::with_size(grid_size);
        let n = grid.faces.len();
        let adjacency = grid.adjacency();

        let (
            tiles,
            mut groups,
            ridges,
            geoticks,
            atmosphere_ticks_remaining,
            life_ticks_remaining,
            total_target_ticks,
        ) = match snap.stage.as_ref()? {
            tec11_proto::PlanetStageOneof::Tectonics(tec) => {
                if tec.tiles.len() != n {
                    return None;
                }
                let tiles: Vec<TileGeology> = tec
                    .tiles
                    .iter()
                    .map(TileGeology::from_proto_geological_tile)
                    .collect::<Option<Vec<_>>>()?;
                let groups: Vec<LithosphericGroup> = tec
                    .groups
                    .iter()
                    .map(|pg| {
                        let v = pg.velocity.unwrap_or_default();
                        LithosphericGroup {
                            tiles: pg
                                .tile_ids
                                .iter()
                                .map(|&id| id as usize)
                                .filter(|&id| id < n)
                                .collect(),
                            velocity: [v.x as f64, v.y as f64, v.z as f64],
                        }
                    })
                    .collect();
                let ridges: Vec<SpreadingRidgeArc> = tec
                    .ridges
                    .iter()
                    .map(|pr| {
                        let s = pr.start.unwrap_or_default();
                        let e = pr.end.unwrap_or_default();
                        let v = pr.velocity.unwrap_or_default();
                        SpreadingRidgeArc {
                            start: [s.x as f64, s.y as f64, s.z as f64],
                            end: [e.x as f64, e.y as f64, e.z as f64],
                            velocity: [v.x as f64, v.y as f64, v.z as f64],
                            age_ticks: pr.age_ticks,
                        }
                    })
                    .collect();
                let total_target = if tec.people_ticks_remaining > 0 {
                    tec.people_ticks_remaining
                } else {
                    tec.tectonics_ticks.max(75)
                };
                (
                    tiles,
                    groups,
                    ridges,
                    tec.tectonics_ticks,
                    tec.atmosphere_ticks_remaining,
                    tec.life_ticks_remaining,
                    total_target,
                )
            }
            tec11_proto::PlanetStageOneof::Prehistory(pre) => {
                if pre.tiles.len() != n {
                    return None;
                }
                let tiles: Vec<TileGeology> = pre
                    .tiles
                    .iter()
                    .map(|pt| TileGeology::from_proto_geological_tile(pt.geology.as_ref()?))
                    .collect::<Option<Vec<_>>>()?;
                let groups: Vec<LithosphericGroup> = pre
                    .groups
                    .iter()
                    .map(|pg| {
                        let v = pg.velocity.unwrap_or_default();
                        LithosphericGroup {
                            tiles: pg
                                .tile_ids
                                .iter()
                                .map(|&id| id as usize)
                                .filter(|&id| id < n)
                                .collect(),
                            velocity: [v.x as f64, v.y as f64, v.z as f64],
                        }
                    })
                    .collect();
                (
                    tiles,
                    groups,
                    Vec::new(),
                    pre.tectonics_ticks,
                    0,
                    0,
                    pre.tectonics_ticks.max(75),
                )
            }
            tec11_proto::PlanetStageOneof::History(hist) => {
                let pre = hist.prehistory_base.as_ref()?;
                if pre.tiles.len() != n {
                    return None;
                }
                let tiles: Vec<TileGeology> = pre
                    .tiles
                    .iter()
                    .map(|pt| TileGeology::from_proto_geological_tile(pt.geology.as_ref()?))
                    .collect::<Option<Vec<_>>>()?;
                let groups: Vec<LithosphericGroup> = pre
                    .groups
                    .iter()
                    .map(|pg| {
                        let v = pg.velocity.unwrap_or_default();
                        LithosphericGroup {
                            tiles: pg
                                .tile_ids
                                .iter()
                                .map(|&id| id as usize)
                                .filter(|&id| id < n)
                                .collect(),
                            velocity: [v.x as f64, v.y as f64, v.z as f64],
                        }
                    })
                    .collect();
                (
                    tiles,
                    groups,
                    Vec::new(),
                    pre.tectonics_ticks,
                    0,
                    0,
                    pre.tectonics_ticks.max(75),
                )
            }
        };

        if groups.is_empty() {
            let mut visited = vec![false; n];
            for start in 0..n {
                if visited[start] || tiles[start].elevation() <= 0.0 {
                    continue;
                }
                let mut comp = Vec::new();
                let mut queue = std::collections::VecDeque::new();
                visited[start] = true;
                queue.push_back(start);
                while let Some(u) = queue.pop_front() {
                    comp.push(u);
                    for &nb in &adjacency[u] {
                        if !visited[nb] && tiles[nb].elevation() > 0.0 {
                            visited[nb] = true;
                            queue.push_back(nb);
                        }
                    }
                }
                groups.push(LithosphericGroup {
                    tiles: comp,
                    velocity: [0.0, 0.0, 0.0],
                });
            }
        }

        let indexed_pts: Vec<(Vec3, usize)> = grid
            .faces
            .iter()
            .enumerate()
            .map(|(i, (c, _))| (*c, i))
            .collect();
        let tree = PointTree::new(&indexed_pts);
        let radius_km = if snap.radius_km > 0.0 {
            snap.radius_km as f64
        } else {
            PangeaWorld::planet_radius_km(grid_size)
        };

        let spin = if snap.spin > 0.0 {
            snap.spin as f64
        } else {
            1.0
        };
        let target_day_hours = (24.0 / spin).clamp(1.0, 240.0);
        let cells = tec11_climate::nominal_cells_for_spin(spin);
        let tilt_deg = if snap.tilt_deg > 0.0 {
            snap.tilt_deg as f64
        } else {
            23.0
        };
        let dt = 5.0;
        let dp = (100.0 / radius_km) * dt;
        let build_rate = dt / 5.0;
        let erode_rate = dt;
        let earth_area_km2 = 4.0 * PI * 6400.0 * 6400.0;
        let splitnum = (25.0e6 / earth_area_km2) * (n as f64);

        let rng = if let Ok(bytes_16) = <[u8; 16]>::try_from(snap.rng_state_16b.as_slice()) {
            Pcg64Mcg::from_bytes(bytes_16)
        } else {
            Pcg64Mcg::from_seed_u64(fallback_seed.wrapping_add(geoticks as u64))
        };

        let climate_model = ClimateModel::new_for_tectonics(grid.clone(), cells, spin, tilt_deg);

        let mut sim = Self {
            grid,
            adjacency,
            tree,
            radius_km,
            dp,
            build_rate,
            erode_rate,
            splitnum,
            atmosphere_ticks_remaining,
            life_ticks_remaining,
            total_target_ticks,
            geoticks,
            target_day_hours,
            base_tilt_deg: tilt_deg,
            moon_count: 1,
            moon_mass: 1.0,
            target_moon_dist_radii: 60.0,
            tiles,
            groups,
            ridges,
            climate_model,
            tile_koeppen: vec![0; n],
            tile_life: vec![0.0; n],
            tile_temp_norm: vec![0.5; n],
            tile_temp_c: vec![15.0; n],
            tile_precip_norm: vec![0.5; n],
            tile_impact_scars: vec![0.0; n],
            meteor_freq: 1.0,
            impact_count: 0,
            impact_winter_ticks: 0,
            sterilization_delays: 0,
            last_impact_tile: None,
            last_impact_diameter_km: 0.0,
            rng,
        };
        sim.climate_model
            .set_spin_and_tilt_for_tectonics(sim.current_spin(), sim.current_tilt_deg());
        if sim.has_atmosphere() {
            sim.refresh_climate_fields();
        }
        // Reconstruct visible impact scars from shock-metamorphic hornfels / coarse impact breccia in top strata:
        for v in 0..n {
            sim.tile_impact_scars[v] = sim.tiles[v].impact_signature();
        }
        Some(sim)
    }

    /// Decode `.tec11` binary Protocol Buffer bytes into a `TectonicsSimulation`.
    pub fn decode_snapshot(bytes: &[u8], fallback_seed: u64) -> Option<Self> {
        let snap = tec11_proto::PlanetSnapshot::decode_bytes(bytes).ok()?;
        Self::from_snapshot(&snap, fallback_seed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tec11_proto::GeologicalTileExt;

    #[test]
    fn test_tectonics_simulation_lifecycle() {
        // Use size 4 (2,100 km radius, 812 tiles) for fast multi-step lifecycle test
        let mut sim = TectonicsSimulation::new(4, 29.0, 42);
        assert_eq!(sim.geoticks(), 0);
        assert_eq!(sim.continent_count(), 1);
        assert!(!sim.has_atmosphere());
        assert!(!sim.has_life());

        // Step 30 ticks (150 Myr): Pangaea should rift/move, atmosphere should activate at tick 25,
        // and metamorphic/sedimentary/igneous layers should form.
        for _ in 0..30 {
            sim.step();
        }

        assert_eq!(sim.geoticks(), 30);
        assert!(sim.has_atmosphere());
        assert!(sim.continent_count() >= 1);
        assert!(
            (15..=40).contains(&sim.land_percent()),
            "Expected land percent to remain near initial ~29%, got {}%",
            sim.land_percent()
        );

        let has_metamorphic = sim.tiles().iter().any(|t| {
            t.layers
                .iter()
                .any(|l| l.rock.kind == RockKind::Metamorphic)
        });
        let has_sedimentary = sim.tiles().iter().any(|t| {
            t.layers
                .iter()
                .any(|l| l.rock.kind == RockKind::Sedimentary)
        });
        assert!(has_metamorphic, "Expected regional/contact metamorphic layers after 30 ticks");
        assert!(has_sedimentary, "Expected eroded sedimentary layers after atmosphere activates");

        let report = sim.inspect_tile(0);
        assert!(report.contains("Tile #0"));
    }

    #[test]
    fn test_no_interior_pinholes() {
        let mut sim = TectonicsSimulation::new(5, 29.0, 42);
        for tick in 1..=35 {
            sim.step();

            // Count non-land components (ocean + any interior holes)
            let n = sim.tile_count();
            let mut visited = vec![false; n];
            let mut ocean_comps = Vec::new();
            for start in 0..n {
                if visited[start] || sim.tiles()[start].elevation() > 0.0 {
                    continue;
                }
                let mut comp = Vec::new();
                let mut q = std::collections::VecDeque::new();
                visited[start] = true;
                q.push_back(start);
                while let Some(u) = q.pop_front() {
                    comp.push(u);
                    for &nb in &sim.adjacency[u] {
                        if !visited[nb] && sim.tiles()[nb].elevation() <= 0.0 {
                            visited[nb] = true;
                            q.push_back(nb);
                        }
                    }
                }
                ocean_comps.push(comp);
            }
            ocean_comps.sort_by_key(|c| std::cmp::Reverse(c.len()));
            let pinholes: Vec<usize> = ocean_comps
                .iter()
                .skip(1)
                .map(|c| c.len())
                .filter(|&len| len <= 3)
                .collect();
            assert!(
                pinholes.is_empty(),
                "Tick {}: unexpected 1-3 tile interior pinholes: {:?}",
                tick,
                pinholes
            );
        }
    }

    #[test]
    fn test_realistic_ocean_bathymetry() {
        let mut sim = TectonicsSimulation::new(5, 29.0, 42);
        for _ in 0..25 {
            sim.step();
        }
        let n = sim.tile_count();
        let mut min_ocean = f32::INFINITY;
        let mut max_ocean = f32::NEG_INFINITY;
        let mut non_coastal_above_abyssal = 0usize;
        for v in 0..n {
            let elev = sim.tiles()[v].elevation();
            if elev <= 0.0 {
                min_ocean = min_ocean.min(elev);
                max_ocean = max_ocean.max(elev);
                let has_land_nb = sim.adjacency[v]
                    .iter()
                    .any(|&nb| sim.tiles()[nb].elevation() > 0.0);
                if !has_land_nb {
                    // Open-ocean tiles (not adjacent to a continent) must follow ridge-to-abyssal
                    // thermal subsidence [-5.5, -2.5] km and never retain deep trench scars (< -5.6 km).
                    assert!(
                        (-5.6..=-2.4).contains(&elev),
                        "Open-ocean tile {} has out-of-range elevation {} km",
                        v,
                        elev
                    );
                    if elev > -4.5 {
                        non_coastal_above_abyssal += 1;
                    }
                }
            }
        }
        assert!(
            min_ocean >= -7.0 && min_ocean < -5.3,
            "Expected abyssal/trench depths in [-7.0, -5.3), got {}",
            min_ocean
        );
        assert!(
            max_ocean <= -0.15 && max_ocean > -1.5,
            "Expected passive continental shelf in (-1.5, -0.15], got {}",
            max_ocean
        );
        assert!(
            non_coastal_above_abyssal > 20,
            "Expected active mid-ocean ridges to elevate open-ocean tiles above abyssal plain, got {}",
            non_coastal_above_abyssal
        );
    }

    #[test]
    fn test_orogeny_erosion_balance() {
        let mut sim = TectonicsSimulation::new(4, 45.0, 42);
        for _ in 0..80 {
            sim.step();
        }
        let land_tiles: Vec<usize> = (0..sim.tile_count())
            .filter(|&v| sim.tiles()[v].elevation() > 0.0)
            .collect();
        assert!(!land_tiles.is_empty());
        let mean_land_elev: f32 = land_tiles
            .iter()
            .map(|&v| sim.tiles()[v].elevation())
            .sum::<f32>()
            / (land_tiles.len() as f32);
        let max_land_elev: f32 = land_tiles
            .iter()
            .map(|&v| sim.tiles()[v].elevation())
            .fold(0.0f32, f32::max);
        let alpine_snow_count = land_tiles
            .iter()
            .filter(|&&v| matches!(sim.tile_koeppen_ids()[v], 12 | 13))
            .count();
        let alpine_snow_frac = (alpine_snow_count as f64) / (land_tiles.len() as f64);

        assert!(
            max_land_elev > 4.2,
            "Expected active orogeny to build mountain ranges > 4.2 km after 80 geoticks, got max {:.2} km (mean {:.2} km)",
            max_land_elev,
            mean_land_elev
        );
        assert!(
            (0.45..=2.40).contains(&mean_land_elev),
            "Expected mean continental elevation in [0.45, 2.40] km after 80 geoticks, got {:.2} km (max {:.2} km)",
            mean_land_elev,
            max_land_elev
        );
        assert!(
            alpine_snow_frac < 0.35,
            "Expected < 35% of land to be polar/alpine snow (ET/EF) after 80 geoticks, got {:.1}% (mean elev {:.2} km, max {:.2} km)",
            alpine_snow_frac * 100.0,
            mean_land_elev,
            max_land_elev
        );
    }

    #[test]
    fn test_proto_snapshot_roundtrip_and_climate_load() {
        let mut sim = TectonicsSimulation::new(4, 30.0, 99);
        for _ in 0..30 {
            sim.step();
        }
        let bytes = sim.encode_snapshot();
        // Verify Protobuf header starts with field 1 (`rng_state_16b`, tag 0x0A, len 0x10)
        assert_eq!(bytes[0], 0x0A);
        assert_eq!(bytes[1], 0x10);

        let mut restored =
            TectonicsSimulation::decode_snapshot(&bytes, 99).expect("valid .tec11 proto snapshot");
        assert_eq!(restored.grid().size, sim.grid().size);
        assert_eq!(restored.geoticks(), sim.geoticks());
        assert_eq!(restored.continent_count(), sim.continent_count());
        assert_eq!(restored.land_percent(), sim.land_percent());
        assert_eq!(restored.has_atmosphere(), sim.has_atmosphere());
        assert_eq!(restored.has_life(), sim.has_life());
        for i in 0..sim.tile_count() {
            assert!((restored.tiles()[i].elevation() - sim.tiles()[i].elevation()).abs() < 1e-5);
            assert_eq!(restored.tiles()[i].layers.len(), sim.tiles()[i].layers.len());
        }

        // Verify loading the decoded PlanetSnapshot into ClimateModel produces seasonal climate and ocean currents:
        let snap = tec11_proto::PlanetSnapshot::decode_bytes(&bytes).unwrap();
        let elevs: Vec<f32> = snap
            .geological_tiles()
            .unwrap()
            .into_iter()
            .map(|t| t.elevation())
            .collect();
        let climate = ClimateModel::with_elevations(restored.grid().clone(), elevs, 3, 0.5);
        assert_eq!(climate.tile_count(), restored.tile_count());
        assert_eq!(climate.season_count(), 8);
        assert!(climate.ocean_for_season(0).is_some());
        assert!(climate.ocean_for_season(4).is_some());

        // Verify exact PRNG continuation: stepping both simulations 5 more ticks produces identical state!
        for _ in 0..5 {
            sim.step();
            restored.step();
        }
        assert_eq!(restored.geoticks(), sim.geoticks());
        assert_eq!(restored.continent_count(), sim.continent_count());
        assert_eq!(restored.land_percent(), sim.land_percent());
    }

    #[test]
    fn test_lunar_recession_spin_braking_and_harmonic_orbits() {
        // 1 Earth-like Moon (target: 24h day, 23 deg tilt, 60 Rp moon distance after 75 ticks = 375 Myr):
        let mut earth =
            TectonicsSimulation::new_with_full_params(4, 24.0, 23.0, 29.0, 125, 125, 125, 1, 1.0, 60.0, 42);
        let a0 = earth.current_moon_dist_radii();
        let day0 = earth.current_day_hours();
        let tide0 = earth.current_tidal_strength();
        assert!(
            a0 > 20.0 && a0 < 30.0,
            "Expected Hadean Moon to start near ~23.4 Rp, got {a0:.2} Rp"
        );
        assert!(
            day0 > 9.5 && day0 < 13.5,
            "Expected Hadean Earth day to start near ~11.1 hours, got {day0:.2} h"
        );
        assert!(
            tide0 > 10.0,
            "Expected Hadean tides > 10x modern Earth, got {tide0:.2}x"
        );

        // Advance to target Prehistory/History transition (75 ticks = 375 Myr):
        earth.geoticks = 75;
        assert!(
            (earth.current_moon_dist_radii() - 60.0).abs() < 1e-4,
            "Expected Moon to reach 60.0 Rp at Prehistory start, got {:.3}",
            earth.current_moon_dist_radii()
        );
        assert!(
            (earth.current_day_hours() - 24.0).abs() < 1e-4,
            "Expected Earth day to brake to 24.0 h at Prehistory start, got {:.3}",
            earth.current_day_hours()
        );
        assert!(
            (earth.current_tidal_strength() - 1.0).abs() < 1e-4,
            "Expected tidal strength = 1.0x modern Earth at Prehistory start, got {:.3}",
            earth.current_tidal_strength()
        );

        // Verify 2:1 Laplace resonance chain multipliers (1 + 1/4 + 1/16 = 1.3125):
        assert!((TectonicsSimulation::harmonic_tidal_multiplier(1) - 1.0).abs() < 1e-9);
        assert!((TectonicsSimulation::harmonic_tidal_multiplier(2) - 1.25).abs() < 1e-9);
        assert!((TectonicsSimulation::harmonic_tidal_multiplier(3) - 1.3125).abs() < 1e-9);

        // Verify moonless planet has weak solar-only tides (0.15x) and larger obliquity excursions:
        let mut moonless =
            TectonicsSimulation::new_with_full_params(4, 24.0, 23.0, 29.0, 125, 125, 125, 0, 0.0, 60.0, 42);
        assert!((moonless.current_tidal_strength() - 0.15).abs() < 1e-6);
        assert!((moonless.current_day_hours() - 24.0).abs() < 1e-6);
        moonless.geoticks = 8;
        let tilt_wander = (moonless.current_tilt_deg() - 23.0).abs();
        earth.geoticks = 8;
        let tilt_stable = (earth.current_tilt_deg() - 23.0).abs();
        assert!(
            tilt_wander > 5.0 * tilt_stable,
            "Expected moonless obliquity excursion ({tilt_wander:.2} deg) >> moon-stabilized ({tilt_stable:.2} deg)"
        );
    }

    #[test]
    fn test_bolide_impact_crater_excavation_and_biosphere_effects() {
        let mut sim = TectonicsSimulation::new(4, 30.0, 42);
        // Find a continental tile:
        let land_idx = (0..sim.tile_count())
            .find(|&v| sim.tiles()[v].elevation() > 0.8)
            .unwrap();
        let pre_elev = sim.tiles()[land_idx].elevation();

        // Apply a 200 km Chicxulub-scale bolide strike:
        sim.apply_bolide_impact(land_idx, 200.0);
        assert!(sim.impact_count() >= 1);
        assert!((sim.tile_impact_scars()[land_idx] - 1.0).abs() < 1e-5);
        assert!(
            sim.tiles()[land_idx].elevation() < pre_elev,
            "Expected bolide strike to excavate & depress crater floor"
        );
        assert!(
            sim.tiles()[land_idx].impact_signature() >= 0.65,
            "Expected shocked hornfels stratigraphic signature at crater floor"
        );
        let report = sim.inspect_tile(land_idx);
        assert!(report.contains("Impact Crater Basin"));

        // Verify meteor_freq = 0.0 disables stochastic bolide impacts across 25 ticks,
        // while meteor_freq = 4.0 increases impact count over meteor_freq = 1.0:
        let mut sim_off = TectonicsSimulation::new(4, 29.0, 42);
        sim_off.set_meteor_freq(0.0);
        let mut sim_std = TectonicsSimulation::new(4, 29.0, 42);
        sim_std.set_meteor_freq(1.0);
        let mut sim_heavy = TectonicsSimulation::new(4, 29.0, 42);
        sim_heavy.set_meteor_freq(4.0);
        for _ in 0..25 {
            sim_off.step();
            sim_std.step();
            sim_heavy.step();
        }
        assert_eq!(sim_off.impact_count(), 0);
        assert!(
            sim_off.tile_impact_scars().iter().all(|&s| s == 0.0),
            "Subduction contact-metamorphic hornfels and coastal conglomerates must not produce false meteor scars when meteor_freq = 0"
        );
        assert!(sim_heavy.impact_count() > sim_std.impact_count());

        // Verify that an impact crater on a moving continental plate advects with the continent
        // rather than remaining pinned to static grid coordinates:
        let mut sim_move = TectonicsSimulation::new(4, 29.0, 42);
        sim_move.set_meteor_freq(0.0);
        // Step until Pangaea splits and plates have non-zero velocity:
        for _ in 0..8 {
            sim_move.step();
        }
        let moving_group_idx = sim_move
            .groups
            .iter()
            .position(|g| {
                let speed = (g.velocity[0].powi(2) + g.velocity[1].powi(2) + g.velocity[2].powi(2)).sqrt();
                speed > 1e-4 && g.tiles.len() > 20
            })
            .expect("Expected a moving continental plate");
        // Pick an interior tile of the moving continent:
        let group_set: std::collections::HashSet<usize> = sim_move.groups[moving_group_idx]
            .tiles
            .iter()
            .copied()
            .collect();
        let impact_tile = sim_move.groups[moving_group_idx]
            .tiles
            .iter()
            .copied()
            .find(|&v| {
                sim_move.tiles()[v].elevation() > 0.5
                    && sim_move.adjacency[v].iter().all(|nb| group_set.contains(nb))
            })
            .unwrap_or(sim_move.groups[moving_group_idx].tiles[0]);
        let initial_pos = sim_move.grid().faces[impact_tile].0;
        let vel = sim_move.groups[moving_group_idx].velocity;

        sim_move.apply_bolide_impact(impact_tile, 200.0);
        assert!(sim_move.tile_impact_scars()[impact_tile] > 0.9);

        // Step 12 ticks so the continent drifts across several grid tiles:
        for _ in 0..12 {
            sim_move.step();
        }

        // Any tile that is now ocean must not retain the continental crater scar:
        for v in 0..sim_move.tile_count() {
            if sim_move.tiles()[v].elevation() <= 0.0 {
                assert!(
                    sim_move.tile_impact_scars()[v] < 0.05,
                    "Vacated ocean tile {v} retained continental crater scar {:.3}",
                    sim_move.tile_impact_scars()[v]
                );
            }
        }

        // The peak scar centroid should have displaced in the direction of plate tangent velocity `vel`:
        let peak_tile = (0..sim_move.tile_count())
            .max_by(|&a, &b| {
                sim_move.tile_impact_scars()[a]
                    .partial_cmp(&sim_move.tile_impact_scars()[b])
                    .unwrap()
            })
            .unwrap();
        assert!(
            sim_move.tile_impact_scars()[peak_tile] > 0.25,
            "Expected crater scar to persist on moving continent after 12 ticks"
        );
        let new_pos = sim_move.grid().faces[peak_tile].0;
        let actual_disp = [
            new_pos[0] - initial_pos[0],
            new_pos[1] - initial_pos[1],
            new_pos[2] - initial_pos[2],
        ];
        let dot_dir = actual_disp[0] * vel[0]
            + actual_disp[1] * vel[1]
            + actual_disp[2] * vel[2];
        assert!(
            peak_tile != impact_tile && dot_dir > 0.0,
            "Expected crater peak tile to move from {impact_tile} to {peak_tile} along plate velocity (dot={dot_dir:.5})"
        );
    }
}
