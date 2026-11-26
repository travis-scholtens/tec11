use tec11_biosphere::{
    settle_species_on_climate, SettledBiosphere, SettledSpecies, SpeciesKind,
};
use tec11_climate::{ClimateModel, EARTH_GLACIATION, EARTH_SIZE6_ELEVATIONS_M};
use tec11_geology::tile::tiles_from_snapshot;
use tec11_math_grid::{normal, PointTree, SphericalGrid, Vec3};
use tec11_proto::PlanetSnapshotExt;
use tec11_topography::PlanetaryGeography;

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
        *self = Self::new_earth(EARTH_GLACIATION);
    }

    pub fn load_snapshot_bytes(&mut self, bytes: &[u8]) -> bool {
        let Ok(snap) = tec11_proto::PlanetSnapshot::decode_bytes(bytes) else {
            return false;
        };
        let Some((grid, tiles, geoticks)) = tiles_from_snapshot(&snap) else {
            return false;
        };
        let n = grid.faces.len();
        let land_cnt = tiles.iter().filter(|t| t.elevation() > 0.0).count();
        let land_pct = ((land_cnt as f64) * 100.0 / (n.max(1) as f64)).round() as u32;
        let indexed_pts: Vec<(Vec3, usize)> = grid
            .faces
            .iter()
            .enumerate()
            .map(|(i, (c, _))| (*c, i))
            .collect();
        let tree = PointTree::new(&indexed_pts);
        let elevations: Vec<f32> = tiles
            .iter()
            .map(|t| t.elevation().max(0.0))
            .collect();
        let felsities: Vec<f32> = tiles.iter().map(|t| t.top_felsity()).collect();
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
        self.custom_geoticks = Some(geoticks);
        self.custom_land_percent = Some(land_pct);
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

    pub fn geography(&self) -> &PlanetaryGeography {
        &self.biosphere.geography
    }

    pub fn tile_region_name(&self, tile_idx: usize) -> String {
        if let Some(cname) = self.biosphere.geography.continent_name(tile_idx) {
            format!("Continent: {cname}")
        } else if let Some(oname) = self.biosphere.geography.ocean_name(tile_idx) {
            format!("Ocean: {oname}")
        } else {
            String::new()
        }
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

        list.sort_by(|a, b| b.trophic_level.cmp(&a.trophic_level).then_with(|| a.name.cmp(&b.name)));
        list.into_iter()
            .map(|sp| {
                let kind_code = match (sp.kind, sp.trophic_level) {
                    (SpeciesKind::Animal, 4) => "A-Apex",
                    (SpeciesKind::Animal, 3) => "A-Meso",
                    (SpeciesKind::Animal, _) => "A-Herb",
                    (SpeciesKind::Plant, _) => "P",
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
        let sim = LifeformsSandbox::new_earth(EARTH_GLACIATION);
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

    #[test]
    fn test_trophic_food_web_and_apex_predator_competition() {
        let sim = LifeformsSandbox::new_earth(EARTH_GLACIATION);
        let bio = &sim.biosphere;
        let level_1 = bio.species.iter().filter(|s| s.trophic_level == 1).count();
        let level_2 = bio.species.iter().filter(|s| s.trophic_level == 2).count();
        let level_3 = bio.species.iter().filter(|s| s.trophic_level == 3).count();
        let level_4 = bio.species.iter().filter(|s| s.trophic_level == 4).count();
        assert!(level_1 > 300, "Expected >300 plants (trophic level 1), got {level_1}");
        assert!(level_2 > 100, "Expected >100 herbivores/insects (trophic level 2), got {level_2}");
        assert!(level_3 > 50, "Expected >50 mesopredators/omnivores (trophic level 3), got {level_3}");
        assert!(level_4 > 15, "Expected >15 apex predators (trophic level 4), got {level_4}");

        // Verify that on every tile, the number of mammalian apex predators is bounded by emergent prey capacity (<= 3)
        // and that every tile with an apex predator also has lower-trophic animal prey present.
        for t in 0..sim.tile_count() {
            let mut mam_apex_on_tile = 0usize;
            let mut any_apex_on_tile = 0usize;
            let mut non_apex_animals_on_tile = 0usize;
            for &sp_id in &bio.tile_all_species[t] {
                let sp = &bio.species[sp_id];
                if sp.kind == SpeciesKind::Animal {
                    if sp.trophic_level == 4 {
                        any_apex_on_tile += 1;
                        if !sp.sealife && (sp.subkind == "large mammal" || sp.subkind == "giant mammal") {
                            mam_apex_on_tile += 1;
                        }
                    } else {
                        non_apex_animals_on_tile += 1;
                    }
                }
            }
            assert!(
                mam_apex_on_tile <= 3,
                "Tile {t} has {mam_apex_on_tile} mammalian apex predators (expected <= 3)"
            );
            if any_apex_on_tile > 0 {
                assert!(
                    non_apex_animals_on_tile >= 2,
                    "Tile {t} has {any_apex_on_tile} apex predators but only {non_apex_animals_on_tile} prey species"
                );
            }
        }
    }

    #[test]
    fn test_allopatric_continental_varieties() {
        let sim = LifeformsSandbox::new_earth(EARTH_GLACIATION);
        let bio = &sim.biosphere;
        assert!(
            bio.geography.continents.len() >= 4,
            "Expected >= 4 named continents on Earth, got {}",
            bio.geography.continents.len()
        );

        let prefixes: Vec<&str> = bio
            .geography
            .continents
            .iter()
            .map(|c| c.adjectival_prefix.as_str())
            .collect();

        let mut prefixed_species_count = 0usize;
        let mut endemic_terrestrial_count = 0usize;
        for sp in &bio.species {
            if !sp.sealife {
                let has_cont_prefix = prefixes
                    .iter()
                    .any(|p| sp.name.starts_with(&format!("{p} ")));
                if has_cont_prefix {
                    prefixed_species_count += 1;
                } else {
                    endemic_terrestrial_count += 1;
                }
                // Verify that every terrestrial SettledSpecies (whether a split continental variety
                // or a single-continent endemic) is confined to at most 1 named continent!
                let mut conts = std::collections::BTreeSet::new();
                for &presence in &sp.tile_seasons {
                    if let Some(cid) = bio.geography.tile_continent[presence.tile] {
                        conts.insert(cid);
                    }
                }
                assert!(
                    conts.len() <= 1,
                    "Terrestrial species '{}' spans multiple continents {:?} without allopatric split",
                    sp.name,
                    conts
                );
            }
        }
        assert!(
            prefixed_species_count > 500,
            "Expected > 500 continent-prefixed allopatric varieties on Earth, got {prefixed_species_count}"
        );
        assert!(
            endemic_terrestrial_count >= 5,
            "Expected >= 5 single-continent endemic species retaining generic names, got {endemic_terrestrial_count}"
        );
    }
}
