use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::PI;

use tec11_climate::ClimateModel;
use tec11_math_grid::{
    great_circle_distance, length, normal, Pcg64Mcg, PointTree, SphericalGrid, Vec3,
};

use crate::movement::{center_tile, move_group, split_group};
use crate::pangea::PangeaWorld;
use crate::rock::{
    deposit_sedimentary, metamorphic_contact, metamorphic_regional, DepositClimate,
    ErodedMaterialBatch, Rock, RockKind, RockLayer,
};
use crate::tile::TileGeology;

/// A moving continental / lithospheric group (`Group` in `tile.py:17-20`).
#[derive(Clone, Debug)]
pub struct LithosphericGroup {
    pub tiles: Vec<usize>,
    pub velocity: Vec3,
}

#[derive(Clone, Debug)]
struct TileMovement {
    sources: Vec<(f32, f32, Vec<RockLayer>)>,
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
    geoticks: u32,
    tiles: Vec<TileGeology>,
    groups: Vec<LithosphericGroup>,
    climate_model: ClimateModel,
    tile_koeppen: Vec<u8>,
    tile_life: Vec<f32>,
    tile_temp_norm: Vec<f32>,
    tile_temp_c: Vec<f32>,
    tile_precip_norm: Vec<f32>,
    rng: Pcg64Mcg,
}

