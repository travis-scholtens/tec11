//! Laterally continuous stratigraphic blending (`RegionalStratigraphyField`), neighbor bed
//! correlation, geological pinch-outs/lenses/interfingering wedges, structural folding/faulting/
//! intrusions/impact bowls, and discrete ore/mineral vein placement (`SC-006`, `FR-004`).

use tec11_geology::rock::{Rock, RockKind};

use crate::anchor::{LayerMineralHint, LocalZoomAnchor};
use crate::materials::MaterialId;

/// Deterministic mineral vein specification attached to a `CorrelatedStratum`.
#[derive(Clone, Debug)]
pub struct MineralVeinSpec {
    pub material: MaterialId,
    pub density: f32,
    pub is_interface: bool,
}

/// Unified stratigraphic bed correlated across the anchor tile and its 1-ring neighbors
/// (`data-model.md`: `CorrelatedStratum`).
#[derive(Clone, Debug)]
pub struct CorrelatedStratum {
    pub stratum_id: u16,
    pub rock: Rock,
    pub material: MaterialId,
    /// Thickness in meters at each neighbor index (`0.0` where absent → tapers to pinch-out wedge).
    pub thickness_by_neighbor_m: Vec<f32>,
    pub mineral_veins: Vec<MineralVeinSpec>,
}

/// Interpolated continuous planetary fields at a tangent-plane point `(x_m, y_m)`.
#[derive(Clone, Debug)]
pub struct InterpolatedLocalContext {
    pub base_elevation_m: f32,
    pub mountainosity: f32,
    pub subduction: f32,
    pub impact_scar: f32,
    pub soil_depth_m: f32,
    pub soil_fertility: f32,
    pub glacier_cover: f32,
    pub is_glaciated: bool,
    pub annual_temp_c: f32,
    pub precip_norm: f32,
    pub tidal_range_m: f32,
    pub wind_vec: [f32; 2],
    pub weights: Vec<f32>,
}

/// Evaluated stratigraphic column at `(x_m, y_m)` ordered top-to-bottom.
#[derive(Clone, Debug)]
pub struct LocalStratumSlice {
    pub stratum_id: u16,
    pub material: MaterialId,
    pub rock_kind: RockKind,
    pub toughness: f32,
    pub calcity: f32,
    pub bogginess: f32,
    pub salinity: f32,
    pub felsity: f32,
    pub top_z_m: f32,
    pub bottom_z_m: f32,
    pub mineral_veins: Vec<MineralVeinSpec>,
}

#[derive(Clone, Debug)]
pub struct RegionalStratigraphyField {
    pub world_seed: u64,
    pub neighbor_xy_m: Vec<[f64; 2]>,
    pub strata: Vec<CorrelatedStratum>,
    pub parent_tile_radius_m: f64,
}

