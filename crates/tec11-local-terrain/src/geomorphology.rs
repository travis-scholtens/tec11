//! Six composable physical geomorphology operators (`FR-004`, `FR-006`, `US2`):
//! 1. Laterally continuous stratigraphy & structural warping (delegated to `stratigraphy.rs`)
//! 2. Differential erosion by `Rock::toughness` (stepped plateaus / ridge-and-valley hogbacks)
//! 3. Glacial (U-valleys/cirques/horns) vs. fluvial (V-gorges/floodplains) carving
//! 4. Soluble-rock karst dissolution (sinkholes & 3D underground caverns in high-`calcity` beds)
//! 5. Surficial mantle & depositional overlays (persisted `soil_depth_m`/`soil_fertility`, dunes, peat, talus)
//! 6. Coastal wave & tide beveling (cliffs/platforms vs. beaches/tidal flats around sea level)

use tec11_geology::rock::RockKind;

use crate::anchor::LocalZoomAnchor;
use crate::materials::{cell_flags, HexPrismCell, MaterialId};
use crate::rivers::{query_rivers_with_min_width, RiverQueryResult, RiverSplineSegment};
use crate::stratigraphy::{
    fbm2, value_noise3, InterpolatedLocalContext, LocalStratumSlice, RegionalStratigraphyField,
};

/// Evaluated surface & column summary at a single tangent-plane point `(x_m, y_m)`.
#[derive(Clone, Debug)]
pub struct ColumnSynthesis {
    pub ctx: InterpolatedLocalContext,
    pub surface_elev_m: f32,
    pub bedrock_top_m: f32,
    pub water_elev_m: Option<f32>,
    pub is_ocean: bool,
    pub is_river: bool,
    pub surficial_material: MaterialId,
    pub soil_depth_m: f32,
    pub ice_thickness_m: f32,
    pub strata: Vec<LocalStratumSlice>,
    pub flora_flag: u8,
}

/// Evaluate all 6 physical geomorphology operators at `(x_m, y_m)` at true physical river width.
pub fn synthesize_column(
    anchor: &LocalZoomAnchor,
    strat: &RegionalStratigraphyField,
    rivers: &[RiverSplineSegment],
    x_m: f64,
    y_m: f64,
) -> ColumnSynthesis {
    synthesize_column_with_river_min_half_w(anchor, strat, rivers, x_m, y_m, 0.0)
}

