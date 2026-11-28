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
use crate::rivers::{DrainageNetwork, LakeState, RiverQueryResult};
use crate::stratigraphy::{
    fbm2, value_noise3, InterpolatedLocalContext, LocalStratumSlice, RegionalStratigraphyField,
};
use crate::volcanoes::{VolcanicField, VolcanicQueryResult};

/// Evaluated surface & column summary at a single tangent-plane point `(x_m, y_m)`.
#[derive(Clone, Debug)]
pub struct ColumnSynthesis {
    pub ctx: InterpolatedLocalContext,
    pub surface_elev_m: f32,
    pub bedrock_top_m: f32,
    pub water_elev_m: Option<f32>,
    pub is_ocean: bool,
    pub is_river: bool,
    pub is_lake: bool,
    pub is_sandbar: bool,
    pub surficial_material: MaterialId,
    pub soil_depth_m: f32,
    pub ice_thickness_m: f32,
    pub strata: Vec<LocalStratumSlice>,
    pub flora_flag: u8,
    pub volcanic_hit: Option<VolcanicQueryResult>,
}

/// Evaluate all physical geomorphology operators at `(x_m, y_m)` at exact physical width
/// (including procedural tributaries and volcanic edifices).
pub fn synthesize_column(
    anchor: &LocalZoomAnchor,
    strat: &RegionalStratigraphyField,
    drainage: &DrainageNetwork,
    volcanoes: &VolcanicField,
    x_m: f64,
    y_m: f64,
) -> ColumnSynthesis {
    synthesize_column_with_options(anchor, strat, drainage, volcanoes, x_m, y_m, 0.0, true)
}

