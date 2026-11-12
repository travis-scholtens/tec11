use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::PI;

use tec11_math_grid::{normal, rotate, Pcg64Mcg, SphericalGrid, SphericalPolygon, Vec3};

use crate::population::{is_passable, DisjointSet};

/// A distinct population heritage (`race.py:1-10`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heritage {
    pub id: u32,
    pub name: String,
    pub ancestry: Vec<u32>,
}

/// Group populated tiles into connected components within `travel_range` steps over passable land,
/// admixing multiple heritages that share a connected component and branching any single heritage
/// that becomes split across multiple geographically isolated components (`racinatemethod.py:5-41`).
pub fn racinate<F>(
    elevations: &[f32],
    koeppen_ids: &[u8],
    adjacency: &[Vec<usize>],
    populated: &mut BTreeMap<usize, u32>,
    agricultural_heritages: &mut BTreeSet<u32>,
    heritages: &mut BTreeMap<u32, Heritage>,
    new_heritage: F,
    travel_range: usize,
) where
    F: FnMut(Vec<u32>) -> Heritage,
{
    let mut rivergoing_heritages = BTreeSet::new();
    let mut seafaring_heritages = BTreeSet::new();
    racinate_with_technologies(
        elevations,
        koeppen_ids,
        adjacency,
        populated,
        agricultural_heritages,
        &mut rivergoing_heritages,
        &mut seafaring_heritages,
        heritages,
        new_heritage,
        travel_range,
    );
}