impl RegionalStratigraphyField {
    /// Build a correlated stratigraphic field from `anchor` and its 5–6 neighbors.
    /// Shared rock layers (matching `RockKind`, `name`, and felsity band in top-down order)
    /// are correlated so their thicknesses interpolate smoothly across tile boundaries;
    /// non-shared beds get `0.0` thickness in neighbors where absent, tapering into pinch-out wedges.
    pub fn from_anchor(anchor: &LocalZoomAnchor) -> Self {
        let num_nb = anchor.neighbors.len();
        let neighbor_xy_m: Vec<[f64; 2]> = anchor.neighbors.iter().map(|n| n.local_xy_m).collect();

        let mut strata: Vec<CorrelatedStratum> = Vec::new();
        let mut next_id = 0u16;

        // First seed strata from the center tile (index 0) top-to-bottom, then correlate each neighbor.
        for (nb_idx, nb) in anchor.neighbors.iter().enumerate() {
            let mut matched_strata = vec![false; strata.len()];
            // Top-down iteration (layers in TileGeology are bottom-to-top)
            for (top_down_idx, layer) in nb.layers.iter().rev().enumerate() {
                let thick_m = (layer.thickness * 1000.0).max(0.5);
                let mat = MaterialId::from_rock(&layer.rock);

                // Collect mineral hints for this layer
                let layer_veins: Vec<MineralVeinSpec> = nb
                    .occurrences
                    .iter()
                    .filter(|o| o.layer_index_from_top == top_down_idx)
                    .map(|o: &LayerMineralHint| MineralVeinSpec {
                        material: o.material,
                        density: (o.grade * 0.5 + o.abundance * 0.5).clamp(0.05, 1.0),
                        is_interface: o.is_interface,
                    })
                    .collect();

                // Try to match an existing correlated stratum of the same material/rock kind not yet matched for this neighbor
                let found = strata.iter().enumerate().position(|(s_idx, s)| {
                    !matched_strata[s_idx]
                        && s.material == mat
                        && s.rock.kind == layer.rock.kind
                        && (s.rock.felsity - layer.rock.felsity).abs() <= 0.18
                });

                if let Some(s_idx) = found {
                    matched_strata[s_idx] = true;
                    strata[s_idx].thickness_by_neighbor_m[nb_idx] += thick_m;
                    for v in layer_veins {
                        if !strata[s_idx]
                            .mineral_veins
                            .iter()
                            .any(|e| e.material == v.material)
                        {
                            strata[s_idx].mineral_veins.push(v);
                        }
                    }
                } else {
                    let mut thicks = vec![0.0f32; num_nb];
                    thicks[nb_idx] = thick_m;
                    strata.push(CorrelatedStratum {
                        stratum_id: next_id,
                        rock: layer.rock.clone(),
                        material: mat,
                        thickness_by_neighbor_m: thicks,
                        mineral_veins: layer_veins,
                    });
                    matched_strata.push(true);
                    next_id = next_id.saturating_add(1);
                }
            }
        }

        Self {
            world_seed: anchor.world_seed,
            neighbor_xy_m,
            strata,
            parent_tile_radius_m: anchor.parent_tile_radius_m.max(1000.0),
        }
    }

    /// Smooth normalized radial weights $w_i(x, y)$ across the center + 1-ring neighbor samples.
    /// Uses an interpolating inverse-distance kernel (`1 / ((d / R)^3 + eps)`) so that at each
    /// tile center `w_i -> 1.0` (preserving isolated islands and mountain peaks instead of drowning
    /// them with deep-ocean neighbors), while transitioning $C^\infty$-smoothly across boundaries.
    pub fn spatial_weights(&self, x_m: f64, y_m: f64) -> Vec<f32> {
        let n = self.neighbor_xy_m.len();
        let mut weights = vec![0.0f32; n];
        if n == 0 {
            return weights;
        }
        let r_ref = self.parent_tile_radius_m.max(1000.0);
        let eps = 0.002f64;
        let mut sum = 0.0f64;
        for (i, &[nx, ny]) in self.neighbor_xy_m.iter().enumerate() {
            let dx = x_m - nx;
            let dy = y_m - ny;
            let u = (dx * dx + dy * dy).sqrt() / r_ref;
            let w = 1.0 / (u * u * u + eps);
            weights[i] = w as f32;
            sum += w;
        }
        if sum > 1e-12 {
            let inv = (1.0 / sum) as f32;
            for w in &mut weights {
                *w *= inv;
            }
        } else {
            weights[0] = 1.0;
        }
        weights
    }

    /// Interpolate all continuous planetary context fields at `(x_m, y_m)`.
    pub fn sample_context(
        &self,
        anchor: &LocalZoomAnchor,
        x_m: f64,
        y_m: f64,
    ) -> InterpolatedLocalContext {
        let weights = self.spatial_weights(x_m, y_m);
        let mut base_elevation_m = 0.0f32;
        let mut mountainosity = 0.0f32;
        let mut subduction = 0.0f32;
        let mut impact_scar = 0.0f32;
        let mut soil_depth_m = 0.0f32;
        let mut soil_fertility = 0.0f32;
        let mut glacier_cover = 0.0f32;
        let mut annual_temp_c = 0.0f32;
        let mut precip_norm = 0.0f32;
        let mut tidal_range_m = 0.0f32;
        let mut wind_x = 0.0f32;
        let mut wind_y = 0.0f32;

        for (i, nb) in anchor.neighbors.iter().enumerate() {
            let w = weights[i];
            base_elevation_m += w * nb.elevation_m;
            mountainosity += w * nb.mountainosity;
            subduction += w * nb.subduction;
            impact_scar += w * nb.impact_scar;
            soil_depth_m += w * nb.soil_depth_m;
            soil_fertility += w * nb.soil_fertility;
            glacier_cover += w * nb.glacier_cover;
            annual_temp_c += w * nb.annual_temp_c;
            precip_norm += w * nb.precip_norm;
            tidal_range_m += w * nb.tidal_range_m;
            wind_x += w * nb.wind_vec[0];
            wind_y += w * nb.wind_vec[1];
        }

        InterpolatedLocalContext {
            base_elevation_m,
            mountainosity,
            subduction,
            impact_scar,
            soil_depth_m,
            soil_fertility,
            glacier_cover,
            is_glaciated: glacier_cover >= 0.35,
            annual_temp_c,
            precip_norm,
            tidal_range_m,
            wind_vec: [wind_x, wind_y],
            weights,
        }
    }