/// Evaluate all 6 physical geomorphology operators at `(x_m, y_m)` with optional coarse-scale
/// minimum river channel half-width (`min_river_half_w_m`) so rivers read continuously at Levels 0–2.
pub fn synthesize_column_with_river_min_half_w(
    anchor: &LocalZoomAnchor,
    strat: &RegionalStratigraphyField,
    rivers: &[RiverSplineSegment],
    x_m: f64,
    y_m: f64,
    min_river_half_w_m: f32,
) -> ColumnSynthesis {
    let ctx = strat.sample_context(anchor, x_m, y_m);
    let seed = anchor.world_seed;
    let sea_m = anchor.sea_level_m as f32;

    // 1. Structural & regional macro-relief
    let m_norm = (ctx.mountainosity / 10.0).clamp(0.0, 1.5);
    let macro_noise = fbm2(x_m / 2600.0, y_m / 2600.0, seed ^ 0x1011, 4);
    let meso_noise = fbm2(x_m / 420.0, y_m / 420.0, seed ^ 0x2022, 3);
    let micro_noise = fbm2(x_m / 65.0, y_m / 65.0, seed ^ 0x3033, 2);

    // Ridge-shaped relief for active orogens (1 - |noise|) vs rolling relief for lowlands
    let ridge_raw = 1.0 - fbm2(x_m / 1100.0, y_m / 1100.0, seed ^ 0x4044, 3).abs();
    let ridge_relief = (ridge_raw * ridge_raw - 0.35) * (140.0 * m_norm);

    let mut elev_m = ctx.base_elevation_m
        + macro_noise * (35.0 + 180.0 * m_norm)
        + meso_noise * (8.0 + 55.0 * m_norm)
        + micro_noise * (1.5 + 10.0 * m_norm)
        + ridge_relief;

    // Impact crater rim & bowl topography if impact_scar > 0.25
    if ctx.impact_scar > 0.25 {
        let r_m = (x_m * x_m + y_m * y_m).sqrt() as f32;
        let crater_r = 1800.0f32;
        let rn = r_m / crater_r;
        if rn < 1.0 {
            elev_m -= 90.0 * ctx.impact_scar * (1.0 - rn * rn);
        } else if rn < 1.35 {
            let t = 1.0 - (rn - 1.0) / 0.35;
            elev_m += 32.0 * ctx.impact_scar * t * t;
        }
    }

    // Preliminary stratigraphic evaluation to read near-surface Rock::toughness and calcity
    let pre_strata = strat.evaluate_column(&ctx, x_m, y_m, elev_m);
    let top_rock = &pre_strata[0];

    // 2. Differential erosion by Rock::toughness:
    // Flat sedimentary plateaus with alternating toughness form stepped cliff-and-bench terraces;
    // folded high-mountainosity belts form hogback ridges along resistant beds.
    if anchor.has_atmosphere && elev_m > sea_m {
        if top_rock.rock_kind == RockKind::Sedimentary && ctx.mountainosity < 3.0 {
            // Terrace quantization scaled by toughness contrast (Catskills / Colorado Plateau style)
            let step_h = (12.0 + 16.0 * top_rock.toughness).clamp(10.0, 28.0);
            let q = elev_m / step_h;
            let frac = q - q.floor();
            // Smooth stepped bench profile: flat plateau tops separated by steeper cliff risers
            let stepped = q.floor() + (frac * frac * (3.0 - 2.0 * frac)).powf(0.45);
            let blend = (0.45 * (1.0 - ctx.mountainosity / 3.0)).clamp(0.1, 0.45);
            elev_m = elev_m * (1.0 - blend) + (stepped * step_h) * blend;
        } else {
            // Differential toughness resistance: tougher caprock stands proud of softer strata
            elev_m += (top_rock.toughness - 0.5) * (18.0 + 24.0 * m_norm);
        }
    }

    // 3. Glacial vs. fluvial carving:
    // Glaciated alpine regions (or heavily scoured shields) carve parabolic U-valleys & cirques;
    // unglaciated mountain belts carve sharper fluvial V-gorges.
    if anchor.has_atmosphere && elev_m > sea_m {
        let valley_wave = fbm2(x_m / 600.0, y_m / 1800.0, seed ^ 0x6066, 2).abs();
        if ctx.is_glaciated || (ctx.glacier_cover > 0.2 && ctx.mountainosity > 2.5) {
            // Broad flat-bottomed parabolic U-trough (valley_wave^2 flattens trough floor, steepens walls)
            let u_factor = (valley_wave * 2.2).clamp(0.0, 1.0);
            let u_carve = (1.0 - u_factor * u_factor) * (25.0 + 75.0 * m_norm);
            elev_m -= u_carve;
        } else if ctx.precip_norm > 0.25 && ctx.mountainosity > 1.5 {
            // Sharp V-shaped fluvial gorge (linear |valley_wave| pinch at valley bottom)
            let v_factor = (valley_wave * 3.0).clamp(0.0, 1.0);
            let v_carve = (1.0 - v_factor) * (18.0 + 60.0 * m_norm) * ctx.precip_norm;
            elev_m -= v_carve;
        }
    }

    // 4. River spline carving & valley/floodplain blending (FR-005, SC-005)
    let mut is_river = false;
    let mut river_water_elev: Option<f32> = None;
    let river_hit: Option<RiverQueryResult> = if anchor.has_atmosphere {
        query_rivers_with_min_width(rivers, x_m, y_m, min_river_half_w_m)
    } else {
        None
    };
    if let Some(rh) = river_hit {
        // Rivers empty into the ocean at the coastline: never carve a freshwater channel or
        // raise artificial levees across open submarine ocean floor (`elev_m <= sea_m`).
        if elev_m > sea_m {
            let eff_z_water = rh.z_water_m.min(elev_m - 1.2).max(sea_m);
            if rh.in_channel {
                let chan_depth = (rh.z_water_m - rh.bed_elev_m).max(1.0);
                elev_m = eff_z_water - chan_depth;
                is_river = true;
                river_water_elev = Some(eff_z_water);
            } else if rh.valley_blend > 0.0 {
                // Smoothly blend land banks down into the incised river without ever lifting
                // submarine terrain above sea level.
                let target_bank = eff_z_water + 1.2 + (1.0 - rh.valley_blend) * 14.0;
                elev_m = elev_m * (1.0 - rh.valley_blend) + target_bank.min(elev_m) * rh.valley_blend;
            }
        }
    }

    // 5. Karst surface sinkholes (dolines) in humid high-calcity strata
    if anchor.has_atmosphere && !is_river && elev_m > sea_m + 3.0 && top_rock.calcity > 0.65 && ctx.precip_norm > 0.25 {
        let sink = fbm2(x_m / 55.0, y_m / 55.0, seed ^ 0x7077, 2);
        if sink < -0.52 {
            let depth = ((-0.52 - sink) / 0.48).clamp(0.0, 1.0) * 14.0 * top_rock.calcity;
            elev_m -= depth;
        }
    }

    // 6. Coastal wave & tide beveling near sea level
    if anchor.has_atmosphere && !is_river {
        let rel_sea = elev_m - sea_m;
        let tide_band = (ctx.tidal_range_m * 1.5).clamp(2.0, 12.0);
        if rel_sea.abs() < tide_band * 2.5 {
            if top_rock.toughness < 0.55 {
                // Soft rock / coastal plain -> gentle beach / tidal flat shelf
                elev_m = sea_m + rel_sea * 0.65;
            } else if rel_sea > 0.0 && rel_sea < tide_band {
                // Tough bedrock coast -> steeper sea-cliff step above wave-cut platform
                elev_m = sea_m + rel_sea * 1.25;
            }
        }
    }

    // Re-evaluate final stratigraphic stack anchored to the carved surface elevation
    let strata = strat.evaluate_column(&ctx, x_m, y_m, elev_m);

    // Determine water/ocean state
    let is_ocean = anchor.has_atmosphere && !is_river && elev_m < sea_m;
    let water_elev_m = if is_river {
        river_water_elev
    } else if is_ocean {
        Some(sea_m)
    } else {
        None
    };

    // Surficial mantle thickness & material (FR-006: directly consumes persisted soil_depth_m / soil_fertility)
    let (surficial_material, soil_depth_m, ice_thickness_m, flora_flag) = if !anchor.has_atmosphere {
        // Airless Hadean tile: bare bedrock/breccia, zero soil, zero water, zero flora
        (strata[0].material, 0.0, 0.0, 0)
    } else if is_river {
        let mat = if ctx.mountainosity > 2.5 {
            MaterialId::RIVER_GRAVEL
        } else {
            MaterialId::ALLUVIAL_SILT
        };
        (mat, 1.5, 0.0, 0)
    } else if is_ocean {
        let mat = if (sea_m - elev_m) < 25.0 {
            MaterialId::SAND
        } else {
            MaterialId::ALLUVIAL_SILT
        };
        (mat, 1.0, 0.0, 0)
    } else {
        // Land tile with atmosphere
        let local_slope = (micro_noise.abs() * 0.5 + meso_noise.abs() * 0.5) * (0.2 + m_norm);
        let ice_thick = if ctx.is_glaciated {
            (2.0 + 14.0 * ctx.glacier_cover).clamp(1.0, 18.0)
        } else if ctx.annual_temp_c < -2.0 && ctx.precip_norm > 0.15 {
            1.0
        } else {
            0.0
        };

        // Modulate persisted soil_depth_m gently by local micro-topography (thin on steep slopes)
        let slope_att = (1.0 - local_slope * 0.6).clamp(0.15, 1.0);
        let d_soil = (ctx.soil_depth_m * slope_att).clamp(0.0, 20.0);

        let mat = if d_soil < 0.35 {
            // Bare scoured bedrock or steep talus
            if local_slope > 0.75 {
                MaterialId::TALUS
            } else {
                strata[0].material
            }
        } else if ctx.precip_norm < 0.16 && ctx.annual_temp_c > 5.0 {
            if strata[0].salinity > 0.35 {
                MaterialId::SALT_CRUST
            } else {
                MaterialId::SAND
            }
        } else if strata[0].bogginess > 0.45 && ctx.precip_norm > 0.45 {
            MaterialId::PEAT
        } else if ctx.soil_depth_m >= 3.5 && ctx.soil_fertility > 1.05 {
            MaterialId::LOESS_TILL
        } else if (elev_m - sea_m) < ctx.tidal_range_m * 1.4 {
            MaterialId::SAND
        } else {
            MaterialId::TOPSOIL
        };

        let flora = if anchor.has_life && ice_thick <= 0.0 && d_soil >= 0.4 && ctx.precip_norm > 0.18 && ctx.annual_temp_c > -4.0 {
            let f_n = fbm2(x_m / 18.0, y_m / 18.0, seed ^ 0x8088, 2);
            if ctx.precip_norm > 0.45 && f_n > 0.35 {
                cell_flags::FLORA_TREE
            } else if f_n > 0.05 {
                cell_flags::FLORA_SHRUB
            } else {
                cell_flags::FLORA_GRASS
            }
        } else {
            0
        };

        (mat, d_soil, ice_thick, flora)
    };

    let bedrock_top_m = elev_m - soil_depth_m;

    ColumnSynthesis {
        ctx,
        surface_elev_m: elev_m,
        bedrock_top_m,
        water_elev_m,
        is_ocean,
        is_river,
        surficial_material,
        soil_depth_m,
        ice_thickness_m,
        strata,
        flora_flag,
    }
}

