#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RockKind {
    Igneous = 0,
    Sedimentary = 1,
    Metamorphic = 2,
}

impl RockKind {
    pub fn code(&self) -> &'static str {
        match self {
            RockKind::Igneous => "I",
            RockKind::Sedimentary => "S",
            RockKind::Metamorphic => "M",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rock {
    pub kind: RockKind,
    pub name: &'static str,
    pub felsity: f32,
    pub toughness: f32,
    pub clasticity: Option<f32>,
    pub calcity: Option<f32>,
    pub bogginess: Option<f32>,
}

impl Rock {
    /// Matches `rock/igneous.py:extrusive(silica)`.
    pub fn extrusive(silica: f32) -> Self {
        let name = if silica > 0.63 {
            "rhyolite"
        } else if silica > 0.52 {
            "dacite"
        } else {
            "basalt"
        };
        Self {
            kind: RockKind::Igneous,
            name,
            felsity: silica,
            toughness: silica,
            clasticity: None,
            calcity: None,
            bogginess: None,
        }
    }

    /// Matches `rock/igneous.py:intrusive(silica)`.
    pub fn intrusive(silica: f32) -> Self {
        let name = if silica > 0.63 {
            "granite"
        } else if silica > 0.52 {
            "andesite"
        } else {
            "gabbro"
        };
        Self {
            kind: RockKind::Igneous,
            name,
            felsity: silica,
            toughness: silica,
            clasticity: None,
            calcity: None,
            bogginess: None,
        }
    }

    /// Standard basaltic seafloor (`PlanetSimulation.seafloor()`).
    pub fn seafloor() -> Self {
        Self::extrusive(0.5)
    }

    /// Lower oceanic crust intrusive gabbro.
    pub fn gabbro_seafloor() -> Self {
        Self::intrusive(0.45)
    }

    /// Map a serialized rock name string back to its static `'static str` literal.
    pub fn intern_name(name: &str) -> &'static str {
        match name {
            "rhyolite" => "rhyolite",
            "dacite" => "dacite",
            "basalt" => "basalt",
            "granite" => "granite",
            "andesite" => "andesite",
            "gabbro" => "gabbro",
            "chalk" => "chalk",
            "limestone" => "limestone",
            "claystone" => "claystone",
            "siltstone" => "siltstone",
            "sandstone" => "sandstone",
            "conglomerate" => "conglomerate",
            "pelite" => "pelite",
            "psammite" => "psammite",
            "psephite" => "psephite",
            "blueschist" => "blueschist",
            "greenschist" => "greenschist",
            "amphibolite" => "amphibolite",
            "eclogite" => "eclogite",
            "hornfels" => "hornfels",
            "granulite" => "granulite",
            "marble" => "marble",
            _ => "basalt",
        }
    }

    /// Intrinsic density in `kg/m^3` based on composition (`Section 5.4.5` of design doc).
    pub fn density_kg_m3(&self) -> f32 {
        match self.kind {
            RockKind::Igneous => 3000.0 - 350.0 * self.felsity,
            RockKind::Sedimentary => 2400.0 + 250.0 * self.toughness,
            RockKind::Metamorphic => match self.name {
                "eclogite" => 3400.0,
                "blueschist" => 3100.0,
                "amphibolite" => 3000.0,
                "greenschist" => 2850.0,
                "pelite" | "psammite" | "psephite" => 2650.0,
                _ => 2750.0,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RockLayer {
    pub rock: Rock,
    pub thickness: f32,
}

/// Eroded material batch delivered from a source tile (`erodemethod.py:ErosionMaterial`).
#[derive(Clone, Debug)]
pub struct ErodedMaterialBatch {
    pub amount: f32,
    pub total: f32,
    /// Source layers ordered bottom-to-top (`substance` in `tile.py`).
    pub substance: Vec<RockLayer>,
}

/// Climate summary for a tile used during sedimentary deposition (`rock/sedimentary.py:41-50`).
#[derive(Copy, Clone, Debug)]
pub struct DepositClimate {
    pub temperature_c: f32,
    pub precipitation_norm: f32,
    pub koeppen_group: char,
}

/// Combine eroded material batches into a newly deposited sedimentary `RockLayer`
/// (`rock/sedimentary.py:deposit`).
pub fn deposit_sedimentary(
    materials: &[ErodedMaterialBatch],
    has_life: bool,
    is_sea: bool,
    climate: Option<DepositClimate>,
) -> RockLayer {
    struct Contrib {
        felsity: f32,
        clasticity: f32,
        calcity: f32,
        bogginess: f32,
        thickness: f32,
    }

    let mut contribs = Vec::with_capacity(materials.len());
    let mut any_clasticity = false;
    let mut any_calcity = false;
    let mut any_bogginess = false;

    for m in materials {
        let total = m.total.max(1e-7);
        let mut t = 0.0f32;
        let mut idx = m.substance.len();
        let mut sources: Vec<(&Rock, f32)> = Vec::new();

        while t < m.total - 1e-6 && idx > 0 {
            idx -= 1;
            let dt = m.total - t;
            let layer = &m.substance[idx];
            let take = layer.thickness.min(dt);
            sources.push((&layer.rock, take));
            t += take;
        }

        let mut felsity_sum = 0.0f32;
        let mut clast_sum = 0.0f32;
        let mut has_clast = false;
        let mut calc_sum = 0.0f32;
        let mut has_calc = false;
        let mut bog_sum = 0.0f32;
        let mut has_bog = false;

        for (r, thick) in &sources {
            felsity_sum += thick * r.felsity;
            if let Some(c) = r.clasticity {
                clast_sum += thick * c;
                has_clast = true;
            }
            if let Some(c) = r.calcity {
                calc_sum += thick * c;
                has_calc = true;
            }
            if let Some(b) = r.bogginess {
                bog_sum += thick * b;
                has_bog = true;
            }
        }

        any_clasticity |= has_clast;
        any_calcity |= has_calc;
        any_bogginess |= has_bog;

        contribs.push(Contrib {
            felsity: felsity_sum / total,
            clasticity: clast_sum / total,
            calcity: calc_sum / total,
            bogginess: bog_sum / total,
            thickness: m.amount,
        });
    }

    let total_thickness: f32 = contribs.iter().map(|c| c.thickness).sum();
    let denom = total_thickness.max(1e-7);

    let felsity = contribs.iter().map(|c| c.thickness * c.felsity).sum::<f32>() / denom;
    let prev_clasticity = if any_clasticity {
        Some(contribs.iter().map(|c| c.thickness * c.clasticity).sum::<f32>() / denom)
    } else {
        None
    };
    let mut calcity = if any_calcity {
        Some(contribs.iter().map(|c| c.thickness * c.calcity).sum::<f32>() / denom)
    } else {
        None
    };
    let mut bogginess = if any_bogginess {
        Some(contribs.iter().map(|c| c.thickness * c.bogginess).sum::<f32>() / denom)
    } else {
        None
    };

    let clasticity = match prev_clasticity {
        Some(c) if c > 0.0 => c * 2.0,
        _ => 1.0,
    };

    let mut chosen_name: Option<&'static str> = None;

    if has_life {
        if let Some(clim) = climate {
            if is_sea {
                let calc = ((clim.temperature_c - 18.0) / 25.0).clamp(0.0, 1.0);
                calcity = Some(calc);
                if calc > 0.99 {
                    chosen_name = Some("chalk");
                } else if calc > 0.75 {
                    chosen_name = Some("limestone");
                }
            } else if clim.koeppen_group == 'C' && clim.temperature_c < 18.0 {
                let bog = ((clim.precipitation_norm - 0.75) * 4.0).max(0.0);
                bogginess = Some(bog);
            }
        }
    }

    let name = chosen_name.unwrap_or_else(|| {
        let grain = 1e-3 / clasticity;
        if grain < 4e-6 {
            "claystone"
        } else if grain < 60e-6 {
            "siltstone"
        } else if grain < 2e-3 {
            "sandstone"
        } else {
            "conglomerate"
        }
    });

    RockLayer {
        rock: Rock {
            kind: RockKind::Sedimentary,
            name,
            felsity,
            toughness: 0.0,
            clasticity: Some(clasticity),
            calcity,
            bogginess,
        },
        thickness: total_thickness,
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum FaciesOp {
    Identity,
    Zeolite,
    Blueschist,
    Greenschist,
    Amphibolite,
    Eclogite,
    Hornfels,
    Granulite,
}

fn apply_facies(mut layer: RockLayer, op: FaciesOp) -> RockLayer {
    match op {
        FaciesOp::Identity => return layer,
        FaciesOp::Zeolite => {
            if let Some(clast) = layer.rock.clasticity.take() {
                layer.rock.kind = RockKind::Metamorphic;
                let grain = 1e-3 / clast;
                layer.rock.name = if grain < 60e-6 {
                    "pelite"
                } else if grain < 2e-3 {
                    "psammite"
                } else {
                    "psephite"
                };
                layer.rock.toughness = layer.rock.toughness.max(0.25);
            }
        }
        FaciesOp::Blueschist => {
            layer.rock.kind = RockKind::Metamorphic;
            layer.rock.name = "blueschist";
            layer.rock.toughness = layer.rock.toughness.max(0.75);
        }
        FaciesOp::Greenschist => {
            layer.rock.kind = RockKind::Metamorphic;
            layer.rock.name = "greenschist";
            layer.rock.toughness = layer.rock.toughness.max(0.75);
        }
        FaciesOp::Amphibolite => {
            layer.rock.kind = RockKind::Metamorphic;
            layer.rock.name = "amphibolite";
            layer.rock.toughness = layer.rock.toughness.max(0.80);
        }
        FaciesOp::Eclogite => {
            layer.rock.kind = RockKind::Metamorphic;
            layer.rock.name = "eclogite";
            layer.rock.toughness = layer.rock.toughness.max(0.85);
        }
        FaciesOp::Hornfels => {
            layer.rock.kind = RockKind::Metamorphic;
            layer.rock.name = "hornfels";
            layer.rock.toughness = 1.0;
        }
        FaciesOp::Granulite => {
            layer.rock.kind = RockKind::Metamorphic;
            layer.rock.name = "granulite";
            layer.rock.toughness = 1.0;
        }
    }

    // `marblize` (`rock/metamorphic.py:63-66`)
    if let Some(calc) = layer.rock.calcity {
        if calc > 0.75 {
            layer.rock.name = "marble";
        }
    }
    layer
}

fn transform_by_facies(layers: &[RockLayer], facies: &[(f32, FaciesOp)]) -> Vec<RockLayer> {
    let mut f = 0usize;
    let mut new_layers: Vec<RockLayer> = Vec::new();
    let mut t = 0.0f32;
    let mut stack: Vec<RockLayer> = layers.to_vec();

    while let Some(mut l) = stack.pop() {
        let dt = t + l.thickness - facies[f].0;
        if dt > 1e-6 {
            stack.push(RockLayer {
                rock: l.rock.clone(),
                thickness: dt,
            });
            l.thickness -= dt;
        }

        if facies[f].1 != FaciesOp::Identity {
            l = apply_facies(l, facies[f].1);
        }

        t += l.thickness;
        new_layers.push(l);
        if dt > 1e-6 && f + 1 < facies.len() {
            f += 1;
        }
    }

    new_layers.reverse();
    new_layers
}

/// Regional depth-facies metamorphism (`rock/metamorphic.py:regional`).
pub fn metamorphic_regional(layers: &[RockLayer], subduction: bool) -> Vec<RockLayer> {
    const NORMAL_FACIES: [(f32, FaciesOp); 5] = [
        (5.0, FaciesOp::Identity),
        (15.0, FaciesOp::Zeolite),
        (30.0, FaciesOp::Greenschist),
        (70.0, FaciesOp::Amphibolite),
        (f32::INFINITY, FaciesOp::Eclogite),
    ];
    const SUBDUCTION_FACIES: [(f32, FaciesOp); 4] = [
        (10.0, FaciesOp::Identity),
        (20.0, FaciesOp::Zeolite),
        (60.0, FaciesOp::Blueschist),
        (f32::INFINITY, FaciesOp::Eclogite),
    ];

    if subduction {
        transform_by_facies(layers, &SUBDUCTION_FACIES)
    } else {
        transform_by_facies(layers, &NORMAL_FACIES)
    }
}

/// Contact thermal metamorphism around an igneous intrusion (`rock/metamorphic.py:contact`).
pub fn metamorphic_contact(
    layers: &[RockLayer],
    intrusion: Option<(f32, f32)>,
) -> Vec<RockLayer> {
    let Some((top, bottom)) = intrusion else {
        return layers.to_vec();
    };

    let mid_depth = 0.5 * (top + bottom);
    let temp = 900.0 - 40.0 * mid_depth.max(0.0).sqrt();
    let thickness = (bottom - top).max(1e-6);
    let m = temp / thickness;

    let hightemp: [(f32, FaciesOp); 3] = [
        (250.0, FaciesOp::Identity),
        (800.0, FaciesOp::Hornfels),
        (f32::INFINITY, FaciesOp::Granulite),
    ];

    let d0 = (top - thickness).max(0.0);
    let mut facies: Vec<(f32, FaciesOp)> = vec![(d0, FaciesOp::Identity)];

    // Approaching intrusion from above
    for &(t_thresh, op) in &hightemp {
        let d = if t_thresh.is_finite() {
            t_thresh / m + (top - thickness)
        } else {
            f32::INFINITY
        };
        if d > top {
            facies.push((top, op));
            break;
        }
        if d > top - thickness {
            facies.push((d, op));
        }
    }

    // Intrusion itself stays unchanged
    facies.push((bottom, FaciesOp::Identity));

    // Away from intrusion below
    for i in (0..(hightemp.len() - 1)).rev() {
        let t_thresh = hightemp[i].0;
        let op = hightemp[i + 1].1;
        let d = if t_thresh.is_finite() {
            -t_thresh / m + bottom + thickness
        } else {
            f32::INFINITY
        };
        if d < bottom {
            continue;
        }
        if d < bottom + thickness {
            facies.push((d, op));
        }
    }

    facies.push((f32::INFINITY, FaciesOp::Identity));

    let mut i = 0;
    while i + 1 < facies.len() {
        if (facies[i].0 - facies[i + 1].0).abs() < 1e-6 || facies[i].1 == facies[i + 1].1 {
            facies.remove(i);
        } else {
            i += 1;
        }
    }

    transform_by_facies(layers, &facies)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_layer(thickness: f32) -> RockLayer {
        RockLayer {
            rock: Rock::extrusive(0.5),
            thickness,
        }
    }

    fn clastic_layer(clasticity: f32, thickness: f32) -> RockLayer {
        let mut r = Rock::extrusive(0.5);
        r.clasticity = Some(clasticity);
        RockLayer { rock: r, thickness }
    }

    #[test]
    fn test_metamorphic_regional() {
        let layers = metamorphic_regional(&[dummy_layer(50.0)], true);
        let sum_t: f32 = layers.iter().map(|l| l.thickness).sum();
        assert!((sum_t - 50.0).abs() < 1e-5);
        // Top 10 km is unchanged Igneous
        assert_eq!(layers.last().unwrap().rock.kind, RockKind::Igneous);
        // 10..20 km non-clastic zeolite remains Igneous
        assert_eq!(layers[layers.len() - 2].rock.kind, RockKind::Igneous);
        // 20..50 km is blueschist
        assert_eq!(layers[layers.len() - 3].rock.name, "blueschist");

        // Clastic zeolite facies:
        let grav = metamorphic_regional(&[clastic_layer(0.5, 50.0)], true);
        assert_eq!(grav[grav.len() - 2].rock.name, "psephite");

        let sandy = metamorphic_regional(&[clastic_layer(4.0, 50.0)], true);
        assert_eq!(sandy[sandy.len() - 2].rock.name, "psammite");

        let clayey = metamorphic_regional(&[clastic_layer(32.0, 50.0)], true);
        assert_eq!(clayey[clayey.len() - 2].rock.name, "pelite");

        // Eclogite below 60 km (subduction) and 70 km (normal):
        let sub_deep = metamorphic_regional(&[dummy_layer(75.0)], true);
        assert_eq!(sub_deep[sub_deep.len() - 4].rock.name, "eclogite");

        let norm = metamorphic_regional(&[dummy_layer(75.0)], false);
        assert_eq!(norm[norm.len() - 3].rock.name, "greenschist");
        assert_eq!(norm[norm.len() - 4].rock.name, "amphibolite");
        assert_eq!(norm[norm.len() - 5].rock.name, "eclogite");
    }

    #[test]
    fn test_metamorphic_contact() {
        let layers = metamorphic_contact(&[dummy_layer(50.0)], Some((5.0, 6.0)));
        let n = layers.len();
        assert_eq!(layers[n - 1].rock.kind, RockKind::Igneous);
        assert_eq!(layers[n - 2].rock.name, "hornfels");
        assert_eq!(layers[n - 3].rock.name, "granulite");
        assert_eq!(layers[n - 4].rock.kind, RockKind::Igneous);
        assert_eq!(layers[n - 5].rock.name, "granulite");
        assert_eq!(layers[n - 6].rock.name, "hornfels");

        // Deep intrusion at (25, 26) is cooler (< 800 C) so only hornfels forms:
        let deep = metamorphic_contact(&[dummy_layer(50.0)], Some((25.0, 26.0)));
        let nd = deep.len();
        assert_eq!(deep[nd - 2].rock.name, "hornfels");
        assert_eq!(deep[nd - 3].rock.kind, RockKind::Igneous);
        assert_eq!(deep[nd - 4].rock.name, "hornfels");
    }

    #[test]
    fn test_sedimentary_deposit() {
        let m1 = ErodedMaterialBatch {
            amount: 1.0,
            total: 1.0,
            substance: vec![RockLayer {
                rock: Rock::extrusive(0.6),
                thickness: 1.0,
            }],
        };
        let m2 = ErodedMaterialBatch {
            amount: 2.0,
            total: 2.0,
            substance: vec![RockLayer {
                rock: Rock::extrusive(0.9),
                thickness: 2.0,
            }],
        };
        let dep = deposit_sedimentary(&[m1, m2], false, false, None);
        assert_eq!(dep.rock.kind, RockKind::Sedimentary);
        assert!((dep.thickness - 3.0).abs() < 1e-5);
        assert!((dep.rock.felsity - 0.8).abs() < 1e-5);
        assert_eq!(dep.rock.clasticity, Some(1.0));
        assert_eq!(dep.rock.name, "sandstone");

        // Warm sea with life deposits limestone / chalk:
        let warm_sea_dep = deposit_sedimentary(
            &[ErodedMaterialBatch {
                amount: 1.0,
                total: 1.0,
                substance: vec![dummy_layer(1.0)],
            }],
            true,
            true,
            Some(DepositClimate {
                temperature_c: 40.0,
                precipitation_norm: 0.5,
                koeppen_group: 'A',
            }),
        );
        assert_eq!(warm_sea_dep.rock.name, "limestone");
    }
}