    /// Continuous structural deformation (vertical horizon shift in meters) at `(x_m, y_m)`:
    /// combines regional dip, mountainosity-driven anticline/syncline folding, and fault throws.
    pub fn structural_warp_m(&self, ctx: &InterpolatedLocalContext, x_m: f64, y_m: f64) -> f32 {
        let m_norm = (ctx.mountainosity / 10.0).clamp(0.0, 1.5);
        // Gentle regional bedding undulation even on flat plateaus (~8 m) scaling up to ~180 m in orogens
        let fold_amp = 8.0 + 140.0 * m_norm;
        // Strike direction derived deterministically from world seed
        let angle = ((self.world_seed % 360) as f64).to_radians();
        let (ca, sa) = (angle.cos(), angle.sin());
        let u = x_m * ca - y_m * sa;
        let v = x_m * sa + y_m * ca;

        let primary_fold = fbm2(u / 650.0, v / 2400.0, self.world_seed ^ 0x51a7, 3);
        let secondary_fold = fbm2(u / 180.0, v / 520.0, self.world_seed ^ 0x9b31, 2);

        // Step-like fault offset in high-mountainosity or subduction zones
        let fault_amp = (ctx.mountainosity * 4.5 + ctx.subduction * 35.0).clamp(0.0, 65.0);
        let fault_phase = fbm2(u / 420.0, v / 900.0, self.world_seed ^ 0xf407, 2);
        let fault_step = ((fault_phase * 5.0).tanh()) * fault_amp;

        // Impact crater bowl depression / raised rim if impact_scar > 0
        let impact_warp = if ctx.impact_scar > 0.25 {
            let r_km = ((x_m * x_m + y_m * y_m).sqrt() / 1000.0) as f32;
            let crater_r = 2.5f32;
            let norm_r = r_km / crater_r;
            if norm_r < 1.0 {
                -60.0 * ctx.impact_scar * (1.0 - norm_r * norm_r)
            } else if norm_r < 1.4 {
                let t = 1.0 - (norm_r - 1.0) / 0.4;
                22.0 * ctx.impact_scar * t * t
            } else {
                0.0
            }
        } else {
            0.0
        };

        primary_fold * fold_amp + secondary_fold * (fold_amp * 0.28) + fault_step + impact_warp
    }

