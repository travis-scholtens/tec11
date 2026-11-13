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
    pub quartzity: Option<f32>,
    pub sulfidity: Option<f32>,
    pub salinity: Option<f32>,
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
            quartzity: None,
            sulfidity: None,
            salinity: None,
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
            quartzity: None,
            sulfidity: None,
            salinity: None,
        }
    }

    /// Standard basaltic seafloor (`PlanetSimulation.seafloor()`), slightly enriched in
    /// seafloor hydrothermal VMS sulfides (`sulfidity = 0.08`).
    pub fn seafloor() -> Self {
        let mut r = Self::extrusive(0.5);
        r.sulfidity = Some(0.08);
        r
    }

    /// Lower oceanic crust intrusive gabbro, slightly enriched in magmatic cumulate sulfides (`sulfidity = 0.12`).
    pub fn gabbro_seafloor() -> Self {
        let mut r = Self::intrusive(0.45);
        r.sulfidity = Some(0.12);
        r
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

/// Physical climate inputs for a tile used during sedimentary deposition (`rock/sedimentary.py:41-50`).
#[derive(Copy, Clone, Debug)]
pub struct DepositClimate {
    pub temperature_c: f32,
    pub precipitation_norm: f32,
    pub life: f32,
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
        quartzity: f32,
        sulfidity: f32,
        salinity: f32,
        thickness: f32,
    }

    let mut contribs = Vec::with_capacity(materials.len());
    let mut any_clasticity = false;
    let mut any_calcity = false;
    let mut any_bogginess = false;
    let mut any_quartzity = false;
    let mut any_sulfidity = false;
    let mut any_salinity = false;

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
        let mut quartz_sum = 0.0f32;
        let mut has_quartz = false;
        let mut sulf_sum = 0.0f32;
        let mut has_sulf = false;
        let mut sal_sum = 0.0f32;
        let mut has_sal = false;

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
            if let Some(q) = r.quartzity {
                quartz_sum += thick * q;
                has_quartz = true;
            }
            if let Some(s) = r.sulfidity {
                sulf_sum += thick * s;
                has_sulf = true;
            }
            if let Some(s) = r.salinity {
                sal_sum += thick * s;
                has_sal = true;
            }
        }

        any_clasticity |= has_clast;
        any_calcity |= has_calc;
        any_bogginess |= has_bog;
        any_quartzity |= has_quartz;
        any_sulfidity |= has_sulf;
        any_salinity |= has_sal;

        contribs.push(Contrib {
            felsity: felsity_sum / total,
            clasticity: clast_sum / total,
            calcity: calc_sum / total,
            bogginess: bog_sum / total,
            quartzity: quartz_sum / total,
            sulfidity: sulf_sum / total,
            salinity: sal_sum / total,
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

    // Heavy detrital vein minerals (native gold, cassiterite/tin, quartz-vein pebbles) concentrate
    // hydraulically in coarse fluvial gravels/sands (`clasticity <= 2.5`) as placer deposits,
    // whereas fine silts/clays winnow them out (`placer_factor < 1.0`).
    let placer_factor = (1.35 / clasticity.max(0.5).sqrt()).clamp(0.25, 1.65);
    let mut quartzity = if any_quartzity {
        let q = (contribs.iter().map(|c| c.thickness * c.quartzity).sum::<f32>() / denom)
            * placer_factor;
        (q > 1e-4).then_some(q.clamp(0.0, 1.0))
    } else {
        None
    };

    // Sulfide minerals weather/oxidize during subaerial transport (`0.65` retention),
    // but in anoxic pre-biotic oceans (`is_sea && !has_life`) dissolved ferrous iron, sulfur,
    // and silica precipitate together as Banded Iron Formations (alternating iron/sulfide
    // `sulfidity` + chert `quartzity` bands).
    let mut sulfidity = if any_sulfidity {
        let s = (contribs.iter().map(|c| c.thickness * c.sulfidity).sum::<f32>() / denom) * 0.65;
        (s > 1e-4).then_some(s.clamp(0.0, 1.0))
    } else {
        None
    };
    if is_sea && !has_life {
        let bif_iron = (0.22 * (1.0 - 0.5 * felsity)).clamp(0.10, 0.35);
        let bif_chert = 0.24;
        sulfidity = Some((sulfidity.unwrap_or(0.0) + bif_iron).clamp(0.0, 1.0));
        quartzity = Some((quartzity.unwrap_or(0.0) + bif_chert).clamp(0.0, 1.0));
    }

    // Halite/gypsum evaporite salts dissolve in wet runoff (`0.25` inherited retention) and
    // precipitate in hot, arid basins where potential evaporation exceeds precipitation
    // (`precipitation_norm < evap_limit`, strictly keyed off physical temperature & precipitation).
    let mut salinity = if any_salinity {
        let s = (contribs.iter().map(|c| c.thickness * c.salinity).sum::<f32>() / denom) * 0.25;
        (s > 1e-4).then_some(s.clamp(0.0, 1.0))
    } else {
        None
    };
    if let Some(clim) = climate {
        if clim.temperature_c > 8.0 {
            let evap_limit = (0.20 + 0.008 * (clim.temperature_c - 8.0)).clamp(0.20, 0.48);
            if clim.precipitation_norm < evap_limit {
                let dryness = ((evap_limit - clim.precipitation_norm) / evap_limit).clamp(0.0, 1.0);
                let heat = ((clim.temperature_c - 8.0) / 32.0).clamp(0.15, 1.0);
                let basin_boost = if is_sea { 0.85 } else { 1.0 };
                let evap_sal = (dryness * heat * basin_boost * 1.15).clamp(0.0, 1.0);
                if evap_sal > 1e-4 {
                    salinity = Some(salinity.unwrap_or(0.0).max(evap_sal));
                }
            }
        }
    }

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
            } else if clim.life > 0.0 && clim.temperature_c > -5.0 {
                // Peat/bog accumulation from physical water-table saturation (precipitation vs.
                // temperature-dependent evapotranspiration) and local biomass (`clim.life`):
                // - Cool/temperate & boreal wetlands saturate above `precip_norm > ~0.55`
                // - Warm tropical swamp forests require higher rainfall (`precip_norm > ~0.68`) to stay waterlogged
                let evap_threshold =
                    (0.55 + 0.005 * (clim.temperature_c - 5.0).max(0.0)).clamp(0.52, 0.72);
                let bog =
                    ((clim.precipitation_norm - evap_threshold) * 3.2 * clim.life).clamp(0.0, 1.0);
                if bog > 1e-4 {
                    bogginess = Some(bogginess.unwrap_or(0.0).max(bog));
                }
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
            quartzity,
            sulfidity,
            salinity,
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
            if layer.rock.toughness <= 0.75 {
                layer.rock.kind = RockKind::Metamorphic;
                layer.rock.name = "blueschist";
                layer.rock.toughness = layer.rock.toughness.max(0.75);
            }
        }
        FaciesOp::Greenschist => {
            if layer.rock.toughness <= 0.75 && layer.rock.name != "blueschist" {
                layer.rock.kind = RockKind::Metamorphic;
                layer.rock.name = "greenschist";
                layer.rock.toughness = layer.rock.toughness.max(0.75);
            }
        }
        FaciesOp::Amphibolite => {
            if layer.rock.toughness <= 0.80 {
                layer.rock.kind = RockKind::Metamorphic;
                layer.rock.name = "amphibolite";
                layer.rock.toughness = layer.rock.toughness.max(0.80);
            }
        }
        FaciesOp::Eclogite => {
            if layer.rock.toughness <= 0.85 {
                layer.rock.kind = RockKind::Metamorphic;
                layer.rock.name = "eclogite";
                layer.rock.toughness = layer.rock.toughness.max(0.85);
            }
        }
        FaciesOp::Hornfels => {
            if layer.rock.name != "granulite" {
                layer.rock.kind = RockKind::Metamorphic;
                layer.rock.name = "hornfels";
                layer.rock.toughness = 1.0;
            }
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
    let mut new_layers: Vec<RockLayer> = Vec::with_capacity(layers.len() + 2);
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

fn transform_by_facies_in_place(layers: &mut Vec<RockLayer>, facies: &[(f32, FaciesOp)]) {
    if layers.is_empty() {
        return;
    }
    // Fast check: if total depth is <= the first Identity boundary, nothing splits or changes!
    if facies.first().map(|&(d, op)| op == FaciesOp::Identity && layers.iter().map(|l| l.thickness).sum::<f32>() <= d + 1e-6).unwrap_or(false) {
        return;
    }
    let mut f = 0usize;
    let mut new_layers: Vec<RockLayer> = Vec::with_capacity(layers.len() + 2);
    let mut t = 0.0f32;

    while let Some(mut l) = layers.pop() {
        let dt = t + l.thickness - facies[f].0;
        if dt > 1e-6 {
            layers.push(RockLayer {
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
    *layers = new_layers;
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

/// In-place regional depth-facies metamorphism (`rock/metamorphic.py:regional`).
pub fn metamorphic_regional_in_place(layers: &mut Vec<RockLayer>, subduction: bool) {
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
        transform_by_facies_in_place(layers, &SUBDUCTION_FACIES);
    } else {
        transform_by_facies_in_place(layers, &NORMAL_FACIES);
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

    let mut transformed = transform_by_facies(layers, &facies);

    // Magmatic-hydrothermal fluid injection across the intrusion (`top..bottom`) and its
    // surrounding contact aureole (`d0..bottom + thickness`):
    // - Felsic plutons (`granite`, high `felsity`) exsolve silica-rich hydrothermal fluids (`quartzity`).
    // - Intermediate/mafic plutons (`andesite`/`gabbro`) and carbonate skarns (`calcity > 0.35`)
    //   precipitate metallic sulfides (`sulfidity`).
    let mut pluton_felsity = 0.65f32;
    let mut d = 0.0f32;
    for l in transformed.iter().rev() {
        let l_top = d;
        let l_bot = d + l.thickness;
        if l.rock.kind == RockKind::Igneous
            && l_bot > top - 0.1
            && l_top < bottom + 0.1
            && matches!(l.rock.name, "granite" | "andesite" | "gabbro")
        {
            pluton_felsity = l.rock.felsity;
            break;
        }
        d = l_bot;
    }

    let aureole_top = d0;
    let aureole_bot = bottom + thickness;
    let mut d = 0.0f32;
    for l in transformed.iter_mut().rev() {
        let l_top = d;
        let l_bot = d + l.thickness;
        d = l_bot;
        let overlap = (l_bot.min(aureole_bot) - l_top.max(aureole_top)).max(0.0);
        if overlap <= 1e-5 {
            continue;
        }
        let is_intrusion = l_bot > top + 1e-4 && l_top < bottom - 1e-4;
        let skarn_boost = if l.rock.calcity.unwrap_or(0.0) > 0.35 {
            0.35 * l.rock.calcity.unwrap_or(0.0)
        } else {
            0.0
        };
        let q_gain = if is_intrusion {
            (0.45 * (pluton_felsity - 0.45).max(0.0)).clamp(0.05, 0.35)
        } else {
            (0.65 * (pluton_felsity - 0.35).max(0.0) + 0.15 * skarn_boost).clamp(0.10, 0.55)
        };
        let s_gain = if is_intrusion {
            (0.42 * (1.05 - pluton_felsity).max(0.2)).clamp(0.10, 0.40)
        } else {
            (0.38 * (1.0 - (pluton_felsity - 0.58).abs()) + skarn_boost).clamp(0.12, 0.70)
        };
        l.rock.quartzity = Some((l.rock.quartzity.unwrap_or(0.0) + q_gain).clamp(0.0, 1.0));
        l.rock.sulfidity = Some((l.rock.sulfidity.unwrap_or(0.0) + s_gain).clamp(0.0, 1.0));
    }

    transformed
}

/// Apply orogenic metamorphic dewatering (`quartzity` & `sulfidity` vein enrichment)
/// to `Zeolite`, `Greenschist`, and `Blueschist` facies (`5..35 km` depth) during active
/// subduction or continental-collision compression (`orogenic_intensity > 0.0`).
pub fn enrich_orogenic_veins_in_place(layers: &mut [RockLayer], orogenic_intensity: f32) {
    if orogenic_intensity <= 1e-4 || layers.is_empty() {
        return;
    }
    let intensity = orogenic_intensity.clamp(0.0, 1.0);
    let mut depth = 0.0f32;
    for l in layers.iter_mut().rev() {
        let l_top = depth;
        let l_bot = depth + l.thickness;
        depth = l_bot;
        // Orogenic quartz-gold and polymetallic veins precipitate in brittle-ductile
        // shear zones between 5 km and 35 km depth (`pelite`, `psammite`, `psephite`,
        // `greenschist`, `blueschist`).
        let zone_overlap = (l_bot.min(35.0) - l_top.max(5.0)).max(0.0);
        if zone_overlap <= 1e-4 || l.rock.kind != RockKind::Metamorphic {
            continue;
        }
        let host_factor = match l.rock.name {
            "greenschist" | "blueschist" => 1.0,
            "pelite" | "psammite" | "psephite" => 0.85,
            "amphibolite" => 0.45,
            _ => 0.25,
        };
        let dq = 0.045 * intensity * host_factor;
        let ds = 0.022 * intensity * host_factor * (1.1 - 0.5 * l.rock.felsity).max(0.3);
        let new_q = (l.rock.quartzity.unwrap_or(0.0) + dq).clamp(0.0, 1.0);
        let new_s = (l.rock.sulfidity.unwrap_or(0.0) + ds).clamp(0.0, 1.0);
        if new_q > 1e-4 {
            l.rock.quartzity = Some(new_q);
        }
        if new_s > 1e-4 {
            l.rock.sulfidity = Some(new_s);
        }
    }
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

        // Prograde irreversibility: blueschist formed during subduction must not be overwritten
        // by greenschist when subduction ceases at the same depth (15..30 km), nor should
        // higher-grade eclogite/amphibolite/hornfels/granulite retrograde when exhumed shallower.
        let sub_layers = metamorphic_regional(&[dummy_layer(30.0)], true);
        assert_eq!(sub_layers[0].rock.name, "blueschist");
        let after_subduction_stops = metamorphic_regional(&sub_layers, false);
        assert_eq!(
            after_subduction_stops[0].rock.name,
            "blueschist",
            "Blueschist at 20..30 km must not be overwritten by greenschist after subduction stops"
        );
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
                life: 1.0,
            }),
        );
        assert_eq!(warm_sea_dep.rock.name, "limestone");

        // Wet terrestrial basins with life accumulate bogginess across both cool temperate/boreal
        // peatlands and warm tropical swamp forests, keyed purely off physical temp/precip/life:
        let cool_bog_dep = deposit_sedimentary(
            &[ErodedMaterialBatch {
                amount: 1.0,
                total: 1.0,
                substance: vec![dummy_layer(1.0)],
            }],
            true,
            false,
            Some(DepositClimate {
                temperature_c: 10.0,
                precipitation_norm: 0.75,
                life: 1.0,
            }),
        );
        assert!(cool_bog_dep.rock.bogginess.unwrap_or(0.0) > 0.4);

        let tropical_swamp_dep = deposit_sedimentary(
            &[ErodedMaterialBatch {
                amount: 1.0,
                total: 1.0,
                substance: vec![dummy_layer(1.0)],
            }],
            true,
            false,
            Some(DepositClimate {
                temperature_c: 26.0,
                precipitation_norm: 0.85,
                life: 1.0,
            }),
        );
        assert!(tropical_swamp_dep.rock.bogginess.unwrap_or(0.0) > 0.5);

        // Arid terrestrial basin has no bogginess, and warm arid basins accumulate evaporite salinity:
        let arid_dep = deposit_sedimentary(
            &[ErodedMaterialBatch {
                amount: 1.0,
                total: 1.0,
                substance: vec![dummy_layer(1.0)],
            }],
            true,
            false,
            Some(DepositClimate {
                temperature_c: 28.0,
                precipitation_norm: 0.15,
                life: 1.0,
            }),
        );
        assert_eq!(arid_dep.rock.bogginess, None);
        assert!(
            arid_dep.rock.salinity.unwrap_or(0.0) > 0.25,
            "Expected hot arid basin to precipitate evaporite salinity, got {:?}",
            arid_dep.rock.salinity
        );

        // Eroding a quartz-veined metamorphic rock into coarse conglomerate concentrates placer quartzity:
        let mut veined = dummy_layer(1.0);
        veined.rock.kind = RockKind::Metamorphic;
        veined.rock.name = "greenschist";
        veined.rock.quartzity = Some(0.50);
        let placer_dep = deposit_sedimentary(
            &[ErodedMaterialBatch {
                amount: 1.0,
                total: 1.0,
                substance: vec![veined],
            }],
            true,
            false,
            None,
        );
        assert!(
            placer_dep.rock.quartzity.unwrap_or(0.0) > 0.60,
            "Expected coarse gravel conglomerate to hydraulically concentrate placer quartzity, got {:?}",
            placer_dep.rock.quartzity
        );
    }
}