/// Group populated tiles into connected components and propagate `agricultural_heritages`,
/// `rivergoing_heritages`, and `seafaring_heritages` across admixed and branched heritages.
pub fn racinate_with_technologies<F>(
    elevations: &[f32],
    koeppen_ids: &[u8],
    adjacency: &[Vec<usize>],
    populated: &mut BTreeMap<usize, u32>,
    agricultural_heritages: &mut BTreeSet<u32>,
    rivergoing_heritages: &mut BTreeSet<u32>,
    seafaring_heritages: &mut BTreeSet<u32>,
    heritages: &mut BTreeMap<u32, Heritage>,
    mut new_heritage: F,
    travel_range: usize,
) where
    F: FnMut(Vec<u32>) -> Heritage,
{
    if populated.is_empty() {
        return;
    }

    let n = elevations.len();
    let mut dsu = DisjointSet::new(n);

    let mut is_pop = vec![false; n];
    let mut pop_entries: Vec<(usize, u32)> = Vec::with_capacity(populated.len());
    for (&t, &h) in populated.iter() {
        if t < n {
            is_pop[t] = true;
        }
        pop_entries.push((t, h));
    }

    let mut visited_stamp = vec![0u32; n];
    let mut stamp = 0u32;
    let mut curr_frontier: Vec<usize> = Vec::with_capacity(64);
    let mut next_frontier: Vec<usize> = Vec::with_capacity(64);

    for &(t, h) in &pop_entries {
        let is_ag = agricultural_heritages.contains(&h);
        let is_sea = seafaring_heritages.contains(&h);
        let eff_range = if is_sea { travel_range + 1 } else { travel_range };
        let passable_t = if is_sea && elevations[t] <= 0.0 {
            true
        } else {
            is_passable(elevations[t], koeppen_ids[t], is_ag)
        };
        if !passable_t || eff_range == 0 {
            continue;
        }

        stamp = stamp.wrapping_add(1);
        if stamp == 0 {
            visited_stamp.fill(0);
            stamp = 1;
        }
        visited_stamp[t] = stamp;
        curr_frontier.clear();
        curr_frontier.push(t);

        for _step in 0..eff_range {
            next_frontier.clear();
            for &u in &curr_frontier {
                for &nb in &adjacency[u] {
                    if visited_stamp[nb] != stamp {
                        let ok = if is_sea && elevations[nb] <= 0.0 {
                            true
                        } else {
                            is_passable(elevations[nb], koeppen_ids[nb], is_ag)
                        };
                        if ok {
                            visited_stamp[nb] = stamp;
                            if is_pop[nb] {
                                dsu.union(t, nb);
                            }
                            next_frontier.push(nb);
                        }
                    }
                }
            }
            if next_frontier.is_empty() {
                break;
            }
            std::mem::swap(&mut curr_frontier, &mut next_frontier);
        }
    }

    let mut races: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for (&t, _) in populated.iter() {
        let r = dsu.find(t);
        races.entry(r).or_default().insert(t);
    }

    // 1. If any geographically isolated race has >1 heritages, combine them into a new one (`racinatemethod.py:18-25`)
    for ts in races.values() {
        let hs: BTreeSet<u32> = ts.iter().map(|t| populated[t]).collect();
        if hs.len() > 1 {
            let parents: Vec<u32> = hs.iter().copied().collect();
            let h = new_heritage(parents);
            let new_id = h.id;
            if hs.iter().any(|old_id| agricultural_heritages.contains(old_id)) {
                agricultural_heritages.insert(new_id);
            }
            if hs.iter().any(|old_id| rivergoing_heritages.contains(old_id)) {
                rivergoing_heritages.insert(new_id);
            }
            if hs.iter().any(|old_id| seafaring_heritages.contains(old_id)) {
                seafaring_heritages.insert(new_id);
            }
            heritages.insert(new_id, h);
            for &t in ts {
                populated.insert(t, new_id);
            }
        }
    }

    // 2. If any heritage appears in >1 geographically isolated race, branch it into new ones (`racinatemethod.py:27-41`)
    let mut heritage_races: BTreeMap<u32, BTreeSet<usize>> = BTreeMap::new();
    for (&r, ts) in &races {
        let hs: BTreeSet<u32> = ts.iter().map(|t| populated[t]).collect();
        for h in hs {
            heritage_races.entry(h).or_default().insert(r);
        }
    }

    for (h, rs) in heritage_races {
        if rs.len() > 1 {
            let was_ag = agricultural_heritages.contains(&h);
            let was_riv = rivergoing_heritages.contains(&h);
            let was_sea = seafaring_heritages.contains(&h);
            for r in rs {
                let nh = new_heritage(vec![h]);
                let new_id = nh.id;
                if was_ag {
                    agricultural_heritages.insert(new_id);
                }
                if was_riv {
                    rivergoing_heritages.insert(new_id);
                }
                if was_sea {
                    seafaring_heritages.insert(new_id);
                }
                heritages.insert(new_id, nh);
                if let Some(ts) = races.get(&r) {
                    for &t in ts {
                        populated.insert(t, new_id);
                    }
                }
            }
        }
    }
}

/// Generate distinct orange-yellow RGB shades `[(255, g, 0), ...]` in `[0.0..1.0]`,
/// matching `racinationdisplay.genshades(n)` (`racinationdisplay.py:6-16`).
pub fn racination_shades(n: usize) -> Vec<[f32; 3]> {
    let mut g = 0.0f64;
    let mut dg = 255.0f64;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        g += dg;
        dg /= -2.0;
        out.push([1.0, (g / 255.0).clamp(0.0, 1.0) as f32, 0.0]);
    }
    out
}

#[derive(Clone)]
struct RacinationContinent {
    center: Vec3,
    avg_radius: f64,
    orienting_pt: Vec3,
    outer: SphericalPolygon,
    inner: SphericalPolygon,
}