    /// Evaluate the laterally continuous stratigraphic column at `(x_m, y_m)` anchored at `surface_z_m`.
    /// Non-shared beds taper smoothly to zero via pinch-out noise; intrusive rocks form 3D bulbous
    /// bodies; strata shift vertically by `structural_warp_m` so dipping beds outcrop naturally.
    pub fn evaluate_column(
        &self,
        ctx: &InterpolatedLocalContext,
        x_m: f64,
        y_m: f64,
        surface_z_m: f32,
    ) -> Vec<LocalStratumSlice> {
        let warp_m = self.structural_warp_m(ctx, x_m, y_m);
        let mut out = Vec::with_capacity(self.strata.len());

        // Start stratigraphic top shifted by structural folding/dip (erosion truncates anything above surface_z_m)
        let mut cur_top_z = surface_z_m + warp_m * 0.35;

        for s in &self.strata {
            let mut raw_thick = 0.0f32;
            let mut min_t = f32::INFINITY;
            let mut max_t = 0.0f32;
            for (i, &w) in ctx.weights.iter().enumerate() {
                let ti = s.thickness_by_neighbor_m.get(i).copied().unwrap_or(0.0);
                raw_thick += w * ti;
                min_t = min_t.min(ti);
                max_t = max_t.max(ti);
            }

            // Pinch-out / interfingering modulation when a bed is present on some neighbors and absent on others
            let is_pinching = min_t < 0.01 && max_t > 0.01;
            let effective_thick = if is_pinching {
                let pinch_noise = fbm2(
                    x_m / 480.0 + (s.stratum_id as f64) * 3.7,
                    y_m / 480.0 - (s.stratum_id as f64) * 2.3,
                    self.world_seed ^ (s.stratum_id as u64).wrapping_mul(0x9e3779b9),
                    2,
                );
                // Taper smoothly to zero with natural wavy wedge boundary
                let modulated = raw_thick + pinch_noise * (max_t * 0.18);
                if modulated < 1.5 {
                    0.0
                } else {
                    modulated
                }
            } else {
                // Even continuous beds gently thicken/thin laterally (±12%)
                let swell = fbm2(
                    x_m / 900.0 + (s.stratum_id as f64) * 5.1,
                    y_m / 900.0,
                    self.world_seed ^ (s.stratum_id as u64).wrapping_mul(0x517cc1b7),
                    2,
                );
                (raw_thick * (1.0 + 0.12 * swell)).max(0.0)
            };

            if effective_thick < 0.5 {
                continue;
            }

            let cur_bot_z = cur_top_z - effective_thick;
            out.push(LocalStratumSlice {
                stratum_id: s.stratum_id,
                material: s.material,
                rock_kind: s.rock.kind,
                toughness: s.rock.toughness,
                calcity: s.rock.calcity.unwrap_or(0.0),
                bogginess: s.rock.bogginess.unwrap_or(0.0),
                salinity: s.rock.salinity.unwrap_or(0.0),
                felsity: s.rock.felsity,
                top_z_m: cur_top_z,
                bottom_z_m: cur_bot_z,
                mineral_veins: s.mineral_veins.clone(),
            });
            cur_top_z = cur_bot_z;
        }

        // Guarantee at least one basement stratum if all layers pinched out
        if out.is_empty() {
            out.push(LocalStratumSlice {
                stratum_id: 0,
                material: MaterialId::GRANITE,
                rock_kind: RockKind::Igneous,
                toughness: 0.75,
                calcity: 0.0,
                bogginess: 0.0,
                salinity: 0.0,
                felsity: 0.72,
                top_z_m: surface_z_m + 50.0,
                bottom_z_m: surface_z_m - 10_000.0,
                mineral_veins: Vec::new(),
            });
        }
        out
    }

    /// Sample the bedrock/vein `MaterialId` at 3D point `(x_m, y_m, z_m)` from an evaluated column.
    pub fn sample_bedrock_material<'a>(
        &self,
        column: &'a [LocalStratumSlice],
        x_m: f64,
        y_m: f64,
        z_m: f32,
    ) -> (MaterialId, &'a LocalStratumSlice) {
        // Find the stratum containing `z_m` (if z_m is above the first top_z_m due to folding, use the top stratum)
        let slice = column
            .iter()
            .find(|s| z_m <= s.top_z_m && z_m > s.bottom_z_m)
            .unwrap_or_else(|| {
                if z_m > column[0].top_z_m {
                    &column[0]
                } else {
                    column.last().unwrap()
                }
            });

        // Check for discrete ore/mineral vein voxels inside mineralized strata or near layer contacts
        if !slice.mineral_veins.is_empty() {
            let near_contact = (slice.top_z_m - z_m).abs() < 3.5 || (z_m - slice.bottom_z_m).abs() < 3.5;
            for (v_idx, vein) in slice.mineral_veins.iter().enumerate() {
                if vein.is_interface && !near_contact {
                    continue;
                }
                let seed = self.world_seed
                    ^ (slice.stratum_id as u64).wrapping_mul(0x2545f491)
                    ^ (v_idx as u64).wrapping_mul(0x85ebca6b);
                // 3D sheet/vein noise producing thin dipping mineralized lodes
                let n = value_noise3(
                    x_m * 0.085,
                    y_m * 0.085,
                    (z_m as f64) * 0.14 + x_m * 0.04,
                    seed,
                );
                let threshold = 0.88 - 0.12 * vein.density.clamp(0.0, 1.0);
                if n > threshold {
                    return (vein.material, slice);
                }
            }
        }

        (slice.material, slice)
    }
}

