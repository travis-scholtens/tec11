//! Emergent volcanic edifices, collapse calderas, crater lakes, parasitic cinder cones,
//! eroded volcanic necks, and 3D subsurface magmatic plumbing (`Spec 009`, `FR-001`–`FR-007`).

use tec11_geology::rock::RockKind;

use crate::anchor::{LocalZoomAnchor, NeighborTileSample};
use crate::materials::MaterialId;
use crate::rivers::{classify_lake_state, DrainageNetwork, LakeState};
use crate::stratigraphy::{fbm2, value_noise2, RegionalStratigraphyField};

/// Physical volcanic landform morphology derived strictly from persisted `TileGeology`
/// (`subduction`, `intrusion`, top `Rock` kind & `felsity`, and `mountainosity`).
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum VolcanoKind {
    /// Intermediate extrusive (`andesite`) composite cone (`10–25 km` diameter, `14–28°` flanks)
    /// with radial erosion gullies and a summit crater or collapse caldera.
    Stratovolcano,
    /// Mafic extrusive (`basalt`) broad low-angle dome (`40–88 km` diameter, `< 8°` flanks)
    /// with a nested `3–6 km` summit pit caldera and basalt lava tubes.
    ShieldVolcano,
    /// Felsic extrusive (`rhyolite`) wide collapse caldera (`24–52 km`) with raised outward
    /// ignimbrite rim, steep inner caldera wall, and central resurgent dome.
    ResurgentCaldera,
    /// Exposed intrusive plutonic neck/plug (`1.4–3.8 km`) with radiating resistant dike ridges
    /// remaining after the outer cone has weathered away.
    VolcanicNeck,
}

impl VolcanoKind {
    pub fn display_name(self, has_crater_lake: bool, is_caldera: bool) -> &'static str {
        match self {
            Self::Stratovolcano => {
                if has_crater_lake {
                    "Crater Lake"
                } else if is_caldera {
                    "Caldera"
                } else {
                    "Stratovolcano"
                }
            }
            Self::ShieldVolcano => "Shield Volcano",
            Self::ResurgentCaldera => {
                if has_crater_lake {
                    "Caldera Lake"
                } else {
                    "Resurgent Caldera"
                }
            }
            Self::VolcanicNeck => "Volcanic Neck",
        }
    }
}

/// Deterministic flank cinder / scoria cone (`400–1,000 m` diameter) on shield or stratovolcano
/// slopes (`FR-003`), clearly visible at Level 2 (`~32 m/hex`) and Level 3 (`1 m/hex`).
#[derive(Clone, Debug)]
pub struct ParasiticCone {
    pub center_xy_m: [f64; 2],
    pub radius_m: f32,
    pub height_m: f32,
    pub crater_radius_m: f32,
}

/// Deterministic, anchor-invariant volcanic landform (`FR-001`, `FR-002`).
#[derive(Clone, Debug)]
pub struct VolcanicEdifice {
    pub tile_id: usize,
    pub kind: VolcanoKind,
    pub center_xy_m: [f64; 2],
    pub magma_material: MaterialId,
    pub pluton_material: MaterialId,
    pub base_radius_m: f32,
    pub peak_height_m: f32,
    pub caldera_radius_m: f32,
    pub caldera_depth_m: f32,
    pub caldera_floor_z_m: f32,
    pub resurgent_radius_m: f32,
    pub resurgent_height_m: f32,
    pub conduit_radius_m: f32,
    pub dike_angles: [f32; 4],
    pub dike_count: u8,
    pub activity: f32,
    pub crater_lake_z_m: Option<f32>,
    pub crater_lake_state: Option<LakeState>,
    pub parasitic_cones: Vec<ParasiticCone>,
    /// Canonical tile basis `(east_2d, north_2d)` projected into the active anchor plane.
    east_2d: [f64; 2],
    north_2d: [f64; 2],
}

impl VolcanicEdifice {
    /// Point on the summit rim or caldera edge suitable for anchoring a `Jump to:` POI (`FR-007`).
    pub fn poi_rim_xy_m(&self) -> [f64; 2] {
        let rim_off = if self.caldera_radius_m >= 120.0 {
            self.caldera_radius_m as f64
        } else {
            (self.conduit_radius_m as f64) * 0.65
        };
        [
            self.center_xy_m[0] + self.east_2d[0] * rim_off,
            self.center_xy_m[1] + self.east_2d[1] * rim_off,
        ]
    }
}

/// Continuous evaluation of nearby volcanic edifices & parasitic cones at `(x_m, y_m)`.
#[derive(Copy, Clone, Debug)]
pub struct VolcanicQueryResult {
    pub delta_elev_m: f32,
    pub in_caldera: bool,
    /// `(target_floor_z_m, bowl_blend)` where `bowl_blend = 1.0` on the flat caldera floor
    /// and smoothly tapers to `0.0` at the caldera rim (`cu = 1.0`).
    pub caldera_floor_blend: Option<(f32, f32)>,
    pub crater_lake_z_m: Option<f32>,
    pub crater_lake_state: Option<LakeState>,
    pub rim_min_elev_m: Option<f32>,
    pub in_conduit_or_dike: bool,
    pub in_contact_halo: bool,
    pub magma_material: MaterialId,
    pub pluton_material: MaterialId,
    pub sulfur_intensity: f32,
    pub lava_tube_eligible: bool,
    pub radial_dir: [f32; 2],
}

