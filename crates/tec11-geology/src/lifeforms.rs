use tec11_climate::{ClimateModel, EARTH_SIZE6_ELEVATIONS_M};
use tec11_math_grid::{normal, PointTree, SphericalGrid, Vec3};
use tec11_proto::PlanetSnapshotExt;

use crate::tectonics::TectonicsSimulation;

const ANIMALS_TSV: &str = include_str!("../data/animals.tsv");
const PLANTS_TSV: &str = include_str!("../data/plants.tsv");

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SpeciesKind {
    Animal,
    Plant,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ValueRange {
    pub min: f32,
    pub max: f32,
}

impl ValueRange {
    #[inline]
    pub const fn new(min: f32, max: f32) -> Self {
        Self { min, max }
    }

    #[inline]
    pub fn contains(self, v: f32) -> bool {
        v >= self.min && v <= self.max
    }

    #[inline]
    pub fn map(self, f: impl Fn(f32) -> f32) -> Self {
        Self {
            min: f(self.min),
            max: f(self.max),
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct ClimateTolerance {
    pub temp_norm: ValueRange,
    pub precip_norm: ValueRange,
    pub sun_norm: ValueRange,
}

impl ClimateTolerance {
    #[inline]
    pub fn in_range(&self, temp: f32, precip: f32, sun: f32) -> bool {
        self.temp_norm.contains(temp)
            && self.precip_norm.contains(precip)
            && self.sun_norm.contains(sun)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct TileSeasonPresence {
    pub tile: usize,
    pub season_mask: u8,
}

#[derive(Clone, Debug)]
pub struct SettledSpecies {
    pub id: usize,
    pub kind: SpeciesKind,
    pub name: String,
    /// Tile presence entries where `season_mask != 0`.
    pub tile_seasons: Vec<TileSeasonPresence>,
}

#[derive(Clone, Debug)]
pub struct SettledBiosphere {
    pub species: Vec<SettledSpecies>,
    /// Per-season (`0..8`), per-tile animal count.
    pub seasonal_animal_counts: Vec<Vec<u16>>,
    /// Per-season (`0..8`), per-tile plant count.
    pub seasonal_plant_counts: Vec<Vec<u16>>,
    /// Per-tile union of all settled species indices (`0..species.len()`) across all seasons,
    /// matching `HistorySimulation.tilespecies` (`historysimulation.py:957-964`).
    pub tile_all_species: Vec<Vec<usize>>,
}

/// Extract a `"<key>": "<low>-<high>"` range from a JSON line in `animals.tsv` / `plants.tsv`.
fn extract_json_range(json_str: &str, key: &str) -> Option<ValueRange> {
    let needle = format!("\"{key}\":");
    let pos = json_str.find(&needle)?;
    let rest = &json_str[pos + needle.len()..];
    let first_quote = rest.find('"')?;
    let after_first = &rest[first_quote + 1..];
    let second_quote = after_first.find('"')?;
    let range_str = after_first[..second_quote].trim();

    // Match `(-?[0-9]+)-(-?[0-9]+)`: find the hyphen separating low and high (skipping leading '-')
    let sep = (1..range_str.len()).find(|&i| range_str.as_bytes()[i] == b'-')?;
    let min: f32 = range_str[..sep].parse().ok()?;
    let max: f32 = range_str[sep + 1..].parse().ok()?;
    Some(ValueRange::new(min, max))
}

fn normalize_temp(celsius_range: ValueRange) -> ValueRange {
    celsius_range.map(|c| (c + 25.0) / 75.0)
}

fn normalize_precip(cm_range: ValueRange, num_seasons: f32) -> ValueRange {
    cm_range.map(|cm| cm / num_seasons / 30.0)
}

fn normalize_sun(hours_range: ValueRange, num_seasons: f32) -> ValueRange {
    hours_range.map(|h| h / num_seasons / 400.0)
}

/// Settle all flora and fauna from `animals.tsv` and `plants.tsv` onto the given planet's
/// `elevations` and `climate_model`, matching `lifeformsmethod.settle` and `species.py`.
pub fn settle_species_on_climate(
    elevations: &[f32],
    climate_model: &ClimateModel,
) -> SettledBiosphere {
    let n = elevations.len();
    let num_seasons = climate_model.season_count().clamp(1, 8);
    let num_seasons_f = num_seasons as f32;
    let all_seasons_mask: u8 = if num_seasons >= 8 {
        0xFF
    } else {
        (1u8 << num_seasons) - 1
    };

    let land_tiles: Vec<usize> = (0..n).filter(|&t| elevations[t] > 0.0).collect();

    // Flatten seasonal climate arrays `[season * n + tile]` for cache-friendly inner loops
    let mut temp_s = vec![0.0f32; num_seasons * n];
    let mut precip_s = vec![0.0f32; num_seasons * n];
    let mut sun_s = vec![0.0f32; num_seasons * n];
    for s in 0..num_seasons {
        if let Some(snap) = climate_model.season_snapshot(s) {
            let base = s * n;
            temp_s[base..base + n].copy_from_slice(&snap.temperature);
            precip_s[base..base + n].copy_from_slice(&snap.precipitation);
            sun_s[base..base + n].copy_from_slice(&snap.insolation);
        }
    }

    let mut settled = Vec::new();
    let mut seasonal_animal_counts = vec![vec![0u16; n]; num_seasons];
    let mut seasonal_plant_counts = vec![vec![0u16; n]; num_seasons];
    let mut tile_all_species = vec![Vec::new(); n];

    // 1. Settle Animals (`findregions`, `findhibernationregions`, `findmigratorypatterns`)
    for line in ANIMALS_TSV.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((name_part, json_part)) = line.split_once('\t') else {
            continue;
        };
        let Some(t_range) = extract_json_range(json_part, "temperatureRange") else {
            continue;
        };
        let Some(p_range) = extract_json_range(json_part, "precipitationRange") else {
            continue;
        };
        let tol = ClimateTolerance {
            temp_norm: normalize_temp(t_range),
            precip_norm: normalize_precip(p_range, num_seasons_f),
            sun_norm: ValueRange::new(0.0, 1.0),
        };

        // Strategy 1: Year-round (`findregions`: all seasons in range)
        let year_round_tiles: Vec<TileSeasonPresence> = land_tiles
            .iter()
            .copied()
            .filter(|&t| {
                (0..num_seasons).all(|s| {
                    let idx = s * n + t;
                    tol.in_range(temp_s[idx], precip_s[idx], sun_s[idx])
                })
            })
            .map(|tile| TileSeasonPresence {
                tile,
                season_mask: all_seasons_mask,
            })
            .collect();

        let chosen_tiles = if !year_round_tiles.is_empty() {
            Some(year_round_tiles)
        } else {
            // Strategy 2: Hibernation (`findhibernationregions`)
            let winter_range =
                ValueRange::new((tol.temp_norm.min - 0.01).clamp(0.0, 0.3), tol.temp_norm.min);
            let hib_tiles: Vec<TileSeasonPresence> = land_tiles
                .iter()
                .copied()
                .filter_map(|t| {
                    let (mask, winter_ok) =
                        (0..num_seasons).fold((0u8, true), |(mask, winter_ok), s| {
                            let idx = s * n + t;
                            if tol.in_range(temp_s[idx], precip_s[idx], sun_s[idx]) {
                                (mask | (1u8 << s), winter_ok)
                            } else {
                                (mask, winter_ok && winter_range.contains(temp_s[idx]))
                            }
                        });
                    (winter_ok && mask != 0 && mask != all_seasons_mask).then_some(
                        TileSeasonPresence {
                            tile: t,
                            season_mask: mask,
                        },
                    )
                })
                .collect();
            if !hib_tiles.is_empty() {
                Some(hib_tiles)
            } else {
                // Strategy 3: Migration (`findmigratorypatterns`)
                let mut present_masks = [false; 256];
                let seasonal_tiles: Vec<TileSeasonPresence> = land_tiles
                    .iter()
                    .copied()
                    .filter_map(|t| {
                        let mask = (0..num_seasons).fold(0u8, |acc, s| {
                            let idx = s * n + t;
                            if tol.in_range(temp_s[idx], precip_s[idx], sun_s[idx]) {
                                acc | (1u8 << s)
                            } else {
                                acc
                            }
                        });
                        if mask != 0 && mask != all_seasons_mask {
                            present_masks[mask as usize] = true;
                            Some(TileSeasonPresence {
                                tile: t,
                                season_mask: mask,
                            })
                        } else {
                            None
                        }
                    })
                    .collect();
                let mut valid_mig_mask = [false; 256];
                for m1 in 1..(all_seasons_mask as usize) {
                    if present_masks[m1]
                        && (1..(all_seasons_mask as usize))
                            .any(|m2| present_masks[m2] && ((m1 | m2) as u8) == all_seasons_mask)
                    {
                        valid_mig_mask[m1] = true;
                    }
                }
                let mig_tiles: Vec<TileSeasonPresence> = seasonal_tiles
                    .into_iter()
                    .filter(|p| valid_mig_mask[p.season_mask as usize])
                    .collect();
                (!mig_tiles.is_empty()).then_some(mig_tiles)
            }
        };

        if let Some(tile_seasons) = chosen_tiles {
            let sp_id = settled.len();
            for &presence in &tile_seasons {
                tile_all_species[presence.tile].push(sp_id);
                for s in 0..num_seasons {
                    if (presence.season_mask & (1u8 << s)) != 0 {
                        seasonal_animal_counts[s][presence.tile] =
                            seasonal_animal_counts[s][presence.tile].saturating_add(1);
                    }
                }
            }
            settled.push(SettledSpecies {
                id: sp_id,
                kind: SpeciesKind::Animal,
                name: name_part.trim().to_string(),
                tile_seasons,
            });
        }
    }

    // 2. Settle Plants (`findseasonalregions`)
    for line in PLANTS_TSV.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((name_part, json_part)) = line.split_once('\t') else {
            continue;
        };
        let Some(t_range) = extract_json_range(json_part, "temperatureRange") else {
            continue;
        };
        let Some(p_range) = extract_json_range(json_part, "precipitationRange") else {
            continue;
        };
        let Some(s_range) = extract_json_range(json_part, "sunlightRange") else {
            continue;
        };
        let tol = ClimateTolerance {
            temp_norm: normalize_temp(t_range),
            precip_norm: normalize_precip(p_range, num_seasons_f),
            sun_norm: normalize_sun(s_range, num_seasons_f),
        };

        let plant_tiles: Vec<TileSeasonPresence> = land_tiles
            .iter()
            .copied()
            .filter_map(|t| {
                let mask = (0..num_seasons).fold(0u8, |acc, s| {
                    let idx = s * n + t;
                    if tol.in_range(temp_s[idx], precip_s[idx], sun_s[idx]) {
                        acc | (1u8 << s)
                    } else {
                        acc
                    }
                });
                (mask != 0).then_some(TileSeasonPresence {
                    tile: t,
                    season_mask: mask,
                })
            })
            .collect();

        if !plant_tiles.is_empty() {
            let sp_id = settled.len();
            for &presence in &plant_tiles {
                tile_all_species[presence.tile].push(sp_id);
                for s in 0..num_seasons {
                    if (presence.season_mask & (1u8 << s)) != 0 {
                        seasonal_plant_counts[s][presence.tile] =
                            seasonal_plant_counts[s][presence.tile].saturating_add(1);
                    }
                }
            }
            settled.push(SettledSpecies {
                id: sp_id,
                kind: SpeciesKind::Plant,
                name: name_part.trim().to_string(),
                tile_seasons: plant_tiles,
            });
        }
    }

    SettledBiosphere {
        species: settled,
        seasonal_animal_counts,
        seasonal_plant_counts,
        tile_all_species,
    }
}

/// Standalone Lifeforms sandbox (`lifeformssimulation.py`, `lifeformsdisplay.py`, `lifeformspresenter.py`).
pub struct LifeformsSandbox {
    grid: SphericalGrid,
    tree: PointTree,
    elevations: Vec<f32>,
    felsities: Vec<f32>,
    climate_model: ClimateModel,
    biosphere: SettledBiosphere,
    custom_geoticks: Option<u32>,
    custom_land_percent: Option<u32>,
}

impl LifeformsSandbox {
    pub fn new_earth(glaciation: f64) -> Self {
        let grid = SphericalGrid::with_size(6);
        let indexed_pts: Vec<(Vec3, usize)> = grid
            .faces
            .iter()
            .enumerate()
            .map(|(i, (c, _))| (*c, i))
            .collect();
        let tree = PointTree::new(&indexed_pts);
        let elevations: Vec<f32> = EARTH_SIZE6_ELEVATIONS_M
            .iter()
            .map(|&m| {
                if m > 0 {
                    ((m as f32) / 900.0).min(10.0)
                } else {
                    0.0
                }
            })
            .collect();
        let felsities = vec![0.6f32; elevations.len()];
        let climate_model =
            ClimateModel::with_elevations(grid.clone(), elevations.clone(), 3, glaciation);
        let biosphere = settle_species_on_climate(&elevations, &climate_model);

        Self {
            grid,
            tree,
            elevations,
            felsities,
            climate_model,
            biosphere,
            custom_geoticks: None,
            custom_land_percent: None,
        }
    }

    pub fn load_earth(&mut self) {
        *self = Self::new_earth(self.climate_model.glaciation());
    }

    pub fn load_snapshot_bytes(&mut self, bytes: &[u8]) -> bool {
        let Ok(snap) = tec11_proto::PlanetSnapshot::decode_bytes(bytes) else {
            return false;
        };
        let Some(tec_sim) = TectonicsSimulation::from_snapshot(&snap, 42) else {
            return false;
        };
        let grid = tec_sim.grid().clone();
        let indexed_pts: Vec<(Vec3, usize)> = grid
            .faces
            .iter()
            .enumerate()
            .map(|(i, (c, _))| (*c, i))
            .collect();
        let tree = PointTree::new(&indexed_pts);
        let elevations: Vec<f32> = tec_sim
            .tiles()
            .iter()
            .map(|t| t.elevation().max(0.0))
            .collect();
        let felsities: Vec<f32> = tec_sim.tiles().iter().map(|t| t.top_felsity()).collect();
        let glaciation = self.climate_model.glaciation();
        let climate_model =
            ClimateModel::with_elevations(grid.clone(), elevations.clone(), 3, glaciation);
        let biosphere = settle_species_on_climate(&elevations, &climate_model);

        self.grid = grid;
        self.tree = tree;
        self.elevations = elevations;
        self.felsities = felsities;
        self.climate_model = climate_model;
        self.biosphere = biosphere;
        self.custom_geoticks = Some(tec_sim.geoticks());
        self.custom_land_percent = Some(tec_sim.land_percent());
        true
    }

    pub fn set_glaciation(&mut self, glaciation: f64) {
        let g = glaciation.clamp(0.0, 1.0);
        if (self.climate_model.glaciation() - g).abs() < 1e-6 {
            return;
        }
        self.climate_model
            .set_params(self.climate_model.cells(), g);
        self.biosphere = settle_species_on_climate(&self.elevations, &self.climate_model);
    }

    pub fn grid(&self) -> &SphericalGrid {
        &self.grid
    }

    pub fn tile_count(&self) -> usize {
        self.elevations.len()
    }

    pub fn season_count(&self) -> usize {
        self.climate_model.season_count()
    }

    pub fn total_settled_species(&self) -> usize {
        self.biosphere.species.len()
    }

    pub fn settled_animal_count(&self) -> usize {
        self.biosphere
            .species
            .iter()
            .filter(|s| s.kind == SpeciesKind::Animal)
            .count()
    }

    pub fn settled_plant_count(&self) -> usize {
        self.biosphere
            .species
            .iter()
            .filter(|s| s.kind == SpeciesKind::Plant)
            .count()
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

    pub fn tile_elevations(&self) -> &[f32] {
        &self.elevations
    }

    pub fn tile_felsities(&self) -> &[f32] {
        &self.felsities
    }

    pub fn tile_koeppen_ids(&self) -> Vec<u8> {
        self.climate_model.koeppen_ids()
    }

    pub fn tile_animal_counts(&self, season_idx: usize) -> &[u16] {
        let s = season_idx.min(self.biosphere.seasonal_animal_counts.len().saturating_sub(1));
        &self.biosphere.seasonal_animal_counts[s]
    }

    pub fn tile_plant_counts(&self, season_idx: usize) -> &[u16] {
        let s = season_idx.min(self.biosphere.seasonal_plant_counts.len().saturating_sub(1));
        &self.biosphere.seasonal_plant_counts[s]
    }

    pub fn nearest_tile(&self, x: f64, y: f64, z: f64) -> usize {
        self.tree.nearest(normal([x, y, z]))
    }

    /// Return a newline-separated list of `"<species_id>\t<kind>\t<name>"` for all settled species
    /// present on `tile_idx` during `season_idx` matching `attribute` (`"life"`, `"animals"`, `"plants"`).
    pub fn species_on_tile_tsv(&self, tile_idx: usize, season_idx: usize, attribute: &str) -> String {
        if tile_idx >= self.elevations.len() {
            return String::new();
        }
        let s_bit = 1u8 << (season_idx.min(7) as u8);
        let include_animals = attribute != "plants";
        let include_plants = attribute != "animals";

        let mut list: Vec<&SettledSpecies> = self.biosphere.tile_all_species[tile_idx]
            .iter()
            .filter_map(|&sp_id| {
                let sp = &self.biosphere.species[sp_id];
                let kind_ok = match sp.kind {
                    SpeciesKind::Animal => include_animals,
                    SpeciesKind::Plant => include_plants,
                };
                if !kind_ok {
                    return None;
                }
                let active_in_season = sp
                    .tile_seasons
                    .binary_search_by_key(&tile_idx, |p| p.tile)
                    .ok()
                    .is_some_and(|idx| (sp.tile_seasons[idx].season_mask & s_bit) != 0);
                active_in_season.then_some(sp)
            })
            .collect();

        list.sort_by(|a, b| a.name.cmp(&b.name));
        list.into_iter()
            .map(|sp| {
                let kind_code = match sp.kind {
                    SpeciesKind::Animal => "A",
                    SpeciesKind::Plant => "P",
                };
                format!("{}\t{}\t{}\n", sp.id, kind_code, sp.name)
            })
            .collect()
    }

    /// Return a per-tile `u8` presence mask (`1` if `species_id` is present on tile `t` in `season_idx`, else `0`).
    pub fn species_presence_mask(&self, species_id: usize, season_idx: usize) -> Vec<u8> {
        let mut mask = vec![0u8; self.elevations.len()];
        let Some(sp) = self.biosphere.species.get(species_id) else {
            return mask;
        };
        let s_bit = 1u8 << (season_idx.min(7) as u8);
        for &presence in &sp.tile_seasons {
            if (presence.season_mask & s_bit) != 0 {
                mask[presence.tile] = 1;
            }
        }
        mask
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lifeforms_sandbox_earth() {
        let sim = LifeformsSandbox::new_earth(0.5);
        assert_eq!(sim.tile_count(), 7292);
        assert!(
            sim.settled_animal_count() > 200,
            "Expected >200 settled animal species on Earth, got {}",
            sim.settled_animal_count()
        );
        assert!(
            sim.settled_plant_count() > 300,
            "Expected >300 settled plant species on Earth, got {}",
            sim.settled_plant_count()
        );
    }
}