/// Fast deterministic 2D gradient/value fBm in `[-1, 1]`.
pub fn fbm2(x: f64, y: f64, seed: u64, octaves: u32) -> f32 {
    let mut sum = 0.0f64;
    let mut amp = 0.5f64;
    let mut freq = 1.0f64;
    let mut norm = 0.0f64;
    for o in 0..octaves {
        sum += amp * value_noise2(x * freq, y * freq, seed.wrapping_add((o as u64) * 0x9e3779b9));
        norm += amp;
        amp *= 0.5;
        freq *= 2.03;
    }
    if norm > 0.0 {
        ((sum / norm) * 2.0 - 1.0) as f32
    } else {
        0.0
    }
}

/// Deterministic 2D smooth value noise in `[0, 1]`.
pub fn value_noise2(x: f64, y: f64, seed: u64) -> f64 {
    let ix = x.floor() as i64;
    let iy = y.floor() as i64;
    let fx = x - (ix as f64);
    let fy = y - (iy as f64);
    let ux = fx * fx * (3.0 - 2.0 * fx);
    let uy = fy * fy * (3.0 - 2.0 * fy);

    let a = hash2(ix, iy, seed);
    let b = hash2(ix + 1, iy, seed);
    let c = hash2(ix, iy + 1, seed);
    let d = hash2(ix + 1, iy + 1, seed);

    let x0 = a + (b - a) * ux;
    let x1 = c + (d - c) * ux;
    x0 + (x1 - x0) * uy
}

/// Deterministic 3D smooth value noise in `[0, 1]`.
pub fn value_noise3(x: f64, y: f64, z: f64, seed: u64) -> f32 {
    let ix = x.floor() as i64;
    let iy = y.floor() as i64;
    let iz = z.floor() as i64;
    let fx = x - (ix as f64);
    let fy = y - (iy as f64);
    let fz = z - (iz as f64);
    let ux = fx * fx * (3.0 - 2.0 * fx);
    let uy = fy * fy * (3.0 - 2.0 * fy);
    let uz = fz * fz * (3.0 - 2.0 * fz);

    let c000 = hash3(ix, iy, iz, seed);
    let c100 = hash3(ix + 1, iy, iz, seed);
    let c010 = hash3(ix, iy + 1, iz, seed);
    let c110 = hash3(ix + 1, iy + 1, iz, seed);
    let c001 = hash3(ix, iy, iz + 1, seed);
    let c101 = hash3(ix + 1, iy, iz + 1, seed);
    let c011 = hash3(ix, iy + 1, iz + 1, seed);
    let c111 = hash3(ix + 1, iy + 1, iz + 1, seed);

    let x00 = c000 + (c100 - c000) * ux;
    let x10 = c010 + (c110 - c010) * ux;
    let x01 = c001 + (c101 - c001) * ux;
    let x11 = c011 + (c111 - c011) * ux;

    let y0 = x00 + (x10 - x00) * uy;
    let y1 = x01 + (x11 - x01) * uy;
    (y0 + (y1 - y0) * uz) as f32
}

#[inline]
fn hash2(x: i64, y: i64, seed: u64) -> f64 {
    let mut h = seed
        ^ (x as u64).wrapping_mul(0x9e3779b97f4a7c15)
        ^ (y as u64).wrapping_mul(0xbf58476d1ce4e5b9);
    h = (h ^ (h >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94d049bb133111eb);
    h ^= h >> 31;
    ((h >> 11) as f64) * (1.0 / ((1u64 << 53) as f64))
}

#[inline]
fn hash3(x: i64, y: i64, z: i64, seed: u64) -> f64 {
    let mut h = seed
        ^ (x as u64).wrapping_mul(0x9e3779b97f4a7c15)
        ^ (y as u64).wrapping_mul(0xbf58476d1ce4e5b9)
        ^ (z as u64).wrapping_mul(0x94d049bb133111eb);
    h = (h ^ (h >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94d049bb133111eb);
    h ^= h >> 31;
    ((h >> 11) as f64) * (1.0 / ((1u64 << 53) as f64))
}