impl TectonicsSimulation {
    /// Create a new tectonic simulation initialized with a random Pangaea supercontinent
    /// and surrounding ocean floor (`TectonicsPresenter` defaults: `grid_size = 6`, `land_pct = 29.0`,
    /// `dt = 5 Myr`, `atmdt = 125 Myr`, `lifedt = 125 Myr`).
    pub fn new(grid_size: u32, land_percent: f64, seed: u64) -> Self {
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

        let spin = 1.0;
        let cells = 3;
        let tilt_deg = 23.0;
        let dt = 5.0;
        let dp = (100.0 / radius_km) * dt;
        let build_rate = dt / 5.0;
        let erode_rate = dt;

        // Scale split threshold proportionally to planet surface area so a continent covering
        // ~9.7% of the globe (~50M km^2 on Earth, r = 6400 km) has a 50/50 chance of rifting
        // on any planet size (`planetsimulation.py:86`).
        let earth_area_km2 = 4.0 * PI * 6400.0 * 6400.0;
        let splitnum = (25.0e6 / earth_area_km2) * (n as f64);

        let atmosphere_ticks_remaining = (125.0 / dt) as u32;
        let life_ticks_remaining = (125.0 / dt) as u32;

        let groups = vec![LithosphericGroup {
            tiles: pangea.continent_tiles,
            velocity: [0.0, 0.0, 0.0],
        }];

        let climate_model = ClimateModel::new_for_tectonics(grid.clone(), cells, spin, tilt_deg);

        Self {
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
            geoticks: 0,
            tiles: pangea.tiles,
            groups,
            climate_model,
            tile_koeppen: vec![0; n],
            tile_life: vec![0.0; n],
            tile_temp_norm: vec![0.5; n],
            tile_temp_c: vec![15.0; n],
            tile_precip_norm: vec![0.5; n],
            rng,
        }
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

    /// Advance the tectonic simulation by one 5 Myr timestep (`PlanetSimulation.update`
    /// in `planetsimulation.py:211-381` + full ocean-floor bathymetry from Section 5.4).
    pub fn step(&mut self) {
        let n = self.tiles.len();

        // 0. Clear previous tick's transient subduction & intrusion markers.
        for tile in &mut self.tiles {
            tile.clear_temp();
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
                let sources: Vec<(f32, f32, Vec<RockLayer>)> = src_indices
                    .into_iter()
                    .map(|s| {
                        let st = &self.tiles[s];
                        (st.bottom, st.mountainosity, st.layers.clone())
                    })
                    .collect();
                new_movements
                    .entry(dest)
                    .or_default()
                    .push(TileMovement { sources, speed });
                overlapping[dest].push(i);
            }
        }

        // Close any 1-tile inter-plate pinholes (`>= deg - 1` neighbors, i.e. >= 5 of 6) trapped
        // between adjacent groups in `new_movements` without inflating `group.tiles` at rifts.
        for _pass in 0..2 {
            let mut suture_fills: Vec<(usize, usize, TileMovement)> = Vec::new();
            for h in 0..n {
                if new_movements.contains_key(&h) {
                    continue;
                }
                let deg = self.adjacency[h].len();
                let nb_in_cont: Vec<usize> = self.adjacency[h]
                    .iter()
                    .copied()
                    .filter(|&nb| new_movements.contains_key(&nb))
                    .collect();
                if nb_in_cont.len() + 1 >= deg && !nb_in_cont.is_empty() {
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
                            suture_fills.push((h, best_g, TileMovement { sources, speed }));
                        }
                    }
                }
            }
            if suture_fills.is_empty() {
                break;
            }
            for (h, best_g, tm) in suture_fills {
                new_movements.entry(h).or_default().push(tm);
                overlapping[h].push(best_g);
            }
        }

        let mut in_new_continent = vec![false; n];
        for &dest in new_movements.keys() {
            in_new_continent[dest] = true;
        }

        // 2. Apply tile movements, leading-edge subduction uplift, accretionary sediment scraping,
        //    and oceanic trench deflection.
        let mut collisions: BTreeMap<(usize, usize), usize> = BTreeMap::new();
        let mut active_trench_tile = vec![false; n];

        for (&dest, movements) in &new_movements {
            let is_leading_edge = !old_continental[dest];
            let build_amount = if is_leading_edge {
                let avg_speed: f64 =
                    movements.iter().map(|m| m.speed).sum::<f64>() / (movements.len() as f64);
                (self.build_rate * avg_speed) as f32
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

            let group_sources: Vec<Vec<(f32, f32, Vec<RockLayer>)>> =
                movements.iter().map(|m| m.sources.clone()).collect();
            self.tiles[dest].merge_sources(&group_sources);

            for sed in scraped_sediments {
                self.tiles[dest].deposit(sed);
            }

            if build_amount > 0.0 {
                self.tiles[dest].build(build_amount, Rock::seafloor());

                // Carve an oceanic subduction trench in the 1-ring of ocean tiles ahead of the leading edge.
                let trench_elev = (-5.8 - 22.0 * build_amount).clamp(-8.8, -5.8);
                for &nb in &self.adjacency[dest] {
                    if !in_new_continent[nb] {
                        let thick = self.tiles[nb].thickness();
                        self.tiles[nb].bottom = self.tiles[nb].bottom.min(trench_elev - thick);
                        self.tiles[nb].subduction = (build_amount * 0.5).max(self.tiles[nb].subduction);
                        active_trench_tile[nb] = true;
                    }
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

        // 3. Trailing-edge seafloor accretion (Mid-Ocean Ridges) & ocean floor thermal subsidence.
        //    Compute continent centroids to locate divergent spreading ridges between continents.
        let group_centers: Vec<Vec3> = self
            .groups
            .iter()
            .filter(|g| !g.tiles.is_empty())
            .map(|g| {
                let c_tile = center_tile(&self.grid, &g.tiles);
                self.grid.faces[c_tile].0
            })
            .collect();

        for v in 0..n {
            if (old_continental[v] || self.tiles[v].elevation() > 0.0) && !in_new_continent[v] {
                // Newly rifted / uncovered ocean floor in the wake of a moving continent:
                // spawn buoyant mid-ocean ridge crust (ophiolite sequence at -2.5 km).
                self.tiles[v] = TileGeology::birth_seafloor();
            } else if !in_new_continent[v] && !active_trench_tile[v] {
                let is_passive_shelf = self.adjacency[v]
                    .iter()
                    .any(|&nb| in_new_continent[nb] && self.tiles[nb].subduction <= 0.0);

                let target_elev = if is_passive_shelf {
                    -0.75f32
                } else if group_centers.len() >= 2 {
                    // Check angular distance to the nearest midpoint/equidistant ridge between continent pairs:
                    let p = self.grid.faces[v].0;
                    let mut d1 = f64::INFINITY;
                    let mut d2 = f64::INFINITY;
                    for &gc in &group_centers {
                        let d = great_circle_distance(p, gc);
                        if d < d1 {
                            d2 = d1;
                            d1 = d;
                        } else if d < d2 {
                            d2 = d;
                        }
                    }
                    let ridge_dist_km = ((d2 - d1).abs() * 0.5) * self.radius_km;
                    let u_ridge = (ridge_dist_km / 2400.0).clamp(0.0, 1.0);
                    (-2.5 - 2.9 * u_ridge.sqrt()) as f32
                } else {
                    -5.2f32
                };

                let cur_elev = self.tiles[v].elevation();
                let new_elev = cur_elev + 0.16 * (target_elev - cur_elev);
                self.tiles[v].bottom = new_elev.min(-0.15) - self.tiles[v].thickness();
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
            let land_elevs: Vec<f32> = self
                .tiles
                .iter()
                .map(|t| t.elevation().max(0.0))
                .collect();
            let has_life = self.has_life();
            self.climate_model.update_for_tectonics(&land_elevs, has_life);

            for (v, c) in self.climate_model.classifications().iter().enumerate() {
                self.tile_koeppen[v] = c.koeppen as u8;
                self.tile_life[v] = if c.life { 1.0 } else { 0.0 };
                let temp_norm = c.mean_temperature as f32;
                self.tile_temp_norm[v] = temp_norm;
                // Convert normalized temperature to Celsius using Tectonics temprange (-20..55 C at glaciation 0.5)
                self.tile_temp_c[v] = temp_norm * 75.0 - 20.0;
                self.tile_precip_norm[v] = c.mean_precipitation as f32;
            }

            // Build downhill erosion graph (`erodemethod.py:21-38` with physical local base-level bounds):
            // Each land tile (elevation > 0) erodes only into strictly lower neighbors (`h_u < h_v`),
            // weighted by the elevation drop, and capped at half the relief above its lowest neighbor
            // (and above sea level 0.05 km) so rivers never carve pits below their own outlet.
            let mut outgoing_dests: Vec<Vec<(usize, f32)>> = vec![Vec::new(); n];
            let mut incoming_sources: Vec<Vec<usize>> = vec![Vec::new(); n];
            let mut max_erosion_cap: Vec<f32> = vec![0.0; n];

            for v in 0..n {
                let elev_v = self.tiles[v].elevation();
                if elev_v > 0.05 {
                    let climate_factor = self.tile_precip_norm[v]
                        * (1.0 - self.tile_temp_norm[v]).max(0.0);
                    if climate_factor > 0.0 {
                        let mut min_nb_elev = elev_v;
                        for &u in &self.adjacency[v] {
                            let nb_elev = self.tiles[u].elevation().max(0.0);
                            if nb_elev < elev_v {
                                min_nb_elev = min_nb_elev.min(nb_elev);
                                let drop = elev_v - nb_elev;
                                let d = (drop / 10.0) * climate_factor;
                                if d > 0.0 {
                                    outgoing_dests[v].push((u, d));
                                    incoming_sources[u].push(v);
                                }
                            }
                        }
                        if min_nb_elev < elev_v {
                            let base_level = min_nb_elev.max(0.05);
                            max_erosion_cap[v] = (0.5 * (elev_v - base_level)).max(0.0);
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
                let num_all_neighbors = self.adjacency[v].len().max(1) as f32;
                let avg_degree = total_degree / num_all_neighbors;
                let (total_eroded, stripped_layers) =
                    self.tiles[v].erode_column(avg_degree, erode_dt, max_erosion_cap[v]);
                if total_eroded > 1e-6 && !stripped_layers.is_empty() {
                    // Distribute eroded material across lower neighbors proportional to slope,
                    // capped so no single neighbor receives more than its geometric 1/N perimeter share.
                    let denom = total_degree.max(avg_degree * num_all_neighbors);
                    for &(dest_u, deg_u) in &outgoing_dests[v] {
                        let amount_u = total_eroded * (deg_u / denom);
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
                    self.tiles[t].deposit(dep);
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

                        let mut source_groups = BTreeSet::new();
                        for &src_v in &incoming_sources[t] {
                            for &g_idx in &overlapping[src_v] {
                                source_groups.insert(g_idx);
                            }
                        }
                        for g_idx in source_groups {
                            if !self.groups[g_idx].tiles.contains(&t) {
                                self.groups[g_idx].tiles.push(t);
                            }
                            overlapping[t].push(g_idx);
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

        // 6. Apply continental isostatic rebound (`planetsimulation.py:318-321`).
        for (i, group) in self.groups.iter().enumerate() {
            if group.tiles.is_empty() {
                continue;
            }
            let pre_h = pre_erosion_heights.get(i).copied().unwrap_or(0.0);
            let post_h: f32 = group
                .tiles
                .iter()
                .map(|&t| self.tiles[t].elevation().max(0.0))
                .sum();
            let dh = (pre_h - post_h) / (group.tiles.len() as f32);
            for &t in &group.tiles {
                self.tiles[t].isostasize(dh);
                // Keep continental tiles slightly emergent above sea level
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
                    let dp = self.dp;
                    next_groups.push(LithosphericGroup {
                        tiles: a_tiles,
                        velocity: [
                            g.velocity[0] + va[0] * dp,
                            g.velocity[1] + va[1] * dp,
                            g.velocity[2] + va[2] * dp,
                        ],
                    });
                    next_groups.push(LithosphericGroup {
                        tiles: b_tiles,
                        velocity: [
                            g.velocity[0] + vb[0] * dp,
                            g.velocity[1] + vb[1] * dp,
                            g.velocity[2] + vb[2] * dp,
                        ],
                    });
                    continue;
                }
            }
            next_groups.push(g);
        }
        self.groups = next_groups;

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
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