/// Evaluate all physical geomorphology operators at `(x_m, y_m)` using the Hydrology-First
/// Non-Negative Relief Envelope (`z = z_valley + R(d, x, y)`, `R >= 0`), with optional coarse-scale
/// minimum channel half-width (`min_river_half_w_m`) and tributary visibility toggle.
#[allow(clippy::too_many_arguments)]
pub fn synthesize_column_with_options(
    anchor: &LocalZoomAnchor,
    strat: &RegionalStratigraphyField,
    drainage: &DrainageNetwork,
    volcanoes: &VolcanicField,
    x_m: f64,
    y_m: f64,
    min_river_half_w_m: f32,
    include_tributaries: bool,
) -> ColumnSynthesis {
    let ctx = strat.sample_context(anchor, x_m, y_m);
    let seed = anchor.world_seed;
    let sea_m = anchor.sea_level_m as f32;

    // Ground regional elevation on History's subdivided fine grid when in History stage
    let grounded_base_m = anchor.sample_history_fine_ground(x_m, y_m, ctx.base_elevation_m);

    // 1. Structural & regional macro-relief
    let m_norm = (ctx.mountainosity / 10.0).clamp(0.0, 1.5);
    let macro_noise = fbm2(x_m / 2600.0, y_m / 2600.0, seed ^ 0x1011, 4);
    let meso_noise = fbm2(x_m / 420.0, y_m / 420.0, seed ^ 0x2022, 3);
    let micro_noise = fbm2(x_m / 65.0, y_m / 65.0, seed ^ 0x3033, 2);

    // Ridge-shaped relief for active orogens (1 - |noise|) vs rolling relief for lowlands
    let ridge_raw = 1.0 - fbm2(x_m / 1100.0, y_m / 1100.0, seed ^ 0x4044, 3).abs();
    let ridge_relief = (ridge_raw * ridge_raw - 0.35) * (140.0 * m_norm);

    // Smooth macro-scale regional elevation (`1–2.6 km` wavelengths, uniform across a river
    // channel cross-section) used to align river mouths with the actual coastline.
    let regional_base_m =
        grounded_base_m + macro_noise * (32.0 + 170.0 * m_norm) + ridge_relief;

    let mut elev_m = regional_base_m
        + meso_noise * (7.0 + 52.0 * m_norm)
        + micro_noise * (1.5 + 10.0 * m_norm);

    // Impact crater rim & bowl topography if impact_scar > 0.25
    if ctx.impact_scar > 0.25 {
        let r_m = (x_m * x_m + y_m * y_m).sqrt() as f32;
        let crater_r = 2200.0f32;
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
    // Flat sedimentary plateaus form stepped cliff-and-bench terraces; folded belts form hogbacks.
    if anchor.has_atmosphere && elev_m > sea_m {
        if top_rock.rock_kind == RockKind::Sedimentary && ctx.mountainosity < 3.0 {
            let step_h = (12.0 + 16.0 * top_rock.toughness).clamp(10.0, 28.0);
            let q = elev_m / step_h;
            let frac = q - q.floor();
            let stepped = q.floor() + (frac * frac * (3.0 - 2.0 * frac)).powf(0.45);
            let blend = (0.42 * (1.0 - ctx.mountainosity / 3.0)).clamp(0.1, 0.42);
            elev_m = elev_m * (1.0 - blend) + (stepped * step_h) * blend;
        } else {
            elev_m += (top_rock.toughness - 0.5) * (16.0 + 22.0 * m_norm);
        }
    }

    // 3. Glacial vs. fluvial regional relief carving outside active channels
    let is_glacial_regime =
        ctx.is_glaciated || (ctx.glacier_cover > 0.2 && ctx.mountainosity > 2.5);
    if anchor.has_atmosphere && elev_m > sea_m {
        let valley_wave = fbm2(x_m / 600.0, y_m / 1800.0, seed ^ 0x6066, 2).abs();
        if is_glacial_regime {
            let u_factor = (valley_wave * 2.2).clamp(0.0, 1.0);
            elev_m -= (1.0 - u_factor * u_factor) * (22.0 + 70.0 * m_norm);
        } else if ctx.precip_norm > 0.25 && ctx.mountainosity > 1.5 {
            let v_factor = (valley_wave * 3.0).clamp(0.0, 1.0);
            elev_m -= (1.0 - v_factor) * (16.0 + 55.0 * m_norm) * ctx.precip_norm;
        }
    }

    // 3b. Volcanic edifices, calderas, parasitic cinder cones, and eroded necks (`Spec 009`).
    // Applied BEFORE the hydrology envelope so rivers compose strictly downhill through flanks.
    let volcanic_hit = volcanoes.query(x_m, y_m, anchor.has_atmosphere, seed);
    if let Some(vh) = volcanic_hit {
        elev_m += vh.delta_elev_m;
        if let Some((floor_z, bowl_blend)) = vh.caldera_floor_blend {
            elev_m = elev_m * (1.0 - bowl_blend) + floor_z * bowl_blend;
        }
        if let Some(rim_min) = vh.rim_min_elev_m {
            elev_m = elev_m.max(rim_min);
        }
    }

    // 4. Hydrology-First Non-Negative Relief Envelope (`z = z_valley + R(d, x, y)`, `R >= 0`):
    // Inside the drainage corridor on land (`grounded_base_m > sea_m`), construct surrounding
    // valley-wall elevation strictly UPWARD from the strictly monotonic river `z_water_m` so
    // rivers never float above terrain (no aqueducts) and never climb uphill (no uphill waterfalls).
    let mut is_river = false;
    let mut is_sandbar = false;
    let mut river_water_elev: Option<f32> = None;
    let river_hit: Option<RiverQueryResult> = if anchor.has_atmosphere {
        drainage.query_rivers(x_m, y_m, min_river_half_w_m, include_tributaries)
    } else {
        None
    };
    if let Some(rh) = river_hit {
        // Use `rh.z_water_m` directly (constant across the channel cross-section) so both banks
        // always see the exact same water elevation as the channel center — preventing diagonal
        // coastlines from dropping one bank below the river surface.
        let z_w = rh.z_water_m.max(sea_m);
        let at_sea_level = z_w <= sea_m + 0.05;
        if rh.in_channel {
            if at_sea_level {
                // Estuary / river mouth at sea level: carve cleanly through any coastal headland
                // or noisy ridge straight into the ocean (`elev_m < sea_m`), with no cliff dam
                // and seamless transition to `OCEAN_WATER`.
                if rh.bar_height_m > 0.0 && grounded_base_m > sea_m - 15.0 {
                    elev_m = sea_m + rh.bar_height_m;
                    is_sandbar = true;
                } else {
                    elev_m = elev_m.min(rh.bed_elev_m.min(sea_m - 1.5));
                }
            } else if rh.bar_height_m > 0.0 {
                // Emergent braided mid-channel sandbar / alluvial island
                elev_m = z_w + rh.bar_height_m;
                is_sandbar = true;
            } else {
                elev_m = rh.bed_elev_m.min(z_w - 0.8);
                is_river = true;
                river_water_elev = Some(z_w);
            }
        } else if rh.valley_blend > 0.0 && (elev_m > sea_m || !at_sea_level) {
            // Non-negative relief R(d, x, y) >= 0 rising strictly upward from `z_w` at bank (u = 0)
            // to the regional terrain height at valley rim (u = 1). Near sea level, taper
            // `min_bank_rise` smoothly to 0 so estuary banks merge into the coastline without
            // raising the seafloor into artificial levees.
            let u = rh.valley_u;
            let profile = if is_glacial_regime {
                // Parabolic glacial U-valley trough
                u * u
            } else {
                // Fluvial V-valley / gentle floodplain rising into canyon walls
                u.powf(1.25)
            };
            let coastal_taper = ((z_w - sea_m) / 14.0).clamp(0.0, 1.0);
            let wall_noise = (0.5 + 0.5 * meso_noise).clamp(0.0, 1.0);
            let bank_floor = z_w + 1.2 * coastal_taper;
            let valley_carved =
                bank_floor + profile * (18.0 + 95.0 * m_norm) * (0.7 + 0.6 * wall_noise);
            // Carve canyons down toward `valley_carved` without ever building a fake wall higher
            // than the surrounding terrain, while keeping banks >= `bank_floor` (zero aqueducts)
            // and blending with C^1 continuity to unmodified `elev_m` at `valley_blend -> 0`.
            let floor_blend = (rh.valley_blend * 2.0).clamp(0.0, 1.0);
            let lifted_elev = elev_m * (1.0 - floor_blend) + elev_m.max(bank_floor) * floor_blend;
            let target_in_valley = valley_carved.min(lifted_elev);
            elev_m = lifted_elev * (1.0 - rh.valley_blend) + target_in_valley * rh.valley_blend;
        }
    }

    // 5. Climate- & rock-gated structural LakeBasins (impact craters, glacial troughs, basins)
    let mut is_lake = false;
    let mut lake_state_hit: Option<LakeState> = None;
    let mut lake_water_elev: Option<f32> = None;
    if anchor.has_atmosphere && !is_river && !is_sandbar && grounded_base_m > sea_m {
        if let Some(lk) = drainage.query_lakes(x_m, y_m) {
            if lk.in_basin {
                elev_m = lk.floor_elev_m;
                lake_state_hit = Some(lk.state);
                if lk.state == LakeState::Freshwater {
                    is_lake = true;
                    lake_water_elev = Some(lk.z_water_m);
                }
            } else if lk.rim_blend > 0.0 {
                let rim_target = lk.z_water_m + (1.0 - lk.rim_blend) * 24.0;
                elev_m = elev_m * (1.0 - lk.rim_blend) + rim_target.max(lk.z_water_m + 1.0) * lk.rim_blend;
            }
        }
    }

    // 5b. Volcanic crater lakes & caldera floors (`Spec 009`, `FR-004`)
    if anchor.has_atmosphere && !is_river && !is_sandbar && !is_lake {
        if let Some(vh) = volcanic_hit {
            if vh.in_caldera {
                if let (Some(z_lake), Some(lstate)) = (vh.crater_lake_z_m, vh.crater_lake_state) {
                    if elev_m < z_lake {
                        lake_state_hit = Some(lstate);
                        if lstate == LakeState::Freshwater {
                            is_lake = true;
                            lake_water_elev = Some(z_lake);
                        }
                    }
                }
            }
        }
    }

    // 6. Karst surface sinkholes (dolines) in dry humid high-calcity strata well away from rivers/lakes
    let away_from_river = river_hit.map_or(true, |rh| rh.valley_blend < 0.15);
    if anchor.has_atmosphere
        && !is_river
        && !is_lake
        && away_from_river
        && elev_m > sea_m + 12.0
        && top_rock.calcity > 0.65
        && ctx.precip_norm > 0.25
    {
        let sink = fbm2(x_m / 55.0, y_m / 55.0, seed ^ 0x7077, 2);
        if sink < -0.52 {
            let depth = ((-0.52 - sink) / 0.48).clamp(0.0, 1.0) * 14.0 * top_rock.calcity;
            elev_m -= depth;
        }
    }

    // 7. Coastal wave & tide beveling near sea level (only outside river valleys so rivers stay recessed)
    if anchor.has_atmosphere && !is_river && !is_lake && away_from_river {
        let rel_sea = elev_m - sea_m;
        let tide_band = (ctx.tidal_range_m * 1.5).clamp(2.0, 12.0);
        if rel_sea.abs() < tide_band * 2.5 {
            if top_rock.toughness < 0.55 {
                elev_m = sea_m + rel_sea * 0.65;
            } else if rel_sea > 0.0 && rel_sea < tide_band {
                elev_m = sea_m + rel_sea * 1.25;
            }
        }
    }

    // Re-evaluate final stratigraphic stack anchored to the carved surface elevation
    let strata = strat.evaluate_column(&ctx, x_m, y_m, elev_m);

    // Determine water/ocean state
    let is_ocean = anchor.has_atmosphere && !is_river && !is_lake && !is_sandbar && elev_m < sea_m;
    let water_elev_m = if is_river {
        river_water_elev
    } else if is_lake {
        lake_water_elev
    } else if is_ocean {
        Some(sea_m)
    } else {
        None
    };

    // Surficial mantle thickness & material (FR-006: directly consumes persisted soil_depth_m / soil_fertility)
    let (surficial_material, soil_depth_m, ice_thickness_m, flora_flag) = if !anchor.has_atmosphere {
        let base_mat = if let Some(vh) = volcanic_hit {
            if vh.in_conduit_or_dike || vh.delta_elev_m > 30.0 {
                vh.magma_material
            } else {
                strata[0].material
            }
        } else {
            strata[0].material
        };
        (base_mat, 0.0, 0.0, 0)
    } else if volcanic_hit.is_some_and(|vh| vh.sulfur_intensity > 0.35 && !is_river && !is_lake) {
        // Active fumarolic sulfur crusts on crater rims & resurgent caldera domes (`Spec 009`, `FR-006`)
        (MaterialId::SULFUR_DEPOSIT, 0.8, 0.0, 0)
    } else if is_sandbar {
        let mat = if ctx.mountainosity > 2.5 {
            MaterialId::RIVER_GRAVEL
        } else {
            MaterialId::SAND
        };
        (mat, 2.0, 0.0, 0)
    } else if is_river {
        let mat = if ctx.mountainosity > 2.5 {
            MaterialId::RIVER_GRAVEL
        } else {
            MaterialId::ALLUVIAL_SILT
        };
        (mat, 1.5, 0.0, 0)
    } else if let Some(lstate) = lake_state_hit {
        match lstate {
            LakeState::Freshwater => (MaterialId::ALLUVIAL_SILT, 1.5, 0.0, 0),
            LakeState::Frozen => (MaterialId::ALLUVIAL_SILT, 1.0, 2.5, 0),
            LakeState::SaltFlat => (MaterialId::SALT_CRUST, 1.2, 0.0, 0),
            LakeState::DryPlaya => (MaterialId::ALLUVIAL_SILT, 0.8, 0.0, 0),
        }
    } else if is_ocean {
        let mat = if (sea_m - elev_m) < 25.0 {
            MaterialId::SAND
        } else {
            MaterialId::ALLUVIAL_SILT
        };
        (mat, 1.0, 0.0, 0)
    } else {
        let local_slope = (micro_noise.abs() * 0.5 + meso_noise.abs() * 0.5) * (0.2 + m_norm);
        let ice_thick = if ctx.is_glaciated {
            (2.0 + 14.0 * ctx.glacier_cover).clamp(1.0, 18.0)
        } else if ctx.annual_temp_c < -2.0 && ctx.precip_norm > 0.15 {
            1.0
        } else {
            0.0
        };

        let slope_att = (1.0 - local_slope * 0.6).clamp(0.15, 1.0);
        let d_soil = (ctx.soil_depth_m * slope_att).clamp(0.0, 20.0);

        let mat = if (elev_m - sea_m) < ctx.tidal_range_m * 1.4 && strata[0].toughness < 0.72 {
            MaterialId::SAND
        } else if d_soil < 0.35 {
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
        } else {
            MaterialId::TOPSOIL
        };

        let flora = if anchor.has_life
            && ice_thick <= 0.0
            && d_soil >= 0.4
            && ctx.precip_norm > 0.18
            && ctx.annual_temp_c > -4.0
        {
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
        is_lake,
        is_sandbar,
        surficial_material,
        soil_depth_m,
        ice_thickness_m,
        strata,
        flora_flag,
        volcanic_hit,
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

    // 4. Bedrock, volcanic feeder conduits/dikes/contact aureoles (`Spec 009`), & ore veins
    let depth_below = (col.surface_elev_m - z_m).max(0.0);
    let (mut mat, slice) = strat.sample_bedrock_material(&col.strata, x_m, y_m, z_m);
    if let Some(vh) = col.volcanic_hit {
        if vh.in_conduit_or_dike {
            mat = if depth_below < 55.0 {
                vh.magma_material
            } else {
                vh.pluton_material
            };
        } else if vh.in_contact_halo {
            mat = MaterialId::HORNFELS;
        }

        // Hollow 3D basalt lava tubes inside fresh shield volcano flanks (`FR-006`)
        if vh.lava_tube_eligible && depth_below >= 4.0 && depth_below <= 28.0 && !vh.in_conduit_or_dike {
            let along = x_m * (vh.radial_dir[0] as f64) + y_m * (vh.radial_dir[1] as f64);
            let across = -x_m * (vh.radial_dir[1] as f64) + y_m * (vh.radial_dir[0] as f64);
            let t1 = value_noise3(along * 0.018, across * 0.075, (z_m as f64) * 0.11, anchor.world_seed ^ 0x1a7a_0001);
            let t2 = value_noise3(along * 0.018 + 9.1, across * 0.075 - 13.4, (z_m as f64) * 0.11, anchor.world_seed ^ 0x1a7a_0002);
            let tube_ridge = (t1 - 0.5).abs() + (t2 - 0.5).abs();
            if tube_ridge < 0.155 {
                return HexPrismCell {
                    material: MaterialId::AIR,
                    fluid: 0,
                    flags: cell_flags::NATURAL_CAVE,
                };
            }
        }
    }

    // 5. Underground natural cave systems (karst, flood-basalt lava tubes, coastal sea caves,
    //    and tectonic fissure caverns). Skip inside active volcanic magma conduits/dikes.
    let in_volcanic_conduit = col
        .volcanic_hit
        .is_some_and(|vh| vh.in_conduit_or_dike);

    if !in_volcanic_conduit {
        // 5a. Karst cavern dissolution inside soluble high-calcity or evaporite strata
        if anchor.has_atmosphere
            && (slice.calcity > 0.65 || slice.salinity > 0.55)
            && col.ctx.precip_norm > 0.25
            && depth_below > 4.0
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

        // 5b. Extrusive flood-basalt lava tube caves (outside shield-volcano edifice flanks)
        if mat == MaterialId::BASALT
            && col.ctx.mountainosity >= 0.45
            && depth_below >= 3.0
            && depth_below <= 28.0
        {
            let t1 = value_noise3(x_m * 0.028, y_m * 0.068, (z_m as f64) * 0.105, anchor.world_seed ^ 0x1a7a_1001);
            let t2 = value_noise3(x_m * 0.028 + 11.3, y_m * 0.068 - 7.9, (z_m as f64) * 0.105, anchor.world_seed ^ 0x1a7a_1002);
            let tube_ridge = (t1 - 0.5).abs() + (t2 - 0.5).abs();
            if tube_ridge < 0.165 {
                return HexPrismCell {
                    material: MaterialId::AIR,
                    fluid: 0,
                    flags: cell_flags::NATURAL_CAVE,
                };
            }
        }

        // 5c. Coastal sea caves & littoral grottoes carved near sea level in coastal cliffs
        let sea_m = anchor.sea_level_m as f32;
        if anchor.has_atmosphere
            && col.surface_elev_m > sea_m + 3.5
            && col.surface_elev_m < sea_m + 75.0
            && z_m >= sea_m - 3.0
            && z_m <= sea_m + 9.0
            && depth_below >= 2.5
        {
            let s1 = value_noise3(x_m * 0.048, y_m * 0.048, ((z_m - sea_m) as f64) * 0.14, anchor.world_seed ^ 0x5ea0_c001);
            let s2 = value_noise3(x_m * 0.048 - 14.1, y_m * 0.048 + 9.7, ((z_m - sea_m) as f64) * 0.14, anchor.world_seed ^ 0x5ea0_c002);
            let grotto_ridge = (s1 - 0.5).abs() + (s2 - 0.5).abs();
            if grotto_ridge < 0.175 {
                if z_m <= sea_m {
                    let mut cell = HexPrismCell::water(true, 7);
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

        // 5d. Tectonic fissure caverns: steep, vertically elongated fault-fracture clefts in
        //     uplifted mountain highlands
        if col.ctx.mountainosity >= 0.65 && depth_below >= 3.5 && depth_below <= 32.0 {
            let f1 = value_noise3(x_m * 0.062, y_m * 0.036, (z_m as f64) * 0.032, anchor.world_seed ^ 0xfa01_7001);
            let f2 = value_noise3(x_m * 0.062 + 23.4, y_m * 0.036 - 15.2, (z_m as f64) * 0.032, anchor.world_seed ^ 0xfa01_7002);
            let fissure_ridge = (f1 - 0.5).abs() + (f2 - 0.5).abs();
            if fissure_ridge < 0.155 {
                let water_table = col.water_elev_m.unwrap_or(sea_m - 8.0);
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
                has_intrusion: false,
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
            fine_samples: vec![],
            fine_spacing_m: 0.0,
            tile_frames: vec![],
        }
    }

    #[test]
    fn test_airless_hadean_and_surficial_soil_and_karst() {
        let empty = DrainageNetwork::default();
        let no_volc = VolcanicField::default();
        // 1. Airless Hadean Tectonics tile: zero soil, zero water, zero flora
        let hadean = synthetic_anchor(false, 0.0, 1.0, Rock::extrusive(0.48), 0.0);
        let h_strat = RegionalStratigraphyField::from_anchor(&hadean);
        let h_col = synthesize_column(&hadean, &h_strat, &empty, &no_volc, 0.0, 0.0);
        assert_eq!(h_col.soil_depth_m, 0.0);
        assert!(h_col.water_elev_m.is_none());
        assert_eq!(h_col.flora_flag, 0);
        assert_eq!(h_col.surficial_material, MaterialId::BASALT);

        // 2. Loess-enriched soil vs. bare scoured bedrock (FR-006)
        let loess = synthetic_anchor(true, 6.0, 1.30, Rock::intrusive(0.74), 0.6);
        let l_strat = RegionalStratigraphyField::from_anchor(&loess);
        let l_col = synthesize_column(&loess, &l_strat, &empty, &no_volc, 0.0, 0.0);
        assert!(l_col.soil_depth_m > 3.5);
        assert_eq!(l_col.surficial_material, MaterialId::LOESS_TILL);

        let scoured = synthetic_anchor(true, 0.05, 0.85, Rock::intrusive(0.74), 0.6);
        let s_strat = RegionalStratigraphyField::from_anchor(&scoured);
        let s_col = synthesize_column(&scoured, &s_strat, &empty, &no_volc, 0.0, 0.0);
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
                let col = synthesize_column(&karst, &k_strat, &empty, &no_volc, x, y);
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

    #[test]
    fn test_basalt_lava_tubes_sea_caves_and_tectonic_fissures() {
        let empty = DrainageNetwork::default();
        let no_volc = VolcanicField::default();

        // 1. Flood-basalt lava tubes in extrusive basalt highlands
        let mut basalt_anchor = synthetic_anchor(true, 0.5, 1.0, Rock::extrusive(0.15), 0.15);
        basalt_anchor.neighbors[0].mountainosity = 1.2;
        let b_strat = RegionalStratigraphyField::from_anchor(&basalt_anchor);
        let mut tube_cells = 0usize;
        for ix in 0..24 {
            for iy in 0..24 {
                let x = (ix as f64) * 2.5;
                let y = (iy as f64) * 2.5;
                let col = synthesize_column(&basalt_anchor, &b_strat, &empty, &no_volc, x, y);
                for dz in 4..26 {
                    let c = sample_cell_at_z(
                        &basalt_anchor,
                        &b_strat,
                        &col,
                        x,
                        y,
                        col.surface_elev_m - (dz as f32),
                    );
                    if (c.flags & cell_flags::NATURAL_CAVE) != 0 && c.is_air() {
                        tube_cells += 1;
                    }
                }
            }
        }
        assert!(
            tube_cells > 40,
            "Expected hollow basalt lava tube voids (found {tube_cells} cells)"
        );

        // 2. Coastal sea caves & flooded littoral grottoes near sea level
        let mut coast_anchor = synthetic_anchor(true, 0.5, 0.8, Rock::intrusive(0.72), 0.15);
        coast_anchor.neighbors[0].elevation_m = 18.0;
        coast_anchor.neighbors[0].mountainosity = 0.2;
        let c_strat = RegionalStratigraphyField::from_anchor(&coast_anchor);
        let mut sea_cave_air = 0usize;
        let mut sea_cave_water = 0usize;
        for ix in 0..24 {
            for iy in 0..24 {
                let x = (ix as f64) * 2.0;
                let y = (iy as f64) * 2.0;
                let col = synthesize_column(&coast_anchor, &c_strat, &empty, &no_volc, x, y);
                for z in -3..=8 {
                    let c = sample_cell_at_z(&coast_anchor, &c_strat, &col, x, y, z as f32);
                    if (c.flags & cell_flags::NATURAL_CAVE) != 0 {
                        if c.is_air() {
                            sea_cave_air += 1;
                        } else if c.material == MaterialId::OCEAN_WATER {
                            sea_cave_water += 1;
                        }
                    }
                }
            }
        }
        assert!(
            sea_cave_air > 20 && sea_cave_water > 10,
            "Expected both open-air and sea-flooded coastal sea caves (air={sea_cave_air}, water={sea_cave_water})"
        );

        // 3. Tectonic fissure caverns in uplifted mountain granite (low precip, non-karst)
        let mut fissure_anchor = synthetic_anchor(true, 0.4, 0.8, Rock::intrusive(0.74), 0.10);
        fissure_anchor.neighbors[0].mountainosity = 1.5;
        let f_strat = RegionalStratigraphyField::from_anchor(&fissure_anchor);
        let mut fissure_cells = 0usize;
        for ix in 0..24 {
            for iy in 0..24 {
                let x = (ix as f64) * 2.5;
                let y = (iy as f64) * 2.5;
                let col = synthesize_column(&fissure_anchor, &f_strat, &empty, &no_volc, x, y);
                for dz in 4..28 {
                    let c = sample_cell_at_z(
                        &fissure_anchor,
                        &f_strat,
                        &col,
                        x,
                        y,
                        col.surface_elev_m - (dz as f32),
                    );
                    if (c.flags & cell_flags::NATURAL_CAVE) != 0 {
                        fissure_cells += 1;
                    }
                }
            }
        }
        assert!(
            fissure_cells > 40,
            "Expected tectonic fissure cavern voids in uplifted mountains (found {fissure_cells} cells)"
        );
    }
}