/// Evaluate a single 3D voxel `HexPrismCell` at vertical elevation `z_m` inside a synthesized column.
pub fn sample_cell_at_z(
    anchor: &LocalZoomAnchor,
    strat: &RegionalStratigraphyField,
    col: &ColumnSynthesis,
    x_m: f64,
    y_m: f64,
    z_m: f32,
) -> HexPrismCell {
    let top_solid_m = col.surface_elev_m + col.ice_thickness_m;

    // 1. Above ground & ice: check water or air
    if z_m > top_solid_m {
        if let Some(w_z) = col.water_elev_m {
            if z_m <= w_z {
                let depth_lvl = ((w_z - z_m).ceil() as u8).clamp(1, 7);
                return HexPrismCell::water(col.is_ocean, depth_lvl);
            }
        }
        return HexPrismCell::AIR;
    }

    // 2. Glacial ice / snowpack cap
    if col.ice_thickness_m > 0.0 && z_m > col.surface_elev_m {
        let mat = if z_m > top_solid_m - 1.2 {
            MaterialId::SNOW
        } else {
            MaterialId::ICE
        };
        let mut cell = HexPrismCell::solid(mat);
        if z_m + 1.0 > top_solid_m {
            cell.flags |= cell_flags::EXPOSED_SURFACE;
        }
        return cell;
    }

    // 3. Surficial soil / loess / sand / peat / alluvial mantle
    if col.soil_depth_m >= 0.35 && z_m > col.bedrock_top_m {
        let mut cell = HexPrismCell::solid(col.surficial_material);
        if z_m + 1.0 > col.surface_elev_m && col.ice_thickness_m <= 0.0 {
            cell.flags |= cell_flags::EXPOSED_SURFACE | col.flora_flag;
        }
        return cell;
    }

    // 4. Bedrock & discrete ore/mineral veins
    let (mat, slice) = strat.sample_bedrock_material(&col.strata, x_m, y_m, z_m);

    // 5. Underground karst cavern dissolution inside soluble high-calcity or evaporite strata
    if anchor.has_atmosphere
        && (slice.calcity > 0.65 || slice.salinity > 0.55)
        && col.ctx.precip_norm > 0.25
        && z_m < col.surface_elev_m - 4.0
    {
        let c1 = value_noise3(x_m * 0.055, y_m * 0.055, (z_m as f64) * 0.09, anchor.world_seed ^ 0xcafe);
        let c2 = value_noise3(x_m * 0.055 + 17.3, y_m * 0.055 - 11.7, (z_m as f64) * 0.09, anchor.world_seed ^ 0xbabe);
        // Intersecting dual-ridged worm cavities form connected 3D cave galleries
        let ridge = (c1 - 0.5).abs() + (c2 - 0.5).abs();
        if ridge < 0.145 {
            // Submerged aquifer below sea level / water table, open air cave above
            let water_table = col.water_elev_m.unwrap_or(anchor.sea_level_m as f32 - 8.0);
            if z_m <= water_table {
                let mut cell = HexPrismCell::water(false, 7);
                cell.flags |= cell_flags::NATURAL_CAVE;
                return cell;
            }
            return HexPrismCell {
                material: MaterialId::AIR,
                fluid: 0,
                flags: cell_flags::NATURAL_CAVE,
            };
        }
    }

    let mut cell = HexPrismCell::solid(mat);
    if z_m + 1.0 > col.surface_elev_m && col.ice_thickness_m <= 0.0 {
        cell.flags |= cell_flags::EXPOSED_SURFACE;
    }
    cell
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anchor::{NeighborTileSample, SnapshotStage};
    use tec11_geology::rock::{Rock, RockLayer};

    fn synthetic_anchor(
        has_atmosphere: bool,
        soil_depth_m: f32,
        soil_fertility: f32,
        rock: Rock,
        precip_norm: f32,
    ) -> LocalZoomAnchor {
        LocalZoomAnchor {
            world_seed: 777,
            stage: if has_atmosphere {
                SnapshotStage::Prehistory
            } else {
                SnapshotStage::Tectonics
            },
            grid_size: 6,
            radius_km: 6371.0,
            parent_tile_id: 0,
            parent_tile_radius_m: 100_000.0,
            anchor_normal: [0.0, 1.0, 0.0],
            tangent_east: [1.0, 0.0, 0.0],
            tangent_north: [0.0, 0.0, 1.0],
            sea_level_m: 0.0,
            has_atmosphere,
            has_life: has_atmosphere,
            is_pentagon: false,
            neighbors: vec![NeighborTileSample {
                tile_id: 0,
                local_xy_m: [0.0, 0.0],
                elevation_m: 500.0,
                bottom_m: -9000.0,
                mountainosity: 0.5,
                subduction: 0.0,
                impact_scar: 0.0,
                soil_depth_m,
                soil_fertility,
                is_glaciated: false,
                glacier_cover: 0.0,
                annual_temp_c: 14.0,
                precip_norm,
                tidal_range_m: 1.5,
                wind_vec: [1.0, 0.0],
                layers: vec![RockLayer {
                    rock,
                    thickness: 9.5,
                }],
                occurrences: vec![],
            }],
            river_edges: vec![],
        }
    }

    #[test]
    fn test_airless_hadean_and_surficial_soil_and_karst() {
        // 1. Airless Hadean Tectonics tile: zero soil, zero water, zero flora
        let hadean = synthetic_anchor(false, 0.0, 1.0, Rock::extrusive(0.48), 0.0);
        let h_strat = RegionalStratigraphyField::from_anchor(&hadean);
        let h_col = synthesize_column(&hadean, &h_strat, &[], 0.0, 0.0);
        assert_eq!(h_col.soil_depth_m, 0.0);
        assert!(h_col.water_elev_m.is_none());
        assert_eq!(h_col.flora_flag, 0);
        assert_eq!(h_col.surficial_material, MaterialId::BASALT);

        // 2. Loess-enriched soil vs. bare scoured bedrock (FR-006)
        let loess = synthetic_anchor(true, 6.0, 1.30, Rock::intrusive(0.74), 0.6);
        let l_strat = RegionalStratigraphyField::from_anchor(&loess);
        let l_col = synthesize_column(&loess, &l_strat, &[], 0.0, 0.0);
        assert!(l_col.soil_depth_m > 3.5);
        assert_eq!(l_col.surficial_material, MaterialId::LOESS_TILL);

        let scoured = synthetic_anchor(true, 0.05, 0.85, Rock::intrusive(0.74), 0.6);
        let s_strat = RegionalStratigraphyField::from_anchor(&scoured);
        let s_col = synthesize_column(&scoured, &s_strat, &[], 0.0, 0.0);
        assert!(s_col.soil_depth_m < 0.35);
        assert_eq!(s_col.surficial_material, MaterialId::GRANITE);

        // 3. Humid limestone karst produces underground natural caves
        let limestone = Rock {
            kind: RockKind::Sedimentary,
            name: "limestone",
            felsity: 0.5,
            toughness: 0.45,
            clasticity: Some(4.0),
            calcity: Some(0.95),
            bogginess: None,
            quartzity: None,
            sulfidity: None,
            salinity: None,
        };
        let karst = synthetic_anchor(true, 2.0, 1.0, limestone, 0.75);
        let k_strat = RegionalStratigraphyField::from_anchor(&karst);
        let mut cave_cells = 0usize;
        for ix in 0..32 {
            for iy in 0..32 {
                let x = (ix as f64) * 3.0;
                let y = (iy as f64) * 3.0;
                let col = synthesize_column(&karst, &k_strat, &[], x, y);
                for dz in 6..38 {
                    let c = sample_cell_at_z(&karst, &k_strat, &col, x, y, col.surface_elev_m - (dz as f32));
                    if (c.flags & cell_flags::NATURAL_CAVE) != 0 {
                        cave_cells += 1;
                    }
                }
            }
        }
        assert!(
            cave_cells > 50,
            "Expected karst caverns in humid high-calcity limestone (found {cave_cells} cave cells)"
        );
    }
}