impl RacinationContinent {
    fn new(center: Vec3, avg_radius: f64, rng: &mut Pcg64Mcg) -> Self {
        let mut orienting_pt = [0.0, 0.0, 0.0];
        let mut mini = 0;
        for i in 1..3 {
            if center[i].abs() < center[mini].abs() {
                mini = i;
            }
        }
        orienting_pt[mini] = if center[mini] < 0.0 { 1.0 } else { -1.0 };

        let mut outer_pts = Vec::with_capacity(16);
        for i in 0..16 {
            let th = (i as f64) * PI / 8.0;
            let r = avg_radius * rng.uniform(0.9, 1.1);
            let p = rotate(rotate(center, orienting_pt, r), center, th);
            outer_pts.push(normal(p));
        }
        let outer = SphericalPolygon::new_with_interior(&outer_pts, center);

        let mut inner_pts = Vec::with_capacity(16);
        for i in 0..16 {
            let th = (i as f64) * PI / 8.0;
            let r = 0.65 * avg_radius * rng.uniform(0.9, 1.1);
            let p = rotate(rotate(center, orienting_pt, r), center, th);
            inner_pts.push(normal(p));
        }
        let inner = SphericalPolygon::new_with_interior(&inner_pts, center);

        Self {
            center,
            avg_radius,
            orienting_pt,
            outer,
            inner,
        }
    }
}

/// Standalone Racination sandbox (`racinationsimulation.py`, `racinationdisplay.py`, `racinationpresenter.py`).
pub struct RacinationSandbox {
    grid: SphericalGrid,
    adjacency: Vec<Vec<usize>>,
    elevations: Vec<f32>,
    koeppen_ids: Vec<u8>,
    continents: Vec<RacinationContinent>,
    populated: BTreeMap<usize, u32>,
    heritages: BTreeMap<u32, Heritage>,
    next_heritage_id: u32,
    travel_range: usize,
    isolations: u32,
    rng: Pcg64Mcg,
}

impl RacinationSandbox {
    pub fn new(travel_range: usize, seed: u128) -> Self {
        let grid = SphericalGrid::with_size(6);
        let adjacency = grid.adjacency();
        let n = grid.faces.len();
        let mut s = Self {
            grid,
            adjacency,
            elevations: vec![-5.0; n],
            koeppen_ids: vec![0; n],
            continents: Vec::new(),
            populated: BTreeMap::new(),
            heritages: BTreeMap::new(),
            next_heritage_id: 1,
            travel_range,
            isolations: 0,
            rng: Pcg64Mcg::new(seed),
        };
        s.reset();
        s
    }

    pub fn reset(&mut self) {
        let raw_p1 = [
            self.rng.uniform(-1.0, 1.0),
            self.rng.uniform(-1.0, 1.0),
            self.rng.uniform(-1.0, 1.0),
        ];
        let p1 = normal(raw_p1);
        let p2 = [-p1[0], -p1[1], -p1[2]];

        self.continents = vec![
            RacinationContinent::new(p1, 0.75, &mut self.rng),
            RacinationContinent::new(p2, 0.5, &mut self.rng),
        ];

        self.populated.clear();
        self.heritages.clear();
        self.next_heritage_id = 1;
        self.isolations = 0;

        let init_id = self.next_heritage_id;
        self.next_heritage_id += 1;
        self.heritages.insert(
            init_id,
            Heritage {
                id: init_id,
                name: "people_1".to_string(),
                ancestry: Vec::new(),
            },
        );

        let n = self.grid.faces.len();
        for t in 0..n {
            let v = normal(self.grid.faces[t].0);
            if self.continents.iter().any(|c| c.outer.contains(v)) {
                self.elevations[t] = 1.0;
                self.koeppen_ids[t] = 8; // Cw
                if !self.continents.iter().any(|c| c.inner.contains(v)) {
                    self.populated.insert(t, init_id);
                }
            } else {
                self.elevations[t] = -5.0;
                self.koeppen_ids[t] = 0;
            }
        }
    }

    pub fn set_travel_range(&mut self, travel_range: usize) {
        self.travel_range = travel_range;
    }