/// Collection of deterministic volcanic edifices active within the 2-ring neighborhood.
#[derive(Clone, Debug, Default)]
pub struct VolcanicField {
    pub edifices: Vec<VolcanicEdifice>,
}

impl VolcanicField {
    /// Derive all volcanic edifices within `anchor`'s 2-ring neighborhood strictly from persisted
    /// `TileGeology` signals (`FR-001`–`FR-004`).
    pub fn build(
        anchor: &LocalZoomAnchor,
        strat: &RegionalStratigraphyField,
        _drainage: &DrainageNetwork,
    ) -> Self {
        let mut edifices = Vec::new();
        let sea_m = anchor.sea_level_m as f32;
        let tile_r = anchor.parent_tile_radius_m.max(15_000.0);

        for nb in &anchor.neighbors {
            let Some((kind, magma_mat, pluton_mat, activity)) = classify_tile_volcanism(nb, sea_m)
            else {
                continue;
            };

            let (east_2d, north_2d) = anchor
                .tile_frames
                .iter()
                .find(|f| f.tile_id == nb.tile_id)
                .map(|f| (f.east_2d, f.north_2d))
                .unwrap_or(([1.0, 0.0], [0.0, 1.0]));

            let seed = anchor.world_seed ^ ((nb.tile_id as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15));
            let u1 = value_noise2(nb.tile_id as f64 * 0.71, 1.3, seed ^ 0x11a1) as f64;
            let u2 = value_noise2(nb.tile_id as f64 * 0.71, 5.7, seed ^ 0x22b2) as f64;
            let u3 = value_noise2(nb.tile_id as f64 * 0.71, 9.1, seed ^ 0x33c3) as f32;
            let u4 = value_noise2(nb.tile_id as f64 * 0.71, 13.7, seed ^ 0x44d4) as f32;

            // Anchor-invariant deterministic offset inside the coarse tile using its canonical frame
            let off_scale = match kind {
                VolcanoKind::ShieldVolcano => 0.18,
                VolcanoKind::ResurgentCaldera => 0.22,
                VolcanoKind::Stratovolcano | VolcanoKind::VolcanicNeck => 0.34,
            };
            let oe = (u1 - 0.5) * 2.0 * tile_r * off_scale;
            let on = (u2 - 0.5) * 2.0 * tile_r * off_scale;
            let cx = nb.local_xy_m[0] + east_2d[0] * oe + north_2d[0] * on;
            let cy = nb.local_xy_m[1] + east_2d[1] * oe + north_2d[1] * on;

            let m_fac = (nb.mountainosity / 6.0).clamp(0.3, 1.4);
            let (
                base_radius_m,
                peak_height_m,
                caldera_radius_m,
                caldera_depth_m,
                resurgent_radius_m,
                resurgent_height_m,
                conduit_radius_m,
            ) = match kind {
                VolcanoKind::Stratovolcano => {
                    let br = (6_200.0 + 5_800.0 * u3).clamp(5_500.0, 12_500.0);
                    let ph = (1_150.0 + 1_350.0 * m_fac * (0.7 + 0.5 * u4)).clamp(1_050.0, 2_850.0);
                    let has_caldera = u4 > 0.52;
                    let cr = if has_caldera {
                        br * (0.22 + 0.14 * u3)
                    } else {
                        br * (0.08 + 0.06 * u3)
                    };
                    let cd = if has_caldera {
                        (240.0 + 240.0 * u4).clamp(200.0, 520.0)
                    } else {
                        (90.0 + 130.0 * u4).clamp(80.0, 240.0)
                    };
                    let (rr, rh_m) = if has_caldera && u3 > 0.45 {
                        (cr * 0.28, cd * 0.52)
                    } else {
                        (0.0, 0.0)
                    };
                    (br, ph, cr, cd, rr, rh_m, (24.0 + 22.0 * u3).clamp(20.0, 48.0))
                }
                VolcanoKind::ShieldVolcano => {
                    let br = (22_000.0 + 20_000.0 * u3).clamp(20_000.0, 44_000.0);
                    let ph = (950.0 + 1_050.0 * (0.6 + 0.6 * u4)).clamp(850.0, 2_100.0);
                    let cr = (1_600.0 + 1_600.0 * u4).clamp(1_500.0, 3_200.0);
                    let cd = (180.0 + 160.0 * u3).clamp(160.0, 340.0);
                    (br, ph, cr, cd, 0.0, 0.0, (28.0 + 26.0 * u4).clamp(24.0, 56.0))
                }
                VolcanoKind::ResurgentCaldera => {
                    let br = (15_000.0 + 11_000.0 * u3).clamp(14_000.0, 26_000.0);
                    let ph = (220.0 + 200.0 * u4).clamp(180.0, 420.0);
                    let cr = br * (0.52 + 0.12 * u4);
                    let cd = (380.0 + 300.0 * u3).clamp(340.0, 720.0);
                    let rr = cr * (0.28 + 0.10 * u3);
                    let rh_m = (cd * 0.58).clamp(160.0, 420.0);
                    (br, ph, cr, cd, rr, rh_m, (35.0 + 28.0 * u3).clamp(30.0, 65.0))
                }
                VolcanoKind::VolcanicNeck => {
                    let br = (750.0 + 1_050.0 * u3).clamp(650.0, 1_900.0);
                    let ph = (240.0 + 360.0 * u4).clamp(220.0, 640.0);
                    (br, ph, 0.0, 0.0, 0.0, 0.0, (32.0 + 28.0 * u3).clamp(28.0, 62.0))
                }
            };

            let dike_count = match kind {
                VolcanoKind::VolcanicNeck => 4u8,
                VolcanoKind::Stratovolcano | VolcanoKind::ShieldVolcano => 3u8,
                VolcanoKind::ResurgentCaldera => 2u8,
            };
            let mut dike_angles = [0.0f32; 4];
            let base_ang = (u3 as f64) * std::f64::consts::TAU;
            for d_idx in 0..4 {
                let jitter = (value_noise2(nb.tile_id as f64, (d_idx as f64) * 3.1, seed ^ 0x55e5) - 0.5) * 0.45;
                dike_angles[d_idx] = (base_ang
                    + (d_idx as f64) * (std::f64::consts::TAU / (dike_count.max(1) as f64))
                    + jitter) as f32;
            }

            // Spawn 3..5 parasitic flank cinder cones on shield volcanoes and stratovolcanoes (`FR-003`)
            let mut parasitic_cones = Vec::new();
            if matches!(kind, VolcanoKind::ShieldVolcano | VolcanoKind::Stratovolcano) {
                let count = if kind == VolcanoKind::ShieldVolcano { 5 } else { 3 };
                for c_idx in 0..count {
                    let a_n = value_noise2(nb.tile_id as f64 * 1.3, (c_idx as f64) * 2.7 + 1.0, seed ^ 0x66f6);
                    let r_n = value_noise2(nb.tile_id as f64 * 1.3, (c_idx as f64) * 2.7 + 5.0, seed ^ 0x7707);
                    let s_n = value_noise2(nb.tile_id as f64 * 1.3, (c_idx as f64) * 2.7 + 9.0, seed ^ 0x8818) as f32;
                    let ang = a_n * std::f64::consts::TAU;
                    let dist = (base_radius_m as f64) * (0.42 + 0.36 * r_n);
                    let ce = ang.cos() * dist;
                    let cn = ang.sin() * dist;
                    let radius_m = (210.0 + 270.0 * s_n).clamp(200.0, 500.0);
                    let height_m = (85.0 + 165.0 * s_n).clamp(80.0, 260.0);
                    parasitic_cones.push(ParasiticCone {
                        center_xy_m: [
                            cx + east_2d[0] * ce + north_2d[0] * cn,
                            cy + east_2d[1] * ce + north_2d[1] * cn,
                        ],
                        radius_m,
                        height_m,
                        crater_radius_m: radius_m * 0.23,
                    });
                }
            }

            // Evaluate actual 360° caldera rim elevation, flat collapsed caldera floor, and
            // climate/permeability-gated crater lake (`FR-004`).
            let ctx = strat.sample_context(anchor, cx, cy);
            let base_e = anchor.sample_history_fine_ground(cx, cy, ctx.base_elevation_m);
            let mut min_rim_e = base_e + peak_height_m;
            if caldera_radius_m > 1.0 {
                min_rim_e = f32::INFINITY;
                for i in 0..16 {
                    let ang = (i as f64) * std::f64::consts::TAU / 16.0;
                    let re = ang.cos() * (caldera_radius_m as f64) * 1.02;
                    let rn = ang.sin() * (caldera_radius_m as f64) * 1.02;
                    let px = cx + east_2d[0] * re + north_2d[0] * rn;
                    let py = cy + east_2d[1] * re + north_2d[1] * rn;
                    let rctx = strat.sample_context(anchor, px, py);
                    let rbase = anchor.sample_history_fine_ground(px, py, rctx.base_elevation_m);
                    let rim_elev = rbase + peak_height_m;
                    if rim_elev < min_rim_e {
                        min_rim_e = rim_elev;
                    }
                }
            }
            // Flat collapsed caldera floor strictly below the lowest 360° rim point
            let caldera_floor_z_m = (min_rim_e - caldera_depth_m * 0.85)
                .min(base_e + peak_height_m - caldera_depth_m);

            let (crater_lake_z_m, crater_lake_state) = if anchor.has_atmosphere && caldera_radius_m >= 500.0 {
                // Keep lake surface strictly below the lowest rim spill point (including relief noise margin)
                let spill_limit = min_rim_e - (caldera_depth_m * 0.35).max(45.0);
                if caldera_floor_z_m > sea_m + 10.0 && spill_limit > caldera_floor_z_m + 12.0 {
                    let col_strata = strat.evaluate_column(&ctx, cx, cy, base_e);
                    let top = &col_strata[0];
                    let clasticity = nb
                        .layers
                        .last()
                        .and_then(|l| l.rock.clasticity)
                        .unwrap_or(0.0);
                    let lstate = classify_lake_state(
                        ctx.precip_norm,
                        ctx.annual_temp_c,
                        ctx.glacier_cover,
                        top.calcity,
                        clasticity,
                        top.toughness.max(0.58), // Volcanic calderas have low-permeability ash/basalt floors
                        top.salinity,
                    );
                    let z_lake = (caldera_floor_z_m
                        + ((spill_limit - caldera_floor_z_m) * 0.50).clamp(12.0, 120.0))
                        .min(spill_limit);
                    (Some(z_lake), Some(lstate))
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            };

            edifices.push(VolcanicEdifice {
                tile_id: nb.tile_id,
                kind,
                center_xy_m: [cx, cy],
                magma_material: magma_mat,
                pluton_material: pluton_mat,
                base_radius_m,
                peak_height_m,
                caldera_radius_m,
                caldera_depth_m,
                caldera_floor_z_m,
                resurgent_radius_m,
                resurgent_height_m,
                conduit_radius_m,
                dike_angles,
                dike_count,
                activity,
                crater_lake_z_m,
                crater_lake_state,
                parasitic_cones,
                east_2d,
                north_2d,
            });
        }

        Self { edifices }
    }

    /// Evaluate combined volcanic relief, caldera depression, crater lake, and 3D plumbing
    /// signals at `(x_m, y_m)`.
    pub fn query(
        &self,
        x_m: f64,
        y_m: f64,
        has_atmosphere: bool,
        world_seed: u64,
    ) -> Option<VolcanicQueryResult> {
        let mut total_delta = 0.0f32;
        let mut any_hit = false;
        let mut in_caldera = false;
        let mut caldera_floor_blend: Option<(f32, f32)> = None;
        let mut crater_lake_z_m: Option<f32> = None;
        let mut crater_lake_state: Option<LakeState> = None;
        let mut rim_min_elev_m: Option<f32> = None;
        let mut in_conduit_or_dike = false;
        let mut in_contact_halo = false;
        let mut magma_material = MaterialId::BASALT;
        let mut pluton_material = MaterialId::GABBRO;
        let mut sulfur_intensity = 0.0f32;
        let mut lava_tube_eligible = false;
        let mut radial_dir = [1.0f32, 0.0];
        let mut best_priority = -1.0f32;

        for ed in &self.edifices {
            let dx = x_m - ed.center_xy_m[0];
            let dy = y_m - ed.center_xy_m[1];
            let max_r = (ed.base_radius_m as f64) * 1.15;
            if dx.abs() > max_r || dy.abs() > max_r {
                continue;
            }
            let r = (dx * dx + dy * dy).sqrt() as f32;
            if r > ed.base_radius_m * 1.15 {
                continue;
            }

            any_hit = true;
            // Express `(dx, dy)` in the edifice's canonical `(east_2d, north_2d)` frame so
            // radial gullies and dike azimuths are 100% invariant across re-anchoring (`SC-004`).
            let lx = dx * ed.east_2d[0] + dy * ed.east_2d[1];
            let ly = dx * ed.north_2d[0] + dy * ed.north_2d[1];
            let theta = ly.atan2(lx) as f32;
            let u = (r / ed.base_radius_m).clamp(0.0, 1.15);

            // 1. Primary edifice relief profile
            // For all crater/caldera-bearing edifices, truncate the pre-collapse summit flat at
            // `rim_u = caldera_radius_m / base_radius_m` so the rim sits at `peak_height_m` and
            // Step 2 carves a genuine steep-walled bowl strictly below the rim.
            let mut d_elev = match ed.kind {
                VolcanoKind::Stratovolcano => {
                    if u < 1.0 {
                        let rim_u = (ed.caldera_radius_m / ed.base_radius_m).clamp(0.05, 0.45);
                        let t = if u <= rim_u {
                            0.0
                        } else {
                            ((u - rim_u) / (1.0 - rim_u).max(0.05)).clamp(0.0, 1.0)
                        };
                        // Steep concave composite cone tapering smoothly from rim to 0 at u = 1
                        let core = (-2.35 * t.powf(1.15)).exp();
                        let rim_fade = 1.0 - smoothstep(0.80, 1.0, t);
                        let mut h = ed.peak_height_m * core * rim_fade;
                        // Radial barranco erosion gullies on outer flanks of atmospheric worlds
                        if has_atmosphere && t > 0.06 && t < 0.92 {
                            let gully = (theta * 11.0).sin().abs().powf(0.75) * 0.65
                                + (theta * 23.0 + 1.4).sin().abs() * 0.35;
                            let g_env = smoothstep(0.06, 0.24, t) * (1.0 - smoothstep(0.74, 0.92, t));
                            h -= h * 0.16 * (1.0 - gully) * g_env;
                        }
                        h
                    } else {
                        0.0
                    }
                }
                VolcanoKind::ShieldVolcano => {
                    if u < 1.0 {
                        let rim_u = (ed.caldera_radius_m / ed.base_radius_m).clamp(0.04, 0.25);
                        let t = if u <= rim_u {
                            0.0
                        } else {
                            ((u - rim_u) / (1.0 - rim_u).max(0.05)).clamp(0.0, 1.0)
                        };
                        // Broad gentle basalt shield (< 8 deg slope) tapering smoothly from rim to 0
                        ed.peak_height_m * (1.0 - t.powf(1.6)).max(0.0).powf(1.35)
                    } else {
                        0.0
                    }
                }
                VolcanoKind::ResurgentCaldera => {
                    if u < 1.0 {
                        // Outward-sloping ignimbrite apron peaking at caldera rim
                        let rim_u = (ed.caldera_radius_m / ed.base_radius_m).clamp(0.3, 0.75);
                        if u >= rim_u {
                            let t = ((u - rim_u) / (1.0 - rim_u).max(0.05)).clamp(0.0, 1.0);
                            ed.peak_height_m * (1.0 - smoothstep(0.0, 1.0, t))
                        } else {
                            ed.peak_height_m
                        }
                    } else {
                        0.0
                    }
                }
                VolcanoKind::VolcanicNeck => {
                    let mut h = if u < 1.0 {
                        ed.peak_height_m * (1.0 - u * u).max(0.0).powf(0.65)
                    } else {
                        0.0
                    };
                    // Resistant radiating dike walls around the eroded volcanic plug
                    let dike_max_r = ed.base_radius_m * 1.12;
                    if r > ed.base_radius_m * 0.35 && r < dike_max_r {
                        let min_dike_dist = min_dike_distance(r, theta, &ed.dike_angles, ed.dike_count);
                        if min_dike_dist < 42.0 {
                            let d_t = 1.0 - smoothstep(10.0, 42.0, min_dike_dist);
                            let r_fade = 1.0 - smoothstep(ed.base_radius_m * 0.5, dike_max_r, r);
                            h = h.max(ed.peak_height_m * 0.38 * d_t * r_fade);
                        }
                    }
                    h
                }
            };

            // 2. Summit crater or collapse caldera bowl + optional central resurgent dome
            let mut ed_in_caldera = false;
            if ed.caldera_radius_m > 1.0 && r < ed.caldera_radius_m * 1.12 {
                let cu = (r / ed.caldera_radius_m).clamp(0.0, 1.12);
                if cu < 1.0 {
                    ed_in_caldera = true;
                    // Flat collapsed caldera floor blending steeply up the inner wall to the rim
                    let bowl_blend = 1.0 - smoothstep(0.74, 1.0, cu);
                    let mut target_floor = ed.caldera_floor_z_m;
                    // Central resurgent dome inside large collapse / resurgent calderas
                    if ed.resurgent_radius_m > 1.0 && r < ed.resurgent_radius_m {
                        let ru = (r / ed.resurgent_radius_m).clamp(0.0, 1.0);
                        target_floor += ed.resurgent_height_m * (1.0 - ru * ru).powf(1.25);
                    }
                    // Subtle bowl concavity (4% deeper at center) for natural bathymetry
                    target_floor -= ed.caldera_depth_m * 0.04 * (1.0 - cu * cu);
                    caldera_floor_blend = Some((target_floor, bowl_blend));
                } else {
                    // Subtle raised crater/caldera rim collar
                    let collar_t = 1.0 - ((cu - 1.0) / 0.12).clamp(0.0, 1.0);
                    d_elev += ed.caldera_depth_m * 0.08 * collar_t * collar_t;
                }
                // Non-negative rim spill-point guard when a crater lake is present so water never
                // appears above the surrounding crater rim even where regional terrain noise dips.
                if let Some(z_lake) = ed.crater_lake_z_m {
                    if cu >= 0.94 && cu <= 1.12 {
                        let guard_t = if cu < 1.0 {
                            smoothstep(0.94, 1.0, cu)
                        } else {
                            1.0 - smoothstep(1.0, 1.12, cu)
                        };
                        let min_rim = z_lake + 12.0 * guard_t;
                        rim_min_elev_m = Some(rim_min_elev_m.map_or(min_rim, |m| m.max(min_rim)));
                    }
                }
            }

            // 3. Flank parasitic cinder / scoria cones (`FR-003`)
            for cone in &ed.parasitic_cones {
                let cdx = x_m - cone.center_xy_m[0];
                let cdy = y_m - cone.center_xy_m[1];
                let cr_max = cone.radius_m as f64;
                if cdx.abs() > cr_max || cdy.abs() > cr_max {
                    continue;
                }
                let cr = (cdx * cdx + cdy * cdy).sqrt() as f32;
                if cr < cone.radius_m {
                    let cu = cr / cone.radius_m;
                    let mut ch = cone.height_m * (1.0 - smoothstep(0.0, 1.0, cu)).powf(0.85);
                    if cr < cone.crater_radius_m {
                        let k = 1.0 - smoothstep(0.45, 1.0, cr / cone.crater_radius_m);
                        ch -= cone.height_m * 0.28 * k;
                    }
                    d_elev += ch;
                }
            }

            total_delta += d_elev;

            // 4. 3D Subsurface conduit, radial dikes, contact aureole, sulfur fumaroles, lava tubes (`FR-006`)
            let mut ed_conduit = r <= ed.conduit_radius_m;
            let mut ed_halo = !ed_conduit && r <= ed.conduit_radius_m + 8.0;
            if !ed_conduit && r <= ed.base_radius_m * 0.90 {
                let d_dike = min_dike_distance(r, theta, &ed.dike_angles, ed.dike_count);
                let dike_half_w = if ed.kind == VolcanoKind::VolcanicNeck { 13.0 } else { 8.0 };
                if d_dike <= dike_half_w {
                    ed_conduit = true;
                } else if d_dike <= dike_half_w + 6.0 {
                    ed_halo = true;
                }
            }

            let mut ed_sulfur = 0.0f32;
            if ed.activity > 0.35 && ed.caldera_radius_m > 1.0 {
                let rim_dist = (r - ed.caldera_radius_m).abs();
                let on_rim = rim_dist < (ed.caldera_radius_m * 0.14).clamp(20.0, 220.0);
                let on_dome = ed.resurgent_radius_m > 1.0 && r < ed.resurgent_radius_m * 0.85;
                if on_rim || on_dome {
                    let sn = fbm2(x_m / 24.0, y_m / 24.0, world_seed ^ 0x501f_0001, 2);
                    if sn > 0.18 {
                        ed_sulfur = (((sn - 0.18) / 0.65) * ed.activity).clamp(0.0, 1.0);
                    }
                }
            }

            let ed_lava_tube = ed.kind == VolcanoKind::ShieldVolcano
                && ed.activity > 0.25
                && r > ed.caldera_radius_m * 1.15
                && r < ed.base_radius_m * 0.85;

            let prio = d_elev.abs() + if ed_in_caldera { 1000.0 } else { 0.0 } + if ed_conduit { 500.0 } else { 0.0 };
            if prio > best_priority {
                best_priority = prio;
                magma_material = ed.magma_material;
                pluton_material = ed.pluton_material;
                if r > 1e-3 {
                    radial_dir = [(dx as f32) / r, (dy as f32) / r];
                }
            }
            if ed_in_caldera {
                in_caldera = true;
                if ed.crater_lake_z_m.is_some() {
                    crater_lake_z_m = ed.crater_lake_z_m;
                    crater_lake_state = ed.crater_lake_state;
                }
            }
            in_conduit_or_dike |= ed_conduit;
            in_contact_halo |= ed_halo;
            sulfur_intensity = sulfur_intensity.max(ed_sulfur);
            lava_tube_eligible |= ed_lava_tube;
        }

        if !any_hit {
            return None;
        }

        Some(VolcanicQueryResult {
            delta_elev_m: total_delta,
            in_caldera,
            caldera_floor_blend,
            crater_lake_z_m,
            crater_lake_state,
            rim_min_elev_m,
            in_conduit_or_dike,
            in_contact_halo,
            magma_material,
            pluton_material,
            sulfur_intensity,
            lava_tube_eligible,
            radial_dir,
        })
    }
}

/// Classify whether a coarse tile should host a volcanic edifice and its magma materials
/// strictly from persisted `TileGeology` signals (`FR-001`, `FR-002`).
pub fn classify_tile_volcanism(
    nb: &NeighborTileSample,
    sea_m: f32,
) -> Option<(VolcanoKind, MaterialId, MaterialId, f32)> {
    let top_rock = &nb.layers.last()?.rock;
    if top_rock.kind != RockKind::Igneous {
        // Active subduction margins with high mountainosity can still host arc stratovolcanoes
        // erupting through sedimentary/metamorphic cover.
        if nb.subduction > 0.25 && nb.elevation_m > sea_m && nb.mountainosity >= 2.2 {
            let act = (0.45 + nb.subduction * 0.55).clamp(0.45, 1.0);
            return Some((
                VolcanoKind::Stratovolcano,
                MaterialId::ANDESITE,
                MaterialId::DIORITE,
                act,
            ));
        }
        return None;
    }

    let is_intrusive = matches!(top_rock.name, "gabbro" | "diorite" | "granite");
    let felsity = top_rock.felsity;

    if is_intrusive {
        // Eroded volcanic neck / plutonic plug when intrusive rock is exposed with an emplaced
        // intrusion or residual volcanic relief on non-subducting terrain.
        if nb.has_intrusion || (nb.subduction <= 0.05 && nb.mountainosity >= 1.8 && nb.elevation_m > sea_m) {
            let pluton = MaterialId::from_rock(top_rock);
            let act = if nb.has_intrusion { 0.30 } else { 0.10 };
            return Some((VolcanoKind::VolcanicNeck, pluton, pluton, act));
        }
        return None;
    }

    // Extrusive igneous rock (`basalt`, `andesite`, `rhyolite`)
    if felsity < 0.46 {
        // Mafic basalt -> Shield Volcano (emergent hotspot island, subaerial flood/shield, or arc seamount)
        if nb.elevation_m > sea_m || nb.subduction > 0.05 || nb.has_intrusion || nb.mountainosity >= 1.2 {
            let act = (0.40 + nb.subduction * 0.4 + if nb.has_intrusion { 0.35 } else { 0.0 }).clamp(0.35, 1.0);
            return Some((
                VolcanoKind::ShieldVolcano,
                MaterialId::BASALT,
                MaterialId::GABBRO,
                act,
            ));
        }
        None
    } else if felsity < 0.66 {
        // Intermediate andesite -> Stratovolcano (arc or elevated volcanic mountain)
        if nb.subduction > 0.05 || nb.has_intrusion || nb.mountainosity >= 1.6 {
            let act = (0.45 + nb.subduction * 0.5 + if nb.has_intrusion { 0.25 } else { 0.0 }).clamp(0.40, 1.0);
            return Some((
                VolcanoKind::Stratovolcano,
                MaterialId::ANDESITE,
                MaterialId::DIORITE,
                act,
            ));
        }
        None
    } else {
        // Felsic rhyolite -> Resurgent Caldera / Supervolcano
        if nb.subduction > 0.05 || nb.has_intrusion || nb.mountainosity >= 2.0 {
            let act = (0.50 + nb.subduction * 0.4 + if nb.has_intrusion { 0.35 } else { 0.0 }).clamp(0.45, 1.0);
            return Some((
                VolcanoKind::ResurgentCaldera,
                MaterialId::RHYOLITE,
                MaterialId::GRANITE,
                act,
            ));
        }
        None
    }
}

#[inline]
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0).max(1e-6)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[inline]
fn min_dike_distance(r: f32, theta: f32, angles: &[f32; 4], count: u8) -> f32 {
    let mut best = f32::INFINITY;
    for &ang in angles.iter().take((count as usize).min(4)) {
        let mut diff = (theta - ang).rem_euclid(std::f32::consts::TAU);
        if diff > std::f32::consts::PI {
            diff = std::f32::consts::TAU - diff;
        }
        let d = r * diff.sin().abs();
        if diff < std::f32::consts::FRAC_PI_2 && d < best {
            best = d;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anchor::{SnapshotStage, TileCanonicalFrame};
    use tec11_geology::rock::{Rock, RockLayer};

    fn make_anchor(
        rock: Rock,
        subduction: f32,
        has_intrusion: bool,
        mountainosity: f32,
        precip_norm: f32,
        annual_temp_c: f32,
        has_atmosphere: bool,
    ) -> LocalZoomAnchor {
        LocalZoomAnchor {
            world_seed: 42_009,
            stage: if has_atmosphere {
                SnapshotStage::History
            } else {
                SnapshotStage::Tectonics
            },
            grid_size: 6,
            radius_km: 6371.0,
            parent_tile_id: 77,
            parent_tile_radius_m: 150_000.0,
            anchor_normal: [0.0, 0.0, 1.0],
            tangent_east: [1.0, 0.0, 0.0],
            tangent_north: [0.0, 1.0, 0.0],
            sea_level_m: 0.0,
            has_atmosphere,
            has_life: has_atmosphere,
            is_pentagon: false,
            neighbors: vec![NeighborTileSample {
                tile_id: 77,
                local_xy_m: [0.0, 0.0],
                elevation_m: 850.0,
                bottom_m: -9000.0,
                mountainosity,
                subduction,
                has_intrusion,
                impact_scar: 0.0,
                soil_depth_m: 1.5,
                soil_fertility: 1.2,
                is_glaciated: false,
                glacier_cover: 0.0,
                annual_temp_c,
                precip_norm,
                tidal_range_m: 1.5,
                wind_vec: [1.0, 0.0],
                layers: vec![RockLayer {
                    rock,
                    thickness: 9.85,
                }],
                occurrences: vec![],
            }],
            river_edges: vec![],
            fine_samples: vec![],
            fine_spacing_m: 0.0,
            tile_frames: vec![TileCanonicalFrame {
                tile_id: 77,
                east_2d: [1.0, 0.0],
                north_2d: [0.0, 1.0],
            }],
        }
    }

    #[test]
    fn four_volcanic_morphologies_and_craton_exclusion() {
        let empty_drain = DrainageNetwork::default();

        // 1. Stratovolcano (andesite + subduction)
        let strato_a = make_anchor(Rock::extrusive(0.56), 0.75, false, 4.5, 0.7, 12.0, true);
        let strato_s = RegionalStratigraphyField::from_anchor(&strato_a);
        let strato_f = VolcanicField::build(&strato_a, &strato_s, &empty_drain);
        assert_eq!(strato_f.edifices.len(), 1);
        assert_eq!(strato_f.edifices[0].kind, VolcanoKind::Stratovolcano);
        assert!(!strato_f.edifices[0].parasitic_cones.is_empty());

        // 2. Shield Volcano (basalt hotspot, gentle < 8 deg slope)
        let shield_a = make_anchor(Rock::extrusive(0.38), 0.0, true, 2.0, 0.6, 22.0, true);
        let shield_s = RegionalStratigraphyField::from_anchor(&shield_a);
        let shield_f = VolcanicField::build(&shield_a, &shield_s, &empty_drain);
        assert_eq!(shield_f.edifices.len(), 1);
        let sh = &shield_f.edifices[0];
        assert_eq!(sh.kind, VolcanoKind::ShieldVolcano);
        let avg_slope_deg = (sh.peak_height_m / sh.base_radius_m).atan().to_degrees();
        assert!(avg_slope_deg < 8.0, "Shield slope should be < 8 deg (got {avg_slope_deg:.2})");

        // 3. Resurgent Caldera (rhyolite + intrusion, caldera floor strictly below rim)
        let cald_a = make_anchor(Rock::extrusive(0.74), 0.5, true, 3.5, 0.75, 10.0, true);
        let cald_s = RegionalStratigraphyField::from_anchor(&cald_a);
        let cald_f = VolcanicField::build(&cald_a, &cald_s, &empty_drain);
        assert_eq!(cald_f.edifices.len(), 1);
        let ca = &cald_f.edifices[0];
        assert_eq!(ca.kind, VolcanoKind::ResurgentCaldera);
        let rim = cald_f
            .query(ca.center_xy_m[0] + (ca.caldera_radius_m as f64) * 1.02, ca.center_xy_m[1], true, cald_a.world_seed)
            .unwrap();
        let moat = cald_f
            .query(ca.center_xy_m[0] + (ca.caldera_radius_m as f64) * 0.60, ca.center_xy_m[1], true, cald_a.world_seed)
            .unwrap();
        let rim_z = 900.0 + rim.delta_elev_m;
        let (moat_floor_z, moat_blend) = moat
            .caldera_floor_blend
            .expect("Caldera moat must provide flat caldera_floor_blend");
        let moat_z = (900.0 + moat.delta_elev_m) * (1.0 - moat_blend) + moat_floor_z * moat_blend;
        assert!(
            moat_z < rim_z - 100.0,
            "Caldera moat ({moat_z:.1} m) must sit well below rim ({rim_z:.1} m)"
        );
        assert_eq!(ca.crater_lake_state, Some(LakeState::Freshwater));

        // 4. Eroded Volcanic Neck (exposed granite + intrusion, no subduction)
        let neck_a = make_anchor(Rock::intrusive(0.72), 0.0, true, 2.2, 0.5, 14.0, true);
        let neck_s = RegionalStratigraphyField::from_anchor(&neck_a);
        let neck_f = VolcanicField::build(&neck_a, &neck_s, &empty_drain);
        assert_eq!(neck_f.edifices.len(), 1);
        assert_eq!(neck_f.edifices[0].kind, VolcanoKind::VolcanicNeck);

        // 5. Sedimentary craton (no subduction, no intrusion) -> zero volcanoes
        let sandstone = Rock {
            kind: RockKind::Sedimentary,
            name: "sandstone",
            felsity: 0.7,
            toughness: 0.6,
            clasticity: Some(4.0),
            calcity: None,
            bogginess: None,
            quartzity: Some(0.8),
            sulfidity: None,
            salinity: None,
        };
        let craton_a = make_anchor(sandstone, 0.0, false, 0.4, 0.5, 15.0, true);
        let craton_s = RegionalStratigraphyField::from_anchor(&craton_a);
        let craton_f = VolcanicField::build(&craton_a, &craton_s, &empty_drain);
        assert!(craton_f.edifices.is_empty());

        // 6. Airless Hadean shield -> edifice exists, zero crater lake
        let hadean_a = make_anchor(Rock::extrusive(0.38), 0.0, true, 2.0, 0.0, 20.0, false);
        let hadean_s = RegionalStratigraphyField::from_anchor(&hadean_a);
        let hadean_f = VolcanicField::build(&hadean_a, &hadean_s, &empty_drain);
        assert_eq!(hadean_f.edifices.len(), 1);
        assert!(hadean_f.edifices[0].crater_lake_z_m.is_none());
    }
}