    /// Carve impassable desert (`BW = 5`) or tundra (`ET = 13`) swaths across each continent
    /// and run `racinate` (`racinationsimulation.py:81-92`).
    pub fn isolate(&mut self) {
        let n = self.grid.faces.len();
        for c in &self.continents {
            let th0 = self.rng.uniform(0.0, PI);
            let angles = [th0, th0 + PI / 12.0, th0 + PI, th0 + PI + PI / 12.0];
            let swath_pts: Vec<Vec3> = angles
                .iter()
                .map(|&th| {
                    normal(rotate(
                        rotate(c.center, c.orienting_pt, 1.2 * c.avg_radius),
                        c.center,
                        th,
                    ))
                })
                .collect();
            let swath = SphericalPolygon::new_with_interior(&swath_pts, c.center);
            let k = if (self.rng.next_u64() & 1) == 0 {
                5 // BW
            } else {
                13 // ET
            };
            for t in 0..n {
                let v = normal(self.grid.faces[t].0);
                if swath.contains(v) && c.outer.contains(v) {
                    self.koeppen_ids[t] = k;
                    self.populated.remove(&t);
                }
            }
        }

        let mut ag = BTreeSet::new();
        let mut next_id = self.next_heritage_id;
        racinate(
            &self.elevations,
            &self.koeppen_ids,
            &self.adjacency,
            &mut self.populated,
            &mut ag,
            &mut self.heritages,
            |ancestry| {
                let id = next_id;
                next_id += 1;
                Heritage {
                    id,
                    name: format!("people_{id}"),
                    ancestry,
                }
            },
            self.travel_range,
        );
        self.next_heritage_id = next_id;
        self.isolations += 1;
    }

    pub fn grid(&self) -> &SphericalGrid {
        &self.grid
    }

    pub fn tile_count(&self) -> usize {
        self.elevations.len()
    }

    pub fn peoples(&self) -> usize {
        let unique: BTreeSet<u32> = self.populated.values().copied().collect();
        unique.len()
    }

    pub fn populated_count(&self) -> usize {
        self.populated.len()
    }

    pub fn isolations(&self) -> u32 {
        self.isolations
    }

    pub fn tile_elevations(&self) -> &[f32] {
        &self.elevations
    }

    pub fn tile_koeppen_ids(&self) -> &[u8] {
        &self.koeppen_ids
    }

    /// Compute per-tile RGB colors (`3 * tile_count` floats in `[0.0..1.0]`) matching `racinationdisplay.py:18-25`:
    /// - Sea (`elevation <= 0`): `(0, 0, 255)`
    /// - Populated land: `shades[i]` (`(255, g, 0)`) for the tile's distinct race
    /// - Unpopulated land: Köppen classification color (`Cw`, `BW`, `ET`)
    pub fn tile_colors_rgb(&self) -> Vec<f32> {
        let unique_races: Vec<u32> = self
            .populated
            .values()
            .copied()
            .collect::<BTreeSet<u32>>()
            .into_iter()
            .collect();
        let shades = racination_shades(unique_races.len());
        let mut race_to_shade: BTreeMap<u32, [f32; 3]> = BTreeMap::new();
        for (i, &r) in unique_races.iter().enumerate() {
            race_to_shade.insert(r, shades[i]);
        }

        let n = self.elevations.len();
        let mut out = Vec::with_capacity(n * 3);
        for t in 0..n {
            if self.elevations[t] <= 0.0 {
                out.extend_from_slice(&[0.0, 0.0, 1.0]);
            } else if let Some(&r) = self.populated.get(&t) {
                let c = race_to_shade.get(&r).copied().unwrap_or([1.0, 1.0, 0.0]);
                out.extend_from_slice(&c);
            } else {
                // `koeppencolor.py`:
                // Cw (8) -> (150, 255, 150)
                // BW (5) -> (255, 0, 0)
                // ET (13) -> (178, 178, 178)
                let rgb = match self.koeppen_ids[t] {
                    5 => [1.0, 0.0, 0.0],
                    13 => [178.0 / 255.0, 178.0 / 255.0, 178.0 / 255.0],
                    _ => [150.0 / 255.0, 1.0, 150.0 / 255.0],
                };
                out.extend_from_slice(&rgb);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_racination_sandbox() {
        let mut sim = RacinationSandbox::new(6, 42);
        assert_eq!(sim.tile_count(), 7292);
        assert_eq!(sim.peoples(), 1);
        sim.isolate();
        assert!(sim.peoples() >= 2, "Expected at least 2 distinct races after isolation");
    }
}
