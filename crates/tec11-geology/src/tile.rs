use std::collections::BTreeMap;

use crate::rock::{Rock, RockKind, RockLayer};

/// Minimum continuous grade (`0.0..=1.0`) for sparse/trace mineralogical occurrences.
pub const TRACE_GRADE: f32 = 0.06;
/// Minimum continuous grade (`0.0..=1.0`) for locally workable deposits & lexicon naming.
pub const WORKABLE_GRADE: f32 = 0.20;
/// Minimum continuous grade (`0.0..=1.0`) for rich / bonanza deposits.
pub const RICH_GRADE: f32 = 0.52;

/// Distinct mineral and gemstone commodities derived from rock composition, facies, and interfaces.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MineralKind {
    Iron,
    Salt,
    Gypsum,
    Gold,
    Copper,
    Tin,
    Coal,
    Graphite,
    Quartz,
    Flint,
    Silver,
    LeadZinc,
    Tungsten,
    Nickel,
    Platinum,
    Diamond,
    Ruby,
    Sapphire,
    Emerald,
    Jade,
    Lapis,
    Turquoise,
    Opal,
}

impl MineralKind {
    pub fn code(&self) -> &'static str {
        match self {
            MineralKind::Iron => "iron",
            MineralKind::Salt => "salt",
            MineralKind::Gypsum => "gypsum",
            MineralKind::Gold => "gold",
            MineralKind::Copper => "copper",
            MineralKind::Tin => "tin",
            MineralKind::Coal => "coal",
            MineralKind::Graphite => "graphite",
            MineralKind::Quartz => "quartz",
            MineralKind::Flint => "flint",
            MineralKind::Silver => "silver",
            MineralKind::LeadZinc => "lead_zinc",
            MineralKind::Tungsten => "tungsten",
            MineralKind::Nickel => "nickel",
            MineralKind::Platinum => "platinum",
            MineralKind::Diamond => "diamond",
            MineralKind::Ruby => "ruby",
            MineralKind::Sapphire => "sapphire",
            MineralKind::Emerald => "emerald",
            MineralKind::Jade => "jade",
            MineralKind::Lapis => "lapis",
            MineralKind::Turquoise => "turquoise",
            MineralKind::Opal => "opal",
        }
    }

    pub fn is_bulk_bedded(&self) -> bool {
        matches!(
            self,
            MineralKind::Iron
                | MineralKind::Salt
                | MineralKind::Gypsum
                | MineralKind::Coal
                | MineralKind::Graphite
                | MineralKind::Quartz
                | MineralKind::Flint
        )
    }
}

/// Qualitative richness tier derived from continuous `grade` (`0.0..=1.0`).
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum GradeTier {
    Trace,
    Workable,
    Rich,
}

impl GradeTier {
    pub fn from_grade(grade: f32) -> Option<Self> {
        if grade >= RICH_GRADE {
            Some(GradeTier::Rich)
        } else if grade >= WORKABLE_GRADE {
            Some(GradeTier::Workable)
        } else if grade >= TRACE_GRADE {
            Some(GradeTier::Trace)
        } else {
            None
        }
    }

    pub fn prefix(&self, bedded: bool) -> &'static str {
        match self {
            GradeTier::Rich => "Rich ",
            GradeTier::Workable => "",
            GradeTier::Trace => {
                if bedded {
                    "Thin "
                } else {
                    "Trace "
                }
            }
        }
    }
}

/// Quantitative mineral deposit or vein system hosted inside a rock layer or along a
/// contact interface between two adjacent layers.
#[derive(Clone, Debug)]
pub struct DepositOccurrence {
    pub label: String,
    /// Primary concentration / quality grade in `[0.0, 1.0]`.
    pub grade: f32,
    /// Effective mineralized zone / seam / pay-streak thickness in km.
    pub thickness_km: f32,
    /// Total resource abundance index (`grade * thickness_km`).
    pub abundance: f32,
    /// `true` when concentrated along the contact interface between two adjacent layers.
    pub is_interface: bool,
    /// Individual mineral grades `(MineralKind, grade)` present in this occurrence.
    pub minerals: Vec<(MineralKind, f32)>,
}

/// Continuous `[0.0, 1.0]` gemstone grades hosted in a stratigraphic layer, derived from
/// pressure-temperature facies and compositional intersections (`0.0` = absent).
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct LayerGems {
    pub diamond: f32,
    pub ruby: f32,
    pub sapphire: f32,
    pub emerald: f32,
    pub jade: f32,
    pub lapis: f32,
    pub turquoise: f32,
    pub opal: f32,
}

impl LayerGems {
    #[inline]
    pub fn is_present(grade: f32) -> bool {
        grade >= TRACE_GRADE
    }

    #[inline]
    pub fn is_workable(grade: f32) -> bool {
        grade >= WORKABLE_GRADE
    }
}

#[derive(Clone, Debug)]
pub struct TileSourceColumn {
    pub bottom: f32,
    pub mountainosity: f32,
    pub impact_scar: f32,
    pub layers: Vec<RockLayer>,
}

#[derive(Clone, Debug)]
pub struct TileGeology {
    pub bottom: f32,
    pub layers: Vec<RockLayer>,
    pub mountainosity: f32,
    pub subduction: f32,
    pub intrusion: Option<(f32, f32)>,
    pub impact_scar: f32,
    /// Physical thickness of weathered regolith + deposited loess/till in meters (`[0.0, 25.0]`).
    pub soil_depth_m: f32,
    /// Nutrient richness of the surficial soil (`[0.45, 1.50]`), initialized from bedrock parent material.
    pub soil_fertility: f32,
}

impl TileGeology {
    pub const MAX_HEIGHT: f32 = 10.0;
    pub const MIN_THICKNESS: f32 = 4.0;
    pub const MAX_THICKNESS: f32 = 75.0;
    pub const MAX_LAYERS: usize = 64;
    pub const MICRO_LAYER_KM: f32 = 0.002;
    pub const IMPACT_BRECCIA_CLASTICITY: f32 = 0.4213;

    /// Continental column initialized with a base crust of 9 km + surface relief `h`.
    pub fn empty_land(rock: Rock, h: f32) -> Self {
        let soil_fertility = rock.soil_fertility().clamp(0.55, 1.40);
        Self {
            bottom: -9.0,
            layers: vec![RockLayer {
                rock,
                thickness: 9.0 + h,
            }],
            mountainosity: 0.0,
            subduction: 0.0,
            intrusion: None,
            impact_scar: 0.0,
            soil_depth_m: if h > 0.0 { 2.0 } else { 0.0 },
            soil_fertility,
        }
    }

    /// Oceanic column initialized with 5 km basaltic crust at abyssal depth (`elevation = -4.0 km`).
    pub fn empty_ocean(rock: Rock) -> Self {
        Self::empty_ocean_with_depth(rock, -4.0)
    }

    /// Oceanic column initialized with 5 km basaltic crust at the specified submarine elevation (`<= 0.0 km`).
    pub fn empty_ocean_with_depth(rock: Rock, elevation_km: f32) -> Self {
        let clamped_elev = elevation_km.min(-0.05);
        let soil_fertility = rock.soil_fertility().clamp(0.55, 1.40);
        Self {
            bottom: clamped_elev - 5.0,
            layers: vec![RockLayer {
                rock,
                thickness: 5.0,
            }],
            mountainosity: 0.0,
            subduction: 0.0,
            intrusion: None,
            impact_scar: 0.0,
            soil_depth_m: 0.0,
            soil_fertility,
        }
    }

    /// Newly accreted mid-ocean ridge seafloor (`Section 5.4.2` of design doc):
    /// 3.0 km lower gabbro + 2.0 km upper pillow basalt = 5.0 km total thickness,
    /// thermally buoyant at `bottom = -7.5 km` (`elevation = -2.5 km` ridge crest).
    pub fn birth_seafloor() -> Self {
        let seafloor = Rock::seafloor();
        let soil_fertility = seafloor.soil_fertility().clamp(0.55, 1.40);
        Self {
            bottom: -7.5,
            layers: vec![
                RockLayer {
                    rock: Rock::gabbro_seafloor(),
                    thickness: 3.0,
                },
                RockLayer {
                    rock: seafloor,
                    thickness: 2.0,
                },
            ],
            mountainosity: 0.0,
            subduction: 0.0,
            intrusion: None,
            impact_scar: 0.0,
            soil_depth_m: 0.0,
            soil_fertility,
        }
    }

    #[inline]
    pub fn thickness(&self) -> f32 {
        self.layers.iter().map(|l| l.thickness).sum()
    }

    /// Signed elevation in km (`> 0` for land, `<= 0` for ocean floor).
    #[inline]
    pub fn elevation(&self) -> f32 {
        self.bottom + self.thickness()
    }

    /// Normalized mountainosity in `[0.0, 1.0]` (`tile.py:291-292`).
    #[inline]
    pub fn mountainosity_norm(&self) -> f32 {
        (self.mountainosity.min(20.0) / 20.0).max(0.0)
    }

    #[inline]
    pub fn top_felsity(&self) -> f32 {
        self.layers.last().map(|l| l.rock.felsity).unwrap_or(0.5)
    }

    #[inline]
    pub fn top_rock_kind(&self) -> RockKind {
        self.layers
            .last()
            .map(|l| l.rock.kind)
            .unwrap_or(RockKind::Igneous)
    }

    /// Near-surface regolith & soil fertility (`[0.55, 1.40]`) derived from the uppermost `0.30 km`
    /// of this tile's rock column (`Rock::soil_fertility`), weighted exponentially toward the surface
    /// (`exp(-depth / 0.12 km)`). Fresh basaltic hotspot shields and andesitic volcanic arcs weather
    /// into fertile Andisols (`~1.24..1.38`), whereas coral-limestone atoll caps (`~0.72`) and granitic
    /// cratons (`~0.85`) yield thinner, nutrient-limited soils.
    pub fn surface_soil_fertility(&self) -> f32 {
        if self.layers.is_empty() {
            return 1.0;
        }
        let max_depth_km = 0.30f32;
        let mut depth_km = 0.0f32;
        let mut w_sum = 0.0f32;
        let mut fert_sum = 0.0f32;
        for layer in self.layers.iter().rev() {
            if depth_km >= max_depth_km {
                break;
            }
            let take = layer.thickness.clamp(0.0, max_depth_km - depth_km);
            if take <= 1e-6 {
                continue;
            }
            let mid_depth = depth_km + 0.5 * take;
            let w = take * (-mid_depth / 0.12).exp();
            w_sum += w;
            fert_sum += w * layer.rock.soil_fertility();
            depth_km += take;
        }
        if w_sum > 1e-7 {
            (fert_sum / w_sum).clamp(0.55, 1.40)
        } else {
            self.layers
                .last()
                .map(|l| l.rock.soil_fertility())
                .unwrap_or(1.0)
        }
    }

    /// Initialize `soil_depth_m` and `soil_fertility` from current elevation and exposed bedrock.
    pub fn reset_surficial_soil_from_bedrock(&mut self) {
        self.soil_depth_m = if self.elevation() > 0.0 { 2.0 } else { 0.0 };
        self.soil_fertility = self.surface_soil_fertility();
    }

    /// Effective surficial soil fertility combining physical `soil_fertility` and `soil_depth_m`:
    /// - `0.0 m` (bare scoured bedrock) → `0.68x`
    /// - `2.0 m` (normal weathered regolith) → `1.00x`
    /// - `10.0+ m` (deep periglacial loess/till) → up to `1.08x`
    pub fn effective_soil_fertility(&self) -> f32 {
        let d = self.soil_depth_m.clamp(0.0, 25.0);
        let depth_factor = if d <= 2.0 {
            0.68 + 0.32 * (d / 2.0)
        } else {
            1.00 + 0.08 * ((d - 2.0) / 8.0).clamp(0.0, 1.0)
        };
        (self.soil_fertility * depth_factor).clamp(0.45, 1.55)
    }

    /// Detect stratigraphic impact crater / ejecta breccia signature in the near-surface rock layers:
    /// - `>= 0.65`: Shock-metamorphosed `hornfels` (`0.82 <= toughness <= 0.94`) crater basin floor.
    ///   (Note: plutonic contact-metamorphic `hornfels` along subduction zones has `toughness == 1.0` and is excluded.)
    /// - `0.35..0.60`: Coarse impact-breccia `conglomerate` (`clasticity ≈ IMPACT_BRECCIA_CLASTICITY`) ejecta rim.
    pub fn stratigraphic_impact_signature(&self) -> f32 {
        let mut best = 0.0f32;
        let mut buried_km = 0.0f32;
        for layer in self.layers.iter().rev().take(3) {
            if buried_km > 1.2 {
                break;
            }
            let atten = (-0.65 * buried_km).exp();
            if layer.rock.name == "hornfels" && (0.82..=0.94).contains(&layer.rock.toughness) {
                best = best.max(0.85 * atten);
            } else if layer.rock.name == "conglomerate"
                && layer
                    .rock
                    .clasticity
                    .is_some_and(|c| (c - Self::IMPACT_BRECCIA_CLASTICITY).abs() < 0.005)
            {
                best = best.max(0.52 * atten);
            }
            buried_km += layer.thickness;
        }
        best
    }

    /// Lightweight surface summary `(elev_km, mountainosity_norm, top_felsity, rock_name, rock_kind_code)`
    /// for view/rendering layers such as `tec11-sky-view`.
    pub fn surface_info_tuple(&self) -> (f32, f32, f32, &'static str, &'static str) {
        let (rock_name, rock_kind_code) = if let Some(top) = self.layers.last() {
            (top.rock.name, top.rock.kind.code())
        } else {
            ("basalt", "I")
        };
        (
            self.elevation(),
            self.mountainosity_norm(),
            self.top_felsity(),
            rock_name,
            rock_kind_code,
        )
    }

    pub fn impact_signature(&self) -> f32 {
        self.impact_scar.max(self.stratigraphic_impact_signature())
    }

    /// Enforce `MAX_THICKNESS` (`75 km`) and `MAX_HEIGHT` (`10 km`) (`Tile.limit` in `tile.py:62-81`),
    /// without clamping negative ocean bathymetry to `0`.
    pub fn limit(&mut self) {
        let mut cur_thick = self.thickness();
        if cur_thick > Self::MAX_THICKNESS && !self.layers.is_empty() {
            let mut rem_excess = cur_thick - Self::MAX_THICKNESS;
            let mut drop_count = 0usize;
            while drop_count < self.layers.len() && rem_excess > 0.0 {
                let lt = self.layers[drop_count].thickness;
                if rem_excess >= lt {
                    rem_excess -= lt;
                    drop_count += 1;
                } else {
                    self.layers[drop_count].thickness -= rem_excess;
                    rem_excess = 0.0;
                    break;
                }
            }
            let shrunk = (cur_thick - Self::MAX_THICKNESS) - rem_excess;
            if drop_count > 0 {
                self.layers.drain(0..drop_count);
            }
            cur_thick -= shrunk;
            self.bottom += shrunk;
        }

        let de = (self.bottom + cur_thick) - Self::MAX_HEIGHT;
        if de > 0.0 {
            self.bottom -= de;
        }

        if self.bottom > 0.0 {
            self.bottom = -cur_thick;
        }
    }

    /// Merge adjacent nearly-identical rock beds, absorb sub-2-meter same-kind micro-laminae,
    /// and bound total column stratigraphic units (`Tile.compact` in `tile.py:87-96`).
    pub fn compact(&mut self) {
        if self.layers.is_empty() {
            return;
        }

        // Pass 1 (`O(L)`): drop sub-centimeter dust (`<= 1e-5 km`) and merge adjacent beds
        // of the same lithology whose continuous properties are within tolerance.
        let mut pass1: Vec<RockLayer> = Vec::with_capacity(self.layers.len());
        for layer in self.layers.drain(..) {
            if layer.thickness <= 1e-5 {
                continue;
            }
            if let Some(prev) = pass1.last_mut() {
                if prev.rock == layer.rock {
                    prev.thickness += layer.thickness;
                    continue;
                }
                if prev.rock.is_nearly_identical(&layer.rock) {
                    prev.rock =
                        Rock::blend_weighted(&prev.rock, prev.thickness, &layer.rock, layer.thickness);
                    prev.thickness += layer.thickness;
                    continue;
                }
            }
            pass1.push(layer);
        }

        if pass1.len() <= 1 {
            self.layers = pass1;
            return;
        }

        // Pass 2 (`O(L)`): absorb ultra-thin (`< 2 m`) micro-laminae produced by tectonic
        // `merge_layers` tie-slicing or partial erosion into an adjacent layer of the same `RockKind`,
        // while preserving thin veneers of a different `RockKind` (e.g., fresh sediment on granite)
        // and special event markers (kimberlites, impact craters).
        let mut pass2: Vec<RockLayer> = Vec::with_capacity(pass1.len());
        for layer in pass1 {
            if let Some(prev) = pass2.last_mut() {
                let same_kind = prev.rock.kind == layer.rock.kind;
                let either_micro =
                    prev.thickness < Self::MICRO_LAYER_KM || layer.thickness < Self::MICRO_LAYER_KM;
                let neither_special =
                    !prev.rock.is_special_event_layer() && !layer.rock.is_special_event_layer();
                if same_kind && either_micro && neither_special {
                    prev.rock =
                        Rock::blend_weighted(&prev.rock, prev.thickness, &layer.rock, layer.thickness);
                    prev.thickness += layer.thickness;
                    continue;
                }
            }
            pass2.push(layer);
        }

        // Pass 3: enforce `MAX_LAYERS` cap so poly-orogenic tie-alternation never fragments a column
        // into thousands of sub-millimeter alternating slivers.
        while pass2.len() > Self::MAX_LAYERS {
            let mut best_idx = 0usize;
            let mut best_score = f32::INFINITY;
            for i in 0..(pass2.len() - 1) {
                let a = &pass2[i];
                let b = &pass2[i + 1];
                let mut score = a.thickness.min(b.thickness);
                if a.rock.kind == b.rock.kind {
                    score *= 0.25;
                }
                if a.rock.is_special_event_layer() || b.rock.is_special_event_layer() {
                    score *= 20.0;
                }
                if score < best_score {
                    best_score = score;
                    best_idx = i;
                }
            }
            let right = pass2.remove(best_idx + 1);
            let left = &mut pass2[best_idx];
            left.rock = Rock::blend_weighted(&left.rock, left.thickness, &right.rock, right.thickness);
            left.thickness += right.thickness;
        }

        // Final quick linear pass in case Pass 2/3 made two neighbors newly nearly-identical:
        let mut out: Vec<RockLayer> = Vec::with_capacity(pass2.len());
        for layer in pass2 {
            if let Some(prev) = out.last_mut() {
                if prev.rock == layer.rock || prev.rock.is_nearly_identical(&layer.rock) {
                    prev.rock =
                        Rock::blend_weighted(&prev.rock, prev.thickness, &layer.rock, layer.thickness);
                    prev.thickness += layer.thickness;
                    continue;
                }
            }
            out.push(layer);
        }
        self.layers = out;
    }

    pub fn transform(&mut self, layers: Vec<RockLayer>) {
        self.layers = layers;
        self.compact();
    }

    /// Merges multiple rock layer columns by voting from top to bottom (`Tile.mergelayers` in `tile.py`).
    pub fn merge_layers<R: Clone + PartialEq>(
        sources: &[(f32, f32, Vec<(R, f32)>)],
    ) -> (f32, f32, Vec<(R, f32)>) {
        let bottom = sources
            .iter()
            .map(|s| s.0)
            .fold(f32::INFINITY, f32::min);
        let max_elev = sources
            .iter()
            .map(|s| s.0 + s.2.iter().map(|l| l.1).sum::<f32>())
            .fold(f32::NEG_INFINITY, f32::max);

        let mut ss: Vec<Vec<(Option<R>, f32)>> = Vec::with_capacity(sources.len());
        for (b, _, layers) in sources {
            let mut padded = Vec::new();
            let db = *b - bottom;
            if db > 0.0 {
                padded.push((None, db));
            }
            for (r, t) in layers {
                padded.push((Some(r.clone()), *t));
            }
            let top = *b + layers.iter().map(|l| l.1).sum::<f32>();
            let de = max_elev - top;
            if de > 0.0 {
                padded.push((None, de));
            }
            ss.push(padded);
        }

        let mut output: Vec<(R, f32)> = Vec::new();
        let mut last_r: Option<R> = None;

        while ss.iter().any(|s| !s.is_empty()) {
            let min_t = ss
                .iter()
                .filter_map(|s| s.last().map(|l| l.1))
                .fold(f32::INFINITY, f32::min);

            let mut votes: Vec<(R, usize)> = Vec::new();
            for s in &mut ss {
                if let Some((rock_opt, thick)) = s.pop() {
                    if thick > min_t + 1e-6 {
                        s.push((rock_opt.clone(), thick - min_t));
                    }
                    if let Some(r) = rock_opt {
                        if let Some(entry) = votes.iter_mut().find(|(vr, _)| *vr == r) {
                            entry.1 += 1;
                        } else {
                            votes.push((r, 1));
                        }
                    }
                }
            }

            if votes.is_empty() {
                continue;
            }

            let max_vote = votes.iter().map(|(_, c)| *c).max().unwrap();
            let mut winners: Vec<R> = votes
                .into_iter()
                .filter(|(_, c)| *c == max_vote)
                .map(|(r, _)| r)
                .collect();

            if winners.len() > 1 {
                if let Some(ref prev) = last_r {
                    if let Some(pos) = winners.iter().position(|w| w == prev) {
                        winners.remove(pos);
                    }
                }
            }

            if let Some(chosen) = winners.into_iter().next() {
                last_r = Some(chosen.clone());
                output.push((chosen, min_t));
            }
        }

        // Compact adjacent identical layers and drop zero-thickness layers in O(N)
        let mut compacted: Vec<(R, f32)> = Vec::with_capacity(output.len());
        for (r, t) in output {
            if t <= 1e-6 {
                continue;
            }
            if let Some(prev) = compacted.last_mut() {
                if prev.0 == r {
                    prev.1 += t;
                    continue;
                }
            }
            compacted.push((r, t));
        }
        let mut output = compacted;

        output.reverse();
        let mountainosity =
            sources.iter().map(|s| s.1).sum::<f32>() / (sources.len().max(1) as f32);

        (bottom, mountainosity, output)
    }

    /// Average each group's contributing tiles with `merge_layers`, then stack colliding groups
    /// (`Tile.mergesources` in `tile.py:185-194`), carrying any continental `impact_scar` with the moving plate.
    pub fn merge_sources(&mut self, groups: &[Vec<TileSourceColumn>]) {
        self.merge_sources_owned(groups.to_vec());
    }

    /// Owned variant of `merge_sources` that moves single-source `RockLayer` vectors directly
    /// without allocating intermediate `converted` / `merge_layers` buffers.
    pub fn merge_sources_owned(&mut self, mut groups: Vec<Vec<TileSourceColumn>>) {
        if groups.len() == 1 && groups[0].len() == 1 {
            let s = groups[0].pop().unwrap();
            self.bottom = s.bottom;
            self.mountainosity = s.mountainosity;
            self.impact_scar = s.impact_scar;
            self.layers = s.layers;
            self.compact();
            self.limit();
            self.compact();
            return;
        }

        let mut merged_groups: Vec<(f32, f32, f32, Vec<RockLayer>)> =
            Vec::with_capacity(groups.len());
        for mut g in groups {
            if g.len() == 1 {
                let s = g.pop().unwrap();
                let mut tmp = TileGeology {
                    bottom: s.bottom,
                    mountainosity: s.mountainosity,
                    subduction: 0.0,
                    intrusion: None,
                    impact_scar: s.impact_scar,
                    soil_depth_m: 0.0,
                    soil_fertility: 1.0,
                    layers: s.layers,
                };
                tmp.compact();
                merged_groups.push((tmp.bottom, tmp.mountainosity, tmp.impact_scar, tmp.layers));
            } else {
                let n_src = g.len().max(1) as f32;
                let avg_b = g.iter().map(|s| s.bottom).sum::<f32>() / n_src;
                let avg_scar = g.iter().map(|s| s.impact_scar).sum::<f32>() / n_src;
                let avg_t = g
                    .iter()
                    .map(|s| s.layers.iter().map(|l| l.thickness).sum::<f32>())
                    .sum::<f32>()
                    / n_src;
                let converted: Vec<(f32, f32, Vec<(Rock, f32)>)> = g
                    .into_iter()
                    .map(|s| {
                        (
                            s.bottom,
                            s.mountainosity,
                            s.layers
                                .into_iter()
                                .map(|l| (l.rock, l.thickness))
                                .collect(),
                        )
                    })
                    .collect();
                let (_, mountainosity, mut layers) = Self::merge_layers(&converted);
                let raw_t: f32 = layers.iter().map(|l| l.1).sum();
                if raw_t > 1e-6 {
                    let scale = avg_t / raw_t;
                    for l in &mut layers {
                        l.1 *= scale;
                    }
                }
                let rock_layers: Vec<RockLayer> = layers
                    .into_iter()
                    .map(|(rock, thickness)| RockLayer { rock, thickness })
                    .collect();
                merged_groups.push((avg_b, mountainosity, avg_scar, rock_layers));
            }
        }

        if merged_groups.len() == 1 {
            let (b, m, scar, ls) = merged_groups.into_iter().next().unwrap();
            self.bottom = b;
            self.mountainosity = m;
            self.impact_scar = scar;
            self.layers = ls;
            self.limit();
            self.compact();
        } else {
            let max_elev = merged_groups
                .iter()
                .map(|(b, _, _, ls)| *b + ls.iter().map(|l| l.thickness).sum::<f32>())
                .fold(0.5f32, f32::max);
            let mut target_elev = max_elev;
            for _ in 1..merged_groups.len() {
                let uplift = 2.6 * ((7.2 - target_elev) / 6.4).clamp(0.08, 1.0);
                target_elev = (target_elev + uplift).min(7.8);
            }
            self.mountainosity = merged_groups.iter().map(|g| g.1).sum::<f32>()
                + 4.0 * (merged_groups.len().saturating_sub(1) as f32);
            // Continental collision / orogeny partially obliterates pre-existing impact scars:
            let mean_scar = merged_groups.iter().map(|g| g.2).sum::<f32>()
                / (merged_groups.len().max(1) as f32);
            self.impact_scar = mean_scar * 0.55;
            self.layers = merged_groups.into_iter().flat_map(|g| g.3).collect();
            self.bottom = target_elev - self.thickness();
            self.limit();
            self.compact();
            self.bottom = target_elev.min(Self::MAX_HEIGHT) - self.thickness();
        }
    }

    /// Subduction uplift at the leading edge of a moving continent (`Tile.build` in `tile.py:195-201`).
    pub fn build(&mut self, amount: f32, rock: Rock) {
        if amount <= 0.0 {
            return;
        }
        self.bottom -= amount;
        self.layers.insert(
            0,
            RockLayer {
                rock,
                thickness: 2.0 * amount,
            },
        );
        self.limit();
        self.compact();
        self.subduction = amount;
        self.mountainosity += amount;
    }

    /// Strip top rock layers by erosion degree (`Tile.erode` in `tile.py:203-245`),
    /// capped by `max_erosion` (the local base-level relief limit), returning `(total_eroded, stripped_layers)`.
    pub fn erode_column(&mut self, degree: f32, dt: f32, max_erosion: f32) -> (f32, Vec<RockLayer>) {
        let mut rem_degree = degree;
        let mut rem_erosion = max_erosion.min(self.elevation().max(0.0)).max(0.0);
        let mut materials: Vec<RockLayer> = Vec::new();

        while rem_degree > 0.0 && rem_erosion > 1e-6 && !self.layers.is_empty() {
            let layer = self.layers.pop().unwrap();
            let rate = 0.5 + 9.5 * (1.0 - layer.rock.toughness);
            let potential = rem_erosion.min(rem_degree * rate * dt);

            if potential < layer.thickness {
                if potential > 1e-6 {
                    materials.push(RockLayer {
                        rock: layer.rock.clone(),
                        thickness: potential,
                    });
                }
                self.layers.push(RockLayer {
                    rock: layer.rock,
                    thickness: layer.thickness - potential,
                });
                break;
            } else if self.layers.is_empty() {
                // Leave at least MIN_THICKNESS behind on the bottom-most layer
                let amount = (potential - Self::MIN_THICKNESS).min(layer.thickness - Self::MIN_THICKNESS);
                if amount > 0.0001 {
                    materials.push(RockLayer {
                        rock: layer.rock.clone(),
                        thickness: amount,
                    });
                    self.layers.push(RockLayer {
                        rock: layer.rock,
                        thickness: layer.thickness - amount,
                    });
                } else {
                    self.layers.push(layer);
                }
                break;
            } else {
                rem_erosion -= layer.thickness;
                rem_degree -= layer.thickness / (rate * dt);
                materials.push(layer);
            }
        }

        let total_eroded: f32 = materials.iter().map(|m| m.thickness).sum();
        self.mountainosity = (self.mountainosity - total_eroded).max(0.0);
        if total_eroded > 0.0 && self.impact_scar > 0.0 {
            self.impact_scar *= (1.0 - 0.35 * total_eroded).clamp(0.25, 1.0);
        }
        self.compact();
        (total_eroded, materials)
    }

    /// Deposit a sedimentary rock layer on top of the column (`Tile.deposit` in `tile.py:246-250`).
    pub fn deposit(&mut self, layer: RockLayer) {
        if layer.thickness <= 1e-6 {
            return;
        }
        let t = layer.thickness;
        self.layers.push(layer);
        self.limit();
        self.mountainosity = (self.mountainosity - 0.25 * t).max(0.0);
        if self.impact_scar > 0.0 {
            self.impact_scar *= (1.0 - 0.30 * t).clamp(0.30, 1.0);
        }
        self.compact();
    }

    /// Apply isostatic adjustment to `bottom` (`Tile.isostasize` in `tile.py:252-254`).
    pub fn isostasize(&mut self, amount: f32) {
        self.bottom += amount;
        self.limit();
    }

    /// Emplace a 1 km plutonic intrusion at depth `max(2, thickness * (1 - felsity))`
    /// (`Tile.intrude` in `tile.py:256-270`).
    pub fn intrude(&mut self, rock: Rock) {
        let depth = (self.thickness() * (1.0 - rock.felsity)).max(2.0);
        let mut l = self.layers.len();
        let mut d = 0.0f32;
        while d < depth && l > 0 {
            l -= 1;
            d += self.layers[l].thickness;
        }
        let intrusion_layer = RockLayer {
            rock,
            thickness: 1.0,
        };
        self.layers.insert(l, intrusion_layer);
        self.bottom -= 1.0;
        self.limit();
        self.compact();
        self.intrusion = Some((d, d + 1.0));
    }

    /// Explosive volatile-rich ultramafic kimberlite / lamproite pipe eruption punching from
    /// the base of thick continental crust (`>= 20 km`) straight to the surface:
    /// - Scans deep lithospheric root layers (`depth >= 20 km`, especially `eclogite` and deep
    ///   `amphibolite`/`granulite`/`blueschist`) for high-pressure carbon (`bogginess` and reduced
    ///   subducted/buried carbonate `calcity`), converting it into entrained diamond xenocrysts.
    /// - Excavates up to `0.35 km` of surface rock into a maar crater and emplaces a `0.75 km`
    ///   ultramafic kimberlite diatreme pipe (`felsity = 0.28`), leaving `+0.20 km` of net surface
    ///   tuff-ring relief while extending the pipe root `0.55 km` into the upper crust.
    /// Returns `true` if the pipe carries diamond xenocrysts (`bogginess >= 0.05`).
    pub fn erupt_kimberlite(&mut self) -> bool {
        let total_thick = self.thickness();
        if total_thick < 20.0 || self.layers.is_empty() {
            return false;
        }

        // 1. Sample deep lithospheric layers (`depth >= 20 km` from surface) for high-pressure carbon:
        let mut depth_top = 0.0f32;
        let mut weighted_carbon = 0.0f32;
        let mut weight_sum = 0.0f32;
        let mut peak_diamond = 0.0f32;
        for l in self.layers.iter().rev() {
            let depth_bot = depth_top + l.thickness;
            let deep_thick = (depth_bot - depth_top.max(20.0)).max(0.0);
            if deep_thick > 1e-4 {
                let r = &l.rock;
                let raw_c = r
                    .bogginess
                    .unwrap_or(0.0)
                    .max(0.20 * r.calcity.unwrap_or(0.0));
                let p_factor = if r.name == "eclogite" || depth_bot >= 50.0 {
                    1.75
                } else if matches!(r.name, "amphibolite" | "granulite" | "blueschist")
                    && depth_bot >= 28.0
                {
                    1.15
                } else {
                    0.60
                };
                let d_grade = (raw_c * p_factor).clamp(0.0, 1.0);
                peak_diamond = peak_diamond.max(d_grade);
                weighted_carbon += deep_thick * d_grade;
                weight_sum += deep_thick;
            }
            depth_top = depth_bot;
        }

        let avg_diamond = if weight_sum > 1e-4 {
            weighted_carbon / weight_sum
        } else {
            0.0
        };
        // Kimberlite pipe samples both peak eclogite lenses and average deep root carbon:
        let diamond_grade = (0.65 * peak_diamond + 0.35 * avg_diamond).clamp(0.0, 1.0);

        // 2. Blast maar diatreme crater through top ~0.35 km of country rock:
        let orig_elev = self.elevation();
        let excavate = (self.thickness() - Self::MIN_THICKNESS)
            .max(0.0)
            .min(0.35);
        if excavate > 1e-4 {
            let _ = self.erode_column(excavate * 4.0, 1.0, excavate);
        }

        // 3. Emplace 0.75 km ultramafic kimberlite diatreme + maar tuff ring (`felsity = 0.28`):
        let mut kimberlite = Rock::extrusive(0.28);
        let has_diamonds = diamond_grade >= 0.05;
        if has_diamonds {
            kimberlite.bogginess = Some(diamond_grade);
        }
        kimberlite.sulfidity = Some(0.06);

        self.layers.push(RockLayer {
            rock: kimberlite,
            thickness: 0.75,
        });
        let target_elev = (orig_elev + 0.20).min(Self::MAX_HEIGHT);
        self.bottom = target_elev - self.thickness();
        self.limit();
        self.compact();
        has_diamonds
    }

    pub fn clear_temp(&mut self) {
        self.subduction = 0.0;
        self.intrusion = None;
    }

    /// Normalize rock layers and temporary fields in-place to match a snapshot roundtrip
    /// (`from_proto_geological_tile(&self.to_proto_geological_tile())`) without heap allocations.
    pub fn normalize_for_snapshot(&mut self) {
        use tec11_proto::MetamorphicFaciesExt;
        for l in &mut self.layers {
            match l.rock.kind {
                RockKind::Igneous => {
                    let is_intrusive = matches!(l.rock.name, "gabbro" | "andesite" | "granite");
                    let quartzity = l.rock.quartzity;
                    let sulfidity = l.rock.sulfidity;
                    let bogginess = l.rock.bogginess;
                    let mut r = if is_intrusive {
                        Rock::intrusive(l.rock.felsity)
                    } else {
                        Rock::extrusive(l.rock.felsity)
                    };
                    r.quartzity = quartzity;
                    r.sulfidity = sulfidity;
                    r.bogginess = bogginess;
                    l.rock = r;
                }
                RockKind::Sedimentary => {
                    let clasticity = l.rock.clasticity.unwrap_or(1.0).max(1e-6);
                    let felsity = l.rock.felsity;
                    let calcity = l.rock.calcity;
                    let bogginess = l.rock.bogginess;
                    let quartzity = l.rock.quartzity;
                    let sulfidity = l.rock.sulfidity;
                    let salinity = l.rock.salinity;
                    let name = if calcity.unwrap_or(0.0) > 0.99 {
                        "chalk"
                    } else if calcity.unwrap_or(0.0) > 0.75 {
                        "limestone"
                    } else {
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
                    };
                    l.rock = Rock {
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
                    };
                }
                RockKind::Metamorphic => {
                    let facies = tec11_proto::MetamorphicFacies::from_rock_name(l.rock.name);
                    l.rock = Rock {
                        kind: RockKind::Metamorphic,
                        name: facies.rock_name(),
                        felsity: l.rock.felsity,
                        toughness: l.rock.toughness,
                        clasticity: None,
                        calcity: l.rock.calcity,
                        bogginess: l.rock.bogginess,
                        quartzity: l.rock.quartzity,
                        sulfidity: l.rock.sulfidity,
                        salinity: l.rock.salinity,
                    };
                }
            }
        }
        self.subduction = 0.0;
        self.intrusion = None;
        self.impact_scar = self.stratigraphic_impact_signature();
    }

    pub fn to_proto_geological_tile(&self) -> tec11_proto::GeologicalTile {
        let layers = self
            .layers
            .iter()
            .map(|l| {
                let r = &l.rock;
                let kind = match r.kind {
                    RockKind::Igneous => {
                        let is_intrusive = matches!(r.name, "gabbro" | "andesite" | "granite");
                        tec11_proto::RockKindOneof::Igneous(tec11_proto::IgneousRock {
                            is_intrusive,
                            felsity: r.felsity,
                            quartzity: r.quartzity,
                            sulfidity: r.sulfidity,
                            bogginess: r.bogginess,
                        })
                    }
                    RockKind::Sedimentary => {
                        tec11_proto::RockKindOneof::Sedimentary(tec11_proto::SedimentaryRock {
                            clasticity: r.clasticity.unwrap_or(1.0),
                            felsity: Some(r.felsity),
                            calcity: r.calcity,
                            bogginess: r.bogginess,
                            quartzity: r.quartzity,
                            sulfidity: r.sulfidity,
                            salinity: r.salinity,
                        })
                    }
                    RockKind::Metamorphic => {
                        use tec11_proto::MetamorphicFaciesExt;
                        tec11_proto::RockKindOneof::Metamorphic(tec11_proto::MetamorphicRock {
                            facies: tec11_proto::MetamorphicFacies::from_rock_name(r.name) as i32,
                            toughness: r.toughness,
                            felsity: Some(r.felsity),
                            calcity: r.calcity,
                            bogginess: r.bogginess,
                            quartzity: r.quartzity,
                            sulfidity: r.sulfidity,
                            salinity: r.salinity,
                        })
                    }
                };
                tec11_proto::RockLayer {
                    rock: Some(tec11_proto::Rock { kind: Some(kind) }),
                    thickness: l.thickness,
                }
            })
            .collect();
        tec11_proto::GeologicalTile {
            bottom: self.bottom,
            mountainosity: self.mountainosity,
            layers,
            soil_depth_m: Some(self.soil_depth_m),
            soil_fertility: Some(self.soil_fertility),
        }
    }

    pub fn from_proto_geological_tile(pt: &tec11_proto::GeologicalTile) -> Option<Self> {
        use tec11_proto::MetamorphicFaciesExt;
        let mut layers = Vec::with_capacity(pt.layers.len());
        for pl in &pt.layers {
            let pr = pl.rock.as_ref()?;
            let rock = match pr.kind.as_ref()? {
                tec11_proto::RockKindOneof::Igneous(ig) => {
                    let mut r = if ig.is_intrusive {
                        Rock::intrusive(ig.felsity)
                    } else {
                        Rock::extrusive(ig.felsity)
                    };
                    r.quartzity = ig.quartzity;
                    r.sulfidity = ig.sulfidity;
                    r.bogginess = ig.bogginess;
                    r
                }
                tec11_proto::RockKindOneof::Sedimentary(sed) => {
                    let clasticity = sed.clasticity.max(1e-6);
                    let felsity = sed.felsity.unwrap_or(0.5);
                    let calcity = sed.calcity;
                    let bogginess = sed.bogginess;
                    let name = if calcity.unwrap_or(0.0) > 0.99 {
                        "chalk"
                    } else if calcity.unwrap_or(0.0) > 0.75 {
                        "limestone"
                    } else {
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
                    };
                    Rock {
                        kind: RockKind::Sedimentary,
                        name,
                        felsity,
                        toughness: 0.0,
                        clasticity: Some(clasticity),
                        calcity,
                        bogginess,
                        quartzity: sed.quartzity,
                        sulfidity: sed.sulfidity,
                        salinity: sed.salinity,
                    }
                }
                tec11_proto::RockKindOneof::Metamorphic(met) => Rock {
                    kind: RockKind::Metamorphic,
                    name: met.facies().rock_name(),
                    felsity: met.felsity.unwrap_or(0.5),
                    toughness: met.toughness,
                    clasticity: None,
                    calcity: met.calcity,
                    bogginess: met.bogginess,
                    quartzity: met.quartzity,
                    sulfidity: met.sulfidity,
                    salinity: met.salinity,
                },
            };
            layers.push(RockLayer {
                rock,
                thickness: pl.thickness,
            });
        }
        let mut tile = Self {
            bottom: pt.bottom,
            mountainosity: pt.mountainosity,
            subduction: 0.0,
            intrusion: None,
            impact_scar: 0.0,
            soil_depth_m: 0.0,
            soil_fertility: 1.0,
            layers,
        };
        let default_depth = if tile.elevation() > 0.0 { 2.0 } else { 0.0 };
        let default_fert = tile.surface_soil_fertility();
        tile.soil_depth_m = pt.soil_depth_m.unwrap_or(default_depth).clamp(0.0, 25.0);
        tile.soil_fertility = pt.soil_fertility.unwrap_or(default_fert).clamp(0.45, 1.50);
        tile.impact_scar = tile.stratigraphic_impact_signature();
        Some(tile)
    }

    fn push_occurrence(
        out: &mut Vec<DepositOccurrence>,
        base_name: &str,
        grade: f32,
        thickness_km: f32,
        bedded: bool,
        is_interface: bool,
        minerals: Vec<(MineralKind, f32)>,
    ) {
        let g = grade.clamp(0.0, 1.0);
        let Some(tier) = GradeTier::from_grade(g) else {
            return;
        };
        let eff_thick = thickness_km.max(0.01);
        let abundance = g * eff_thick;
        let pct = (g * 100.0).round().clamp(1.0, 100.0) as u32;
        let contact_tag = if is_interface { "[Contact] " } else { "" };
        let label = format!("{contact_tag}{}{base_name} ({pct}%)", tier.prefix(bedded));
        let filtered_mins: Vec<(MineralKind, f32)> = minerals
            .into_iter()
            .map(|(mk, mg)| (mk, mg.clamp(0.0, 1.0)))
            .filter(|&(_, mg)| mg >= TRACE_GRADE)
            .collect();
        out.push(DepositOccurrence {
            label,
            grade: g,
            thickness_km: eff_thick,
            abundance,
            is_interface,
            minerals: filtered_mins,
        });
    }

    /// Deterministically evaluate continuous `[0.0, 1.0]` precious and semi-precious gemstone
    /// grades in `self.layers[layer_idx]` at `depth_from_surface_km` (`0.0` = absent).
    pub fn derive_layer_gems(&self, layer_idx: usize, depth_from_surface_km: f32) -> LayerGems {
        self.derive_layer_gems_with_elev(layer_idx, depth_from_surface_km, self.elevation())
    }

    /// `O(1)` variant of `derive_layer_gems` using a precomputed `surface_elev_km`.
    pub fn derive_layer_gems_with_elev(
        &self,
        layer_idx: usize,
        depth_from_surface_km: f32,
        surface_elev_km: f32,
    ) -> LayerGems {
        let Some(l) = self.layers.get(layer_idx) else {
            return LayerGems::default();
        };
        let r = &l.rock;
        let bog = r.bogginess.unwrap_or(0.0);
        let q = r.quartzity.unwrap_or(0.0);
        let s = r.sulfidity.unwrap_or(0.0);
        let sal = r.salinity.unwrap_or(0.0);
        let calc = r.calcity.unwrap_or(0.0);
        let is_bif = r.kind != RockKind::Igneous
            && s >= 0.12
            && (0.18..=0.44).contains(&q)
            && q <= 2.6 * s
            && bog < 0.03
            && calc < 0.15
            && r.clasticity.unwrap_or(1.5) >= 0.8;

        let adj_felsic_max = [layer_idx.checked_sub(1), Some(layer_idx + 1)]
            .into_iter()
            .flatten()
            .filter_map(|idx| self.layers.get(idx))
            .filter(|nb| nb.rock.kind == RockKind::Igneous && nb.rock.felsity > 0.62)
            .map(|nb| nb.rock.felsity)
            .fold(0.0f32, f32::max);

        // 1. Diamond: kimberlite diatreme pipes, UHP eclogite lenses, or Popigai impact-shocked carbon
        let diamond = if r.kind == RockKind::Igneous && r.felsity <= 0.35 && bog >= 0.04 {
            ((bog - 0.03) / 0.28).clamp(0.08, 1.0)
        } else if r.name == "eclogite" && bog >= 0.06 {
            ((bog - 0.04) / 0.36).clamp(0.08, 1.0)
        } else if (r.name == "hornfels" && (0.82..=0.94).contains(&r.toughness) && bog >= 0.06)
            || (self.impact_scar >= 0.60 && bog >= 0.10)
        {
            ((bog - 0.04) / 0.30).clamp(0.10, 1.0)
        } else {
            0.0
        };

        // 2. Lapis Lazuli: Sar-e-Sang type high-grade evaporitic pyrite-lazurite marble
        let lapis = if r.name == "marble"
            && r.toughness >= 0.78
            && calc >= 0.65
            && s >= 0.14
            && sal >= 0.08
        {
            ((s - 0.10) * 2.5 * (sal / 0.10).min(1.8)).clamp(0.0, 1.0)
        } else {
            0.0
        };

        // 3. Ruby: Mogok-type silica-undersaturated (`q < 0.10`) Cr-bearing evaporitic/mafic marble
        let ruby = if r.name == "marble"
            && r.toughness >= 0.78
            && (0.70..0.97).contains(&calc)
            && q < 0.10
            && s < 0.08
            && (sal >= 0.06 || r.felsity <= 0.51)
        {
            ((0.10 - q)
                * 5.5
                * (0.4 + sal * 3.5 + (0.53 - r.felsity).max(0.0) * 4.0))
                .clamp(0.0, 1.0)
        } else {
            0.0
        };

        // 4. Sapphire: silica-undersaturated (`q < 0.06`) Ti-Fe corundum in high-grade granulite or marble
        let sapphire = if !LayerGems::is_present(ruby) && !LayerGems::is_present(lapis) {
            if r.name == "granulite"
                && r.toughness >= 0.82
                && q < 0.05
                && (0.48..=0.58).contains(&r.felsity)
            {
                ((0.05 - q) * 9.0 * ((r.toughness - 0.70) * 3.5).clamp(0.5, 1.5)).clamp(0.0, 1.0)
            } else if r.name == "marble"
                && r.toughness >= 0.82
                && (0.78..0.97).contains(&calc)
                && q < 0.06
                && s < 0.06
                && sal < 0.06
                && (0.52..=0.62).contains(&r.felsity)
            {
                ((0.06 - q) * 7.5).clamp(0.0, 1.0)
            } else {
                0.0
            }
        } else {
            0.0
        };

        // 5. Emerald: requires Be source (`adj_felsic_max > 0.62`) + Cr/V mafic schist, OR Muzo-type evaporitic black pelite
        let emerald = if matches!(r.name, "greenschist" | "amphibolite")
            && r.felsity <= 0.50
            && adj_felsic_max > 0.62
            && q >= 0.07
        {
            ((q - 0.04)
                * 4.5
                * ((adj_felsic_max - 0.60) * 5.0).clamp(0.5, 1.4)
                * ((0.54 - r.felsity) * 14.0).clamp(0.5, 1.4))
            .clamp(0.0, 1.0)
        } else if r.name == "pelite" && bog >= 0.14 && q >= 0.14 && sal >= 0.08 {
            ((bog - 0.10) * 2.2 * ((q - 0.10) * 3.0).clamp(0.4, 1.4) * (sal * 8.0).clamp(0.5, 1.5))
                .clamp(0.0, 1.0)
        } else {
            0.0
        };

        // 6. Jade: high-P/low-T subduction blueschist jadeite (`felsity <= 0.51`, `q in 0.06..0.20`, low sulfide)
        let jade = if r.name == "blueschist"
            && r.felsity <= 0.51
            && (0.06..=0.20).contains(&q)
            && s < 0.14
        {
            ((0.525 - r.felsity) * 11.5 * (q / 0.09).clamp(0.6, 1.4)).clamp(0.0, 1.0)
        } else {
            0.0
        };

        // 7. Turquoise: shallow arid supergene oxidation zone over Cu-rich intermediate/felsic host
        let turquoise = if depth_from_surface_km < 0.5
            && surface_elev_km > 0.15
            && !is_bif
            && s >= 0.20
            && sal >= 0.12
            && matches!(r.kind, RockKind::Igneous | RockKind::Metamorphic)
            && r.felsity >= 0.52
            && r.name != "marble"
        {
            ((s - 0.16) * 2.4 * (sal / 0.15).clamp(0.5, 1.6)).clamp(0.0, 1.0)
        } else {
            0.0
        };

        // 8. Precious Opal: shallow arid silica-gel weathering in fine clastics or felsic volcanics
        let opal = if depth_from_surface_km < 0.45
            && surface_elev_km > 0.05
            && !is_bif
            && q >= 0.22
            && sal >= 0.18
            && ((r.kind == RockKind::Sedimentary && r.clasticity.unwrap_or(1.0) >= 1.5)
                || matches!(r.name, "rhyolite" | "dacite"))
        {
            ((q - 0.18) * 2.2 * ((sal - 0.14) / 0.20).clamp(0.4, 1.5)).clamp(0.0, 1.0)
        } else {
            0.0
        };

        LayerGems {
            diamond: if LayerGems::is_present(diamond) { diamond } else { 0.0 },
            ruby: if LayerGems::is_present(ruby) { ruby } else { 0.0 },
            sapphire: if LayerGems::is_present(sapphire) { sapphire } else { 0.0 },
            emerald: if LayerGems::is_present(emerald) { emerald } else { 0.0 },
            jade: if LayerGems::is_present(jade) { jade } else { 0.0 },
            lapis: if LayerGems::is_present(lapis) { lapis } else { 0.0 },
            turquoise: if LayerGems::is_present(turquoise) { turquoise } else { 0.0 },
            opal: if LayerGems::is_present(opal) { opal } else { 0.0 },
        }
    }

    /// Deterministically evaluate quantitative `DepositOccurrence` entries (continuous grade,
    /// mineralized thickness, and abundance) inside `self.layers[layer_idx]`.
    pub fn derive_layer_occurrences(
        &self,
        layer_idx: usize,
        depth_from_surface_km: f32,
    ) -> Vec<DepositOccurrence> {
        self.derive_layer_occurrences_with_elev(layer_idx, depth_from_surface_km, self.elevation())
    }

    /// `O(1)` variant of `derive_layer_occurrences` using a precomputed `surface_elev_km`.
    pub fn derive_layer_occurrences_with_elev(
        &self,
        layer_idx: usize,
        depth_from_surface_km: f32,
        surface_elev_km: f32,
    ) -> Vec<DepositOccurrence> {
        let Some(l) = self.layers.get(layer_idx) else {
            return Vec::new();
        };
        let r = &l.rock;
        let thick = l.thickness.max(0.01);
        let mut out = Vec::new();

        let bog = r.bogginess.unwrap_or(0.0);
        let q = r.quartzity.unwrap_or(0.0);
        let s = r.sulfidity.unwrap_or(0.0);
        let sal = r.salinity.unwrap_or(0.0);
        let calc = r.calcity.unwrap_or(0.0);
        let gems =
            self.derive_layer_gems_with_elev(layer_idx, depth_from_surface_km, surface_elev_km);

        // 1. Carbon / Coal / Graphite / Primary Diamonds:
        if r.kind == RockKind::Igneous && r.felsity <= 0.35 {
            if gems.diamond >= TRACE_GRADE {
                let pipe_thick = (thick * 0.28).clamp(0.05, 0.35);
                Self::push_occurrence(
                    &mut out,
                    "Diamondiferous Kimberlite Pipe (Gem Diamonds & Pyrope Garnet)",
                    gems.diamond,
                    pipe_thick,
                    false,
                    false,
                    vec![(MineralKind::Diamond, gems.diamond)],
                );
            }
        } else if bog >= 0.05 {
            let seam_thick = (thick * (0.14 + 0.36 * bog)).min(thick).max(0.01);
            match r.kind {
                RockKind::Sedimentary => {
                    let g = ((bog - 0.03) / 0.55).clamp(0.08, 1.0);
                    let rank = if depth_from_surface_km < 1.0 {
                        "Peat / Lignite Beds"
                    } else {
                        "Lignite (Brown Coal) Seams"
                    };
                    Self::push_occurrence(
                        &mut out,
                        rank,
                        g,
                        seam_thick,
                        true,
                        false,
                        vec![(MineralKind::Coal, g)],
                    );
                }
                RockKind::Metamorphic => match r.name {
                    "pelite" | "psammite" | "psephite" => {
                        let g = ((bog - 0.03) / 0.52).clamp(0.08, 1.0);
                        Self::push_occurrence(
                            &mut out,
                            "Bituminous Coal Seams",
                            g,
                            seam_thick,
                            true,
                            false,
                            vec![(MineralKind::Coal, g)],
                        );
                    }
                    "greenschist" | "blueschist" => {
                        let g = ((bog - 0.03) / 0.48).clamp(0.08, 1.0);
                        Self::push_occurrence(
                            &mut out,
                            "Anthracite Coal Seams",
                            g,
                            seam_thick,
                            true,
                            false,
                            vec![(MineralKind::Coal, g)],
                        );
                    }
                    "hornfels" => {
                        if (0.82..=0.94).contains(&r.toughness) && gems.diamond >= TRACE_GRADE {
                            Self::push_occurrence(
                                &mut out,
                                "Impact-Shocked Diamondiferous Carbon (Popigai-Type)",
                                gems.diamond,
                                (seam_thick * 0.4).clamp(0.02, 0.25),
                                false,
                                false,
                                vec![(MineralKind::Diamond, gems.diamond)],
                            );
                        } else {
                            let g = ((bog - 0.03) / 0.50).clamp(0.08, 1.0);
                            Self::push_occurrence(
                                &mut out,
                                "Contact Anthracite / Natural Coke",
                                g,
                                seam_thick,
                                true,
                                false,
                                vec![(MineralKind::Coal, g)],
                            );
                        }
                    }
                    "amphibolite" | "granulite" => {
                        let g = ((bog - 0.04) / 0.50).clamp(0.08, 1.0);
                        Self::push_occurrence(
                            &mut out,
                            "Crystalline Graphite Seams",
                            g,
                            seam_thick,
                            true,
                            false,
                            vec![(MineralKind::Graphite, g)],
                        );
                    }
                    "eclogite" => {
                        let g_d = gems.diamond.max(0.08);
                        Self::push_occurrence(
                            &mut out,
                            "Diamondiferous Graphite / Micro-diamonds",
                            g_d,
                            (seam_thick * 0.35).clamp(0.02, 0.25),
                            false,
                            false,
                            vec![(MineralKind::Diamond, g_d), (MineralKind::Graphite, g_d)],
                        );
                    }
                    _ => {
                        let g = ((bog - 0.03) / 0.52).clamp(0.08, 1.0);
                        Self::push_occurrence(
                            &mut out,
                            "Coal / Carbon Seams",
                            g,
                            seam_thick,
                            true,
                            false,
                            vec![(MineralKind::Coal, g)],
                        );
                    }
                },
                RockKind::Igneous => {
                    let g = ((bog - 0.04) / 0.45).clamp(0.08, 1.0);
                    Self::push_occurrence(
                        &mut out,
                        "Carbonatite / Graphite Xenoliths",
                        g,
                        (thick * 0.15).clamp(0.02, 0.25),
                        false,
                        false,
                        vec![(MineralKind::Graphite, g)],
                    );
                }
            }
        }

        // 2. Banded Iron Formations (co-precipitated marine iron/sulfide + chert):
        let is_bif = r.kind != RockKind::Igneous
            && s >= 0.12
            && (0.18..=0.44).contains(&q)
            && q <= 2.6 * s
            && bog < 0.03
            && calc < 0.15
            && r.clasticity.unwrap_or(1.5) >= 0.8;
        if is_bif {
            let g_fe = ((0.55 * s + 0.45 * q - 0.10) / 0.32).clamp(0.12, 1.0);
            let g_flint = ((q - 0.14) / 0.30).clamp(0.10, 1.0);
            let name = if r.kind == RockKind::Metamorphic {
                "Metamorphosed Banded Iron (Taconite / Magnetite-Chert)"
            } else {
                "Banded Iron Formation (Hematite-Magnetite Chert)"
            };
            Self::push_occurrence(
                &mut out,
                name,
                g_fe,
                thick * 0.65,
                true,
                false,
                vec![(MineralKind::Iron, g_fe), (MineralKind::Flint, g_flint)],
            );
        }

        // 3. Evaporite salt/gypsum/potash beds from `salinity`:
        if sal >= 0.08 {
            let g_sal = ((sal - 0.05) / 0.45).clamp(0.08, 1.0);
            let evap_thick = (thick * (0.20 + 0.50 * sal)).min(thick).max(0.01);
            if sal >= 0.42 {
                Self::push_occurrence(
                    &mut out,
                    "Evaporite Beds (Halite, Potash & Lithium Brines)",
                    g_sal,
                    evap_thick,
                    true,
                    false,
                    vec![(MineralKind::Salt, g_sal), (MineralKind::Gypsum, g_sal * 0.8)],
                );
            } else if sal >= 0.18 {
                Self::push_occurrence(
                    &mut out,
                    "Evaporite Beds (Rock Salt / Halite & Gypsum)",
                    g_sal,
                    evap_thick,
                    true,
                    false,
                    vec![
                        (MineralKind::Salt, g_sal),
                        (MineralKind::Gypsum, (g_sal * 0.85).max(0.10)),
                    ],
                );
            } else {
                Self::push_occurrence(
                    &mut out,
                    "Gypsum & Anhydrite Evaporite Lenses",
                    g_sal,
                    evap_thick,
                    true,
                    false,
                    vec![(MineralKind::Gypsum, g_sal)],
                );
            }
        }

        // 4. Nodular Flint in Limestone / Chalk:
        if matches!(r.name, "limestone" | "chalk") && q >= 0.05 {
            let g_flint = ((q - 0.03) / 0.22).clamp(0.10, 1.0);
            Self::push_occurrence(
                &mut out,
                "Nodular Flint & Chert Horizons",
                g_flint,
                (thick * 0.18).clamp(0.02, 0.30),
                true,
                false,
                vec![(MineralKind::Flint, g_flint)],
            );
        }

        // 5. Hydrothermal Quartz Veins, Felsic Pegmatite/Greisens, and Alluvial Placers:
        // Crucially separates ubiquitous barren milky quartz / eroded BIF chert (`q = 0.08..0.35`)
        // from true high-grade Gold & Cassiterite (Tin) lodes and proximal fluvial placers.
        if q >= 0.08 && !(is_bif && r.kind == RockKind::Sedimentary) {
            let vein_thick = (thick * (0.06 + 0.18 * q)).clamp(0.02, 0.35);
            match r.kind {
                RockKind::Sedimentary => {
                    let clast = r.clasticity.unwrap_or(2.0);
                    let is_proximal_terrestrial =
                        surface_elev_km > 0.05 && clast <= 1.08 && calc < 0.30;
                    // Native gold (19.3 g/cm^3) and cassiterite (7.0 g/cm^3) lag in steep,
                    // high-relief mountain/piedmont streams (`mountainosity_norm >= 0.28`),
                    // whereas flat zero-relief plateaus and plains accumulate ordinary quartz sand.
                    let high_energy_stream = self.mountainosity_norm() >= 0.28;
                    let g_au = if is_proximal_terrestrial
                        && high_energy_stream
                        && clast <= 1.02
                        && r.felsity <= 0.49
                        && q >= 0.66
                        && s >= 0.17
                    {
                        ((q - 0.62) * 2.4 * (s / 0.18).clamp(0.5, 1.5)).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let g_sn = if is_proximal_terrestrial
                        && high_energy_stream
                        && r.felsity >= 0.72
                        && q >= 0.44
                    {
                        ((q - 0.40) * 2.0 * ((r.felsity - 0.70) / 0.16).clamp(0.4, 1.5))
                            .clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let g_qtz = ((q - 0.08) / 0.45).clamp(0.08, 1.0);
                    let placer_thick = (thick * 0.12).clamp(0.02, 0.22);

                    if g_au >= TRACE_GRADE && g_sn >= TRACE_GRADE {
                        Self::push_occurrence(
                            &mut out,
                            "Alluvial Placer Gold & Cassiterite (Tin) Gravels",
                            g_au.max(g_sn),
                            placer_thick,
                            false,
                            false,
                            vec![
                                (MineralKind::Gold, g_au),
                                (MineralKind::Tin, g_sn),
                                (MineralKind::Quartz, g_qtz),
                            ],
                        );
                    } else if g_au >= TRACE_GRADE {
                        Self::push_occurrence(
                            &mut out,
                            "Alluvial Placer Gold Gravels",
                            g_au,
                            placer_thick,
                            false,
                            false,
                            vec![(MineralKind::Gold, g_au), (MineralKind::Quartz, g_qtz)],
                        );
                    } else if g_sn >= TRACE_GRADE {
                        Self::push_occurrence(
                            &mut out,
                            "Alluvial Cassiterite (Tin) Stream Gravels",
                            g_sn,
                            placer_thick,
                            false,
                            false,
                            vec![(MineralKind::Tin, g_sn), (MineralKind::Quartz, g_qtz)],
                        );
                    } else if clast <= 1.5 && q >= 0.14 && !matches!(r.name, "limestone" | "chalk") {
                        let mut mins = vec![(MineralKind::Quartz, g_qtz)];
                        if q >= 0.24 {
                            mins.push((MineralKind::Flint, ((q - 0.20) / 0.35).clamp(0.10, 1.0)));
                        }
                        Self::push_occurrence(
                            &mut out,
                            "Quartzose Gravel & Chert Pebbles",
                            g_qtz,
                            (thick * 0.35).min(thick).max(0.01),
                            true,
                            false,
                            mins,
                        );
                    } else if q >= 0.18 && !matches!(r.name, "limestone" | "chalk") {
                        let g_f = ((q - 0.14) / 0.36).clamp(0.10, 1.0);
                        Self::push_occurrence(
                            &mut out,
                            "Silica Chert / Jasperoid Beds",
                            g_f,
                            (thick * 0.35).min(thick).max(0.01),
                            true,
                            false,
                            vec![(MineralKind::Flint, g_f), (MineralKind::Quartz, g_qtz)],
                        );
                    }
                }
                RockKind::Igneous => {
                    let g_qtz = ((q - 0.06) / 0.42).clamp(0.08, 1.0);
                    if r.name == "granite" && q >= 0.20 && r.felsity >= 0.65 {
                        let f_mult = ((r.felsity - 0.63) / 0.20).clamp(0.45, 1.45);
                        let g_sn = ((q - 0.18) * 1.9 * f_mult).clamp(0.0, 1.0);
                        let g_w = ((q - 0.22) * 1.8 * f_mult).clamp(0.0, 1.0);
                        if g_sn >= TRACE_GRADE || g_w >= TRACE_GRADE {
                            Self::push_occurrence(
                                &mut out,
                                "Pegmatite & Greisen Quartz-Tin-Tungsten Veins",
                                g_sn.max(g_w),
                                vein_thick,
                                false,
                                false,
                                vec![
                                    (MineralKind::Tin, g_sn),
                                    (MineralKind::Tungsten, g_w),
                                    (MineralKind::Quartz, g_qtz),
                                ],
                            );
                        } else {
                            Self::push_occurrence(
                                &mut out,
                                "Granitic Milky Quartz & Pegmatite Veins",
                                g_qtz,
                                vein_thick,
                                false,
                                false,
                                vec![(MineralKind::Quartz, g_qtz)],
                            );
                        }
                    } else if matches!(r.name, "andesite" | "dacite" | "rhyolite") {
                        // Epithermal boiling-zone Au-Ag veins form in shallow, active/high-relief
                        // volcanic edifices (`< 1.8 km` depth, `subduction > 0` or `mountainosity_norm >= 0.25`)
                        // with intense silica-sulfide boiling (`q >= 0.52 && s >= 0.30`). Once an old
                        // volcanic edifice is planed flat by erosion, its shallow epithermal cap is stripped:
                        let active_edifice =
                            self.subduction > 0.0 || self.mountainosity_norm() >= 0.25;
                        let g_au = if active_edifice
                            && depth_from_surface_km < 1.8
                            && q >= 0.52
                            && s >= 0.30
                        {
                            ((q - 0.48) * 2.0 * ((s - 0.26) / 0.18).clamp(0.4, 1.5)).clamp(0.0, 1.0)
                        } else {
                            0.0
                        };
                        let g_ag = (g_au * 0.85).clamp(0.0, 1.0);
                        if g_au >= TRACE_GRADE {
                            Self::push_occurrence(
                                &mut out,
                                "Epithermal Quartz-Gold-Silver Veins",
                                g_au,
                                vein_thick,
                                false,
                                false,
                                vec![
                                    (MineralKind::Gold, g_au),
                                    (MineralKind::Silver, g_ag),
                                    (MineralKind::Quartz, g_qtz),
                                ],
                            );
                        } else {
                            Self::push_occurrence(
                                &mut out,
                                "Volcanic Hydrothermal Quartz Veins",
                                g_qtz,
                                vein_thick,
                                false,
                                false,
                                vec![(MineralKind::Quartz, g_qtz)],
                            );
                        }
                    } else if q >= 0.10 {
                        Self::push_occurrence(
                            &mut out,
                            "Hydrothermal Siliceous Sinter & Chalcedony Veins",
                            g_qtz,
                            vein_thick,
                            false,
                            false,
                            vec![(MineralKind::Quartz, g_qtz)],
                        );
                    }
                }
                RockKind::Metamorphic => {
                    let g_qtz = ((q - 0.06) / 0.42).clamp(0.08, 1.0);
                    let is_felsic_greisen =
                        matches!(r.name, "hornfels" | "granulite") && r.felsity >= 0.66 && q >= 0.22;
                    if is_felsic_greisen {
                        let f_mult = ((r.felsity - 0.63) / 0.20).clamp(0.45, 1.45);
                        let g_sn = ((q - 0.20) * 1.85 * f_mult).clamp(0.0, 1.0);
                        let g_w = ((q - 0.22) * 1.85 * f_mult).clamp(0.0, 1.0);
                        if g_sn >= TRACE_GRADE || g_w >= TRACE_GRADE {
                            Self::push_occurrence(
                                &mut out,
                                "Contact Greisen Quartz-Tin-Tungsten Veins",
                                g_sn.max(g_w),
                                vein_thick,
                                false,
                                false,
                                vec![
                                    (MineralKind::Tin, g_sn),
                                    (MineralKind::Tungsten, g_w),
                                    (MineralKind::Quartz, g_qtz),
                                ],
                            );
                        } else {
                            Self::push_occurrence(
                                &mut out,
                                "Metamorphic Milky Quartz Veins",
                                g_qtz,
                                vein_thick,
                                false,
                                false,
                                vec![(MineralKind::Quartz, g_qtz)],
                            );
                        }
                    } else {
                        // Orogenic lode gold is devolatilized out of high-T `amphibolite`/`granulite`
                        // and focused into regional `greenschist`/`blueschist` greenstone shear zones
                        // where mafic `Fe2+` sulfidation (`felsity <= 0.48 && s >= 0.27`) coincides with
                        // carbonaceous/graphitic shear-zone reduction (`bog >= 0.10`), or carbonaceous
                        // black-slate turbidites (`bog >= 0.24 && s >= 0.20`). Ordinary contact `hornfels`
                        // aureoles and non-carbonaceous schists form Cu-Zn sulfide/barren quartz veins instead.
                        let is_mafic_greenstone_trap =
                            matches!(r.name, "greenschist" | "blueschist")
                                && r.felsity <= 0.48
                                && s >= 0.27
                                && bog >= 0.10;
                        let is_black_slate_trap = matches!(
                            r.name,
                            "pelite" | "greenschist" | "blueschist" | "hornfels"
                        ) && bog >= 0.24
                            && s >= 0.20;
                        let g_au = if q >= 0.58 && (is_mafic_greenstone_trap || is_black_slate_trap) {
                            let trap = ((s - 0.16).max(0.0) * 2.8
                                + bog * 2.5
                                + (0.50 - r.felsity).max(0.0) * 2.0)
                                .clamp(0.0, 1.8);
                            ((q - 0.52) * 2.8 * trap).clamp(0.0, 1.0)
                        } else {
                            0.0
                        };
                        if g_au >= TRACE_GRADE {
                            let vein_name = if matches!(r.name, "hornfels" | "granulite") {
                                "Contact Hydrothermal Quartz-Gold Veins"
                            } else {
                                "Orogenic Quartz-Gold Veins"
                            };
                            Self::push_occurrence(
                                &mut out,
                                vein_name,
                                g_au,
                                vein_thick,
                                false,
                                false,
                                vec![(MineralKind::Gold, g_au), (MineralKind::Quartz, g_qtz)],
                            );
                        } else if q >= 0.10 && !is_bif {
                            Self::push_occurrence(
                                &mut out,
                                "Barren Milky Quartz & Rock-Crystal Veins",
                                g_qtz,
                                vein_thick,
                                false,
                                false,
                                vec![(MineralKind::Quartz, g_qtz)],
                            );
                        }
                    }
                }
            }
        }

        // 6. Sulfide Ore Bodies (requiring enrichment above unaltered ocean-floor baselines `0.08` basalt / `0.12` gabbro):
        if s >= 0.15 && !is_bif {
            let ore_thick = (thick * (0.08 + 0.22 * s)).clamp(0.02, 0.45);
            match r.kind {
                RockKind::Igneous => {
                    if r.name == "basalt" && self.impact_scar >= 0.45 && s >= 0.24 {
                        let g_ni = ((s - 0.12) / 0.32).clamp(0.15, 1.0);
                        let g_pt = ((s - 0.14) / 0.34 * self.impact_scar.max(0.6)).clamp(0.12, 1.0);
                        let g_cu = (g_ni * 0.85).clamp(0.12, 1.0);
                        Self::push_occurrence(
                            &mut out,
                            "Impact-Melt Ni-Cu-PGM Sulfide Sheet",
                            g_ni.max(g_pt),
                            ore_thick,
                            false,
                            false,
                            vec![
                                (MineralKind::Nickel, g_ni),
                                (MineralKind::Platinum, g_pt),
                                (MineralKind::Copper, g_cu),
                            ],
                        );
                    } else if r.name == "basalt" && s >= 0.16 {
                        let g_cu = ((s - 0.13) / 0.30).clamp(0.08, 1.0);
                        let g_zn = (g_cu * 0.80).clamp(0.06, 1.0);
                        let mins = vec![
                            (MineralKind::Copper, g_cu),
                            (MineralKind::LeadZinc, g_zn),
                        ];
                        Self::push_occurrence(
                            &mut out,
                            "Black-Smoker VMS (Cu-Zn Massive Sulfides)",
                            g_cu,
                            ore_thick,
                            false,
                            false,
                            mins,
                        );
                    } else if r.name == "gabbro" && s >= 0.18 {
                        let g_ni = ((s - 0.15) / 0.30).clamp(0.08, 1.0);
                        let g_cu = (g_ni * 0.80).clamp(0.06, 1.0);
                        let mut mins = vec![
                            (MineralKind::Nickel, g_ni),
                            (MineralKind::Copper, g_cu),
                        ];
                        if r.felsity <= 0.44 && s >= 0.24 {
                            let g_pt = ((s - 0.22) / 0.28).clamp(0.06, 1.0);
                            mins.push((MineralKind::Platinum, g_pt));
                        }
                        Self::push_occurrence(
                            &mut out,
                            "Magmatic Cumulate Ni-Cu Sulfides",
                            g_ni,
                            ore_thick,
                            false,
                            false,
                            mins,
                        );
                    } else if matches!(r.name, "andesite" | "dacite") {
                        let g_cu = ((s - 0.12) / 0.34).clamp(0.08, 1.0);
                        let mut mins = vec![(MineralKind::Copper, g_cu)];
                        if s >= 0.30 && q >= 0.24 {
                            mins.push((
                                MineralKind::Silver,
                                ((s - 0.26) * 1.4 * q).clamp(0.06, 0.70),
                            ));
                        }
                        Self::push_occurrence(
                            &mut out,
                            "Porphyry Copper-Molybdenum Stockwork",
                            g_cu,
                            ore_thick,
                            false,
                            false,
                            mins,
                        );
                    } else if matches!(r.name, "granite" | "rhyolite") {
                        let g_pbzn = ((s - 0.13) / 0.32).clamp(0.08, 1.0);
                        let g_ag = if s >= 0.20 {
                            ((s - 0.18) * 2.3 * (0.5 + q)).clamp(0.06, 1.0)
                        } else {
                            0.0
                        };
                        let mut mins = vec![
                            (MineralKind::LeadZinc, g_pbzn),
                            (MineralKind::Copper, g_pbzn * 0.75),
                        ];
                        if g_ag >= TRACE_GRADE {
                            mins.push((MineralKind::Silver, g_ag));
                        }
                        Self::push_occurrence(
                            &mut out,
                            "Polymetallic Ag-Pb-Zn-Cu Sulfide Veins",
                            g_pbzn.max(g_ag),
                            ore_thick,
                            false,
                            false,
                            mins,
                        );
                    }
                }
                RockKind::Metamorphic => {
                    if r.name == "hornfels" && (0.82..=0.94).contains(&r.toughness) {
                        let g_ni = ((s - 0.11) / 0.30).clamp(0.12, 1.0);
                        let g_pt = ((s - 0.13) / 0.32).clamp(0.10, 1.0);
                        Self::push_occurrence(
                            &mut out,
                            "Shocked Sub-Crater Ni-Cu-PGM Sulfide Breccia",
                            g_ni,
                            ore_thick,
                            false,
                            false,
                            vec![
                                (MineralKind::Nickel, g_ni),
                                (MineralKind::Platinum, g_pt),
                                (MineralKind::Copper, g_ni * 0.85),
                            ],
                        );
                    } else if r.name == "marble" || calc > 0.35 {
                        let g_cu = ((s - 0.12) / 0.32).clamp(0.08, 1.0);
                        let g_ag = if s >= 0.22 {
                            ((s - 0.20) * 2.1).clamp(0.06, 1.0)
                        } else {
                            0.0
                        };
                        let mut mins = vec![
                            (MineralKind::Copper, g_cu),
                            (MineralKind::LeadZinc, g_cu * 0.85),
                            (MineralKind::Iron, g_cu * 0.80),
                        ];
                        if g_ag >= TRACE_GRADE {
                            mins.push((MineralKind::Silver, g_ag));
                        }
                        Self::push_occurrence(
                            &mut out,
                            "Carbonate-Replacement Skarn (Cu-Fe-Pb-Zn-Ag)",
                            g_cu.max(g_ag),
                            ore_thick,
                            false,
                            false,
                            mins,
                        );
                    } else if s >= 0.17 {
                        let g_cu = ((s - 0.14) / 0.34).clamp(0.08, 1.0);
                        let g_ag = if s >= 0.24 && q >= 0.16 {
                            ((s - 0.22) * 2.2 * (0.5 + q)).clamp(0.06, 1.0)
                        } else {
                            0.0
                        };
                        let mut mins = vec![
                            (MineralKind::Copper, g_cu),
                            (MineralKind::LeadZinc, g_cu * 0.90),
                        ];
                        if g_ag >= TRACE_GRADE {
                            mins.push((MineralKind::Silver, g_ag));
                        }
                        let name = if matches!(r.name, "hornfels" | "granulite") {
                            "Contact Porphyry Sulfide Halo (Cu-Zn)"
                        } else {
                            "Metamorphic Polymetallic Sulfide Lodes (Cu-Zn-Pb)"
                        };
                        Self::push_occurrence(&mut out, name, g_cu.max(g_ag), ore_thick, false, false, mins);
                    }
                }
                RockKind::Sedimentary => {
                    if s >= 0.17 {
                        let g_cu = ((s - 0.14) / 0.32).clamp(0.08, 1.0);
                        let mut mins = vec![
                            (MineralKind::Copper, g_cu),
                            (MineralKind::LeadZinc, g_cu * 0.90),
                        ];
                        if s >= 0.26 && bog >= 0.10 {
                            mins.push((
                                MineralKind::Silver,
                                ((s - 0.24) * 2.2 * (0.6 + bog)).clamp(0.06, 1.0),
                            ));
                        }
                        Self::push_occurrence(
                            &mut out,
                            "Stratiform Sediment-Hosted Cu-Pb-Zn Sulfides",
                            g_cu,
                            ore_thick,
                            false,
                            false,
                            mins,
                        );
                    }
                }
            }
        }

        // 7. Precious & Semi-Precious Gemstones (continuous grade & pocket/vein thickness):
        let gem_thick = (thick * 0.08).clamp(0.02, 0.16);
        if gems.ruby >= TRACE_GRADE {
            Self::push_occurrence(
                &mut out,
                "Corundum Marble (Ruby & Spinel)",
                gems.ruby,
                gem_thick,
                false,
                false,
                vec![(MineralKind::Ruby, gems.ruby)],
            );
        }
        if gems.sapphire >= TRACE_GRADE {
            Self::push_occurrence(
                &mut out,
                "Metamorphic Corundum Zone (Blue Sapphire)",
                gems.sapphire,
                gem_thick,
                false,
                false,
                vec![(MineralKind::Sapphire, gems.sapphire)],
            );
        }
        if gems.emerald >= TRACE_GRADE {
            Self::push_occurrence(
                &mut out,
                "Beryl-Chromium Veins (Precious Emerald)",
                gems.emerald,
                gem_thick,
                false,
                false,
                vec![(MineralKind::Emerald, gems.emerald)],
            );
        }
        if gems.jade >= TRACE_GRADE {
            Self::push_occurrence(
                &mut out,
                "High-Pressure Jadeite Lodes (Jade)",
                gems.jade,
                gem_thick,
                false,
                false,
                vec![(MineralKind::Jade, gems.jade)],
            );
        }
        if gems.lapis >= TRACE_GRADE {
            Self::push_occurrence(
                &mut out,
                "Lazurite-Pyrite Marble (Lapis Lazuli)",
                gems.lapis,
                gem_thick,
                false,
                false,
                vec![(MineralKind::Lapis, gems.lapis)],
            );
        }
        if gems.turquoise >= TRACE_GRADE {
            Self::push_occurrence(
                &mut out,
                "Supergene Arid Copper-Phosphate Veins (Turquoise)",
                gems.turquoise,
                gem_thick.min(0.10),
                false,
                false,
                vec![(MineralKind::Turquoise, gems.turquoise)],
            );
        }
        if gems.opal >= TRACE_GRADE {
            Self::push_occurrence(
                &mut out,
                "Hydrated Silica Seams (Precious Opal)",
                gems.opal,
                gem_thick.min(0.08),
                false,
                false,
                vec![(MineralKind::Opal, gems.opal)],
            );
        }

        out
    }

    /// Deterministically evaluate mineral & gemstone deposits concentrated specifically along the
    /// contact interface between `self.layers[lower_idx]` (below) and `self.layers[lower_idx + 1]`
    /// (above) at `contact_depth_km` from the surface.
    pub fn derive_interface_occurrences(
        &self,
        lower_idx: usize,
        contact_depth_km: f32,
    ) -> Vec<DepositOccurrence> {
        self.derive_interface_occurrences_with_elev(lower_idx, contact_depth_km, self.elevation())
    }

    /// `O(1)` variant of `derive_interface_occurrences` using a precomputed `surface_elev_km`.
    pub fn derive_interface_occurrences_with_elev(
        &self,
        lower_idx: usize,
        contact_depth_km: f32,
        surface_elev_km: f32,
    ) -> Vec<DepositOccurrence> {
        let (Some(low), Some(up)) = (self.layers.get(lower_idx), self.layers.get(lower_idx + 1)) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let min_adj_thick = low.thickness.min(up.thickness).max(0.02);

        // Helper to match an asymmetric pair (either lower->upper or upper->lower):
        let pairs = [(low, up), (up, low)];

        // 1. Igneous Pluton – Carbonate Contact Skarns (W-Sn, Cu-Fe-Ag-Au, Nephrite Jade, Contact Ruby/Lapis):
        for &(ig_l, carb_l) in &pairs {
            let ig = &ig_l.rock;
            let cb = &carb_l.rock;
            let cb_calc = cb.calcity.unwrap_or(0.0);
            let is_pluton = ig.kind == RockKind::Igneous
                && matches!(ig.name, "granite" | "andesite" | "gabbro");
            let is_carb =
                matches!(cb.name, "marble" | "limestone" | "chalk") || cb_calc >= 0.45;
            if is_pluton && is_carb {
                let skarn_km = (0.20 * min_adj_thick).clamp(0.03, 0.35);
                let q_max = ig.quartzity.unwrap_or(0.0).max(cb.quartzity.unwrap_or(0.0));
                let s_max = ig.sulfidity.unwrap_or(0.0).max(cb.sulfidity.unwrap_or(0.0));
                let sal_max = ig.salinity.unwrap_or(0.0).max(cb.salinity.unwrap_or(0.0));
                let calc_eff = cb_calc.max(if cb.name == "marble" { 0.80 } else { 0.55 });

                // 1a. Felsic Scheelite-Cassiterite (W-Sn) Skarn:
                if ig.felsity >= 0.64 {
                    let g_w = ((ig.felsity - 0.60) * 2.2 * calc_eff * (0.45 + q_max + 0.5 * s_max))
                        .clamp(0.0, 1.0);
                    let g_sn = (g_w * 0.85).clamp(0.0, 1.0);
                    if g_w >= TRACE_GRADE {
                        Self::push_occurrence(
                            &mut out,
                            "Scheelite-Cassiterite (Tungsten-Tin) Skarn",
                            g_w,
                            skarn_km,
                            false,
                            true,
                            vec![(MineralKind::Tungsten, g_w), (MineralKind::Tin, g_sn)],
                        );
                    }
                }

                // 1b. Polymetallic Cu-Fe-Ag(-Au) Exoskarn:
                if s_max >= 0.14 {
                    let g_cu = ((s_max - 0.10) * 2.3 * calc_eff).clamp(0.0, 1.0);
                    let g_ag = if s_max >= 0.20 {
                        ((s_max - 0.17) * 2.4 * calc_eff).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let g_au = if s_max >= 0.38 && q_max >= 0.36 {
                        ((s_max - 0.34) * 1.8 * q_max * calc_eff).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let mut mins = vec![
                        (MineralKind::Copper, g_cu),
                        (MineralKind::Iron, (g_cu * 0.90).clamp(0.08, 1.0)),
                    ];
                    if g_ag >= TRACE_GRADE {
                        mins.push((MineralKind::Silver, g_ag));
                    }
                    if g_au >= TRACE_GRADE {
                        mins.push((MineralKind::Gold, g_au));
                    }
                    Self::push_occurrence(
                        &mut out,
                        "Exoskarn Cu-Fe-Ag Reaction Halo",
                        g_cu.max(g_ag).max(g_au),
                        skarn_km,
                        false,
                        true,
                        mins,
                    );
                }

                // 1c. Mafic Gabbro – Dolomitic Marble Nephrite Jade Contact:
                if ig.felsity <= 0.49 && calc_eff >= 0.65 && q_max < 0.22 {
                    let g_jade = ((0.52 - ig.felsity) * 4.2 * calc_eff).clamp(0.0, 1.0);
                    Self::push_occurrence(
                        &mut out,
                        "Calc-Silicate Nephrite Jade Contact",
                        g_jade,
                        (skarn_km * 0.5).clamp(0.02, 0.15),
                        false,
                        true,
                        vec![(MineralKind::Jade, g_jade)],
                    );
                }

                // 1d. Low-Silica Carbonate Contact Ruby / Lapis Skarn:
                if q_max < 0.09 && calc_eff >= 0.72 {
                    if s_max >= 0.14 && sal_max >= 0.06 {
                        let g_lapis = ((s_max - 0.10) * 2.6 * (0.6 + sal_max * 3.0)).clamp(0.0, 1.0);
                        Self::push_occurrence(
                            &mut out,
                            "Contact Lazurite Skarn (Lapis Lazuli)",
                            g_lapis,
                            (skarn_km * 0.45).clamp(0.02, 0.14),
                            false,
                            true,
                            vec![(MineralKind::Lapis, g_lapis)],
                        );
                    } else if s_max < 0.09 && (sal_max >= 0.05 || ig.felsity <= 0.56) {
                        let g_ruby =
                            ((0.10 - q_max) * 4.8 * calc_eff * (0.5 + sal_max * 3.0)).clamp(0.0, 1.0);
                        Self::push_occurrence(
                            &mut out,
                            "Contact Corundum Skarn (Ruby & Spinel)",
                            g_ruby,
                            (skarn_km * 0.45).clamp(0.02, 0.14),
                            false,
                            true,
                            vec![(MineralKind::Ruby, g_ruby)],
                        );
                    }
                }
                break;
            }
        }

        // 2. Felsic Granite/Pegmatite – Mafic/Ultramafic or Black-Pelite Contact (Emerald Reaction Zone):
        for &(fel_l, cr_l) in &pairs {
            let fel = &fel_l.rock;
            let cr = &cr_l.rock;
            let is_be_source = fel.kind == RockKind::Igneous && fel.felsity > 0.63;
            let cr_bog = cr.bogginess.unwrap_or(0.0);
            let is_cr_host = (matches!(
                cr.name,
                "greenschist" | "amphibolite" | "gabbro" | "basalt" | "blueschist"
            ) && cr.felsity <= 0.50)
                || (cr.name == "pelite" && cr_bog >= 0.14);
            if is_be_source && is_cr_host {
                let q_flux = fel
                    .quartzity
                    .unwrap_or(0.06)
                    .max(cr.quartzity.unwrap_or(0.04));
                let cr_factor = if cr.name == "pelite" {
                    (cr_bog * 2.5).clamp(0.4, 1.3)
                } else {
                    ((0.54 - cr.felsity) * 12.0).clamp(0.5, 1.4)
                };
                let g_em = ((fel.felsity - 0.60) * 3.2 * (0.35 + q_flux * 4.5) * cr_factor)
                    .clamp(0.0, 1.0);
                let em_km = (0.12 * min_adj_thick).clamp(0.02, 0.18);
                Self::push_occurrence(
                    &mut out,
                    "Pegmatite-Schist Beryl Reaction Zone (Emerald)",
                    g_em,
                    em_km,
                    false,
                    true,
                    vec![(MineralKind::Emerald, g_em)],
                );
                break;
            }
        }

        // 3. Sharp Redox-Boundary Hydrothermal Traps (Kupferschiefer Cu-Ag & Indicator-Slate Gold Shoots):
        let bog_low = low.rock.bogginess.unwrap_or(0.0);
        let bog_up = up.rock.bogginess.unwrap_or(0.0);
        if low.rock.kind != RockKind::Igneous
            && up.rock.kind != RockKind::Igneous
            && (bog_low - bog_up).abs() >= 0.15
        {
            let (red_l, ox_l) = if bog_low > bog_up {
                (low, up)
            } else {
                (up, low)
            };
            let red_bog = red_l.rock.bogginess.unwrap_or(0.0);
            let ox_sal = ox_l.rock.salinity.unwrap_or(0.0);
            let ox_q = ox_l.rock.quartzity.unwrap_or(0.0);
            let s_max = red_l
                .rock
                .sulfidity
                .unwrap_or(0.0)
                .max(ox_l.rock.sulfidity.unwrap_or(0.0));
            let trap_km = (0.10 * min_adj_thick).clamp(0.02, 0.16);

            // 3a. Kupferschiefer-type oxidized saline redbed -> reducing organic black shale Cu-Ag trap:
            if red_bog >= 0.16 && ox_sal >= 0.14 && s_max >= 0.10 {
                let g_cu = ((red_bog - 0.10) * 2.2 * (ox_sal * 3.2).clamp(0.5, 1.5)).clamp(0.0, 1.0);
                let g_ag = (g_cu * 0.85).clamp(0.0, 1.0);
                Self::push_occurrence(
                    &mut out,
                    "Redox-Boundary Kupferschiefer Cu-Ag Horizon",
                    g_cu,
                    trap_km,
                    false,
                    true,
                    vec![(MineralKind::Copper, g_cu), (MineralKind::Silver, g_ag)],
                );
            }

            // 3b. Indicator-slate redox-trap bonanza gold shoot (metamorphic carbonaceous slate at burial depth, not cold surface peat):
            if red_l.rock.kind == RockKind::Metamorphic
                && contact_depth_km >= 0.18
                && red_bog >= 0.28
                && ox_q >= 0.48
                && s_max >= 0.20
            {
                let g_au =
                    ((ox_q - 0.44) * 2.2 * (red_bog * 2.4).clamp(0.5, 1.6)).clamp(0.0, 1.0);
                Self::push_occurrence(
                    &mut out,
                    "Carbonaceous Redox-Trap Bonanza Gold Shoot",
                    g_au,
                    (trap_km * 0.75).clamp(0.02, 0.12),
                    false,
                    true,
                    vec![(MineralKind::Gold, g_au)],
                );
            }
        }

        // 4. Basal Unconformity Bedrock-Riffle Heavy-Mineral Placer Lag:
        // Coarse terrestrial clastic bed (`up`) resting directly on hard crystalline basement (`low`).
        if up.rock.kind == RockKind::Sedimentary
            && matches!(low.rock.kind, RockKind::Metamorphic | RockKind::Igneous)
            && low.rock.toughness >= 0.55
            && surface_elev_km > -0.1
            && up.rock.clasticity.unwrap_or(2.0) <= 1.15
            && up.rock.calcity.unwrap_or(0.0) < 0.30
            && contact_depth_km <= 3.0
        {
            let base_occ = self.derive_layer_occurrences_with_elev(
                lower_idx,
                contact_depth_km,
                surface_elev_km,
            );
            let mut heavy_mins: BTreeMap<MineralKind, f32> = BTreeMap::new();
            for occ in &base_occ {
                for &(mk, mg) in &occ.minerals {
                    if matches!(
                        mk,
                        MineralKind::Gold
                            | MineralKind::Tin
                            | MineralKind::Platinum
                            | MineralKind::Diamond
                            | MineralKind::Ruby
                            | MineralKind::Sapphire
                    ) {
                        let entry = heavy_mins.entry(mk).or_insert(0.0);
                        *entry = entry.max((mg * 1.25).clamp(0.0, 1.0));
                    }
                }
            }
            // Also concentrate basal pay-streak if the overlying gravel itself hosts alluvial placer gold/tin:
            for occ in self.derive_layer_occurrences_with_elev(
                lower_idx + 1,
                (contact_depth_km - up.thickness).max(0.0),
                surface_elev_km,
            ) {
                for &(mk, mg) in &occ.minerals {
                    if matches!(mk, MineralKind::Gold | MineralKind::Tin) {
                        let e = heavy_mins.entry(mk).or_insert(0.0);
                        *e = e.max((mg * 1.20).clamp(0.0, 1.0));
                    }
                }
            }
            if !heavy_mins.is_empty() {
                let peak_g = heavy_mins.values().copied().fold(0.0f32, f32::max);
                let lag_km = (0.08 * up.thickness).clamp(0.02, 0.12);
                let mins_vec: Vec<(MineralKind, f32)> = heavy_mins.into_iter().collect();
                let desc = if mins_vec.iter().any(|&(m, _)| m == MineralKind::Diamond) {
                    "Unconformity Basal Diamond & Heavy-Mineral Lag"
                } else if mins_vec
                    .iter()
                    .any(|&(m, _)| matches!(m, MineralKind::Ruby | MineralKind::Sapphire))
                {
                    "Unconformity Corundum Gem Gravel Pay-Streak"
                } else if mins_vec.iter().any(|&(m, _)| m == MineralKind::Gold) {
                    "Unconformity Bedrock-Riffle Placer Gold Pay-Streak"
                } else {
                    "Unconformity Heavy-Mineral Placer Pay-Streak"
                };
                Self::push_occurrence(&mut out, desc, peak_g, lag_km, false, true, mins_vec);
            }
        }

        out
    }

    /// Deterministically derive human-readable mineral deposit, coal seam, ore vein, and
    /// structural feature descriptions hosted inside `self.layers[layer_idx]`.
    pub fn derive_layer_deposits(&self, layer_idx: usize, depth_from_surface_km: f32) -> Vec<String> {
        self.derive_layer_deposits_with_elev(layer_idx, depth_from_surface_km, self.elevation())
    }

    /// `O(1)` variant of `derive_layer_deposits` using a precomputed `surface_elev_km`.
    pub fn derive_layer_deposits_with_elev(
        &self,
        layer_idx: usize,
        depth_from_surface_km: f32,
        surface_elev_km: f32,
    ) -> Vec<String> {
        let occs =
            self.derive_layer_occurrences_with_elev(layer_idx, depth_from_surface_km, surface_elev_km);
        self.layer_deposits_from_occurrences(
            layer_idx,
            depth_from_surface_km,
            surface_elev_km,
            &occs,
        )
    }

    fn layer_deposits_from_occurrences(
        &self,
        layer_idx: usize,
        depth_from_surface_km: f32,
        surface_elev_km: f32,
        occs: &[DepositOccurrence],
    ) -> Vec<String> {
        let Some(l) = self.layers.get(layer_idx) else {
            return Vec::new();
        };
        let r = &l.rock;
        let mut deposits: Vec<String> = occs.iter().map(|occ| occ.label.clone()).collect();

        // Barren ultramafic kimberlite pipe (mantle peridot/xenoliths without gem diamonds):
        if r.kind == RockKind::Igneous
            && r.felsity <= 0.35
            && r.bogginess.unwrap_or(0.0) < 0.04
        {
            deposits.push("Ultramafic Kimberlite Pipe (Peridot & Mantle Xenoliths)".to_string());
        }

        // Near-surface karst caverns in high-calcity carbonate rocks:
        if r.calcity.unwrap_or(0.0) >= 0.72
            && depth_from_surface_km < 2.5
            && surface_elev_km > 0.0
        {
            deposits.push("Karst Limestone Caverns & Aquifer".to_string());
        }

        deposits
    }

    /// Aggregate `(max_grade, total_accessible_abundance)` for every `MineralKind` across both
    /// rock layers and layer-to-layer contact interfaces within `0.0..max_depth_km` of the surface.
    pub fn accessible_mineral_abundances(&self, max_depth_km: f32) -> BTreeMap<MineralKind, (f32, f32)> {
        self.accessible_mineral_abundances_with_elev(max_depth_km, self.elevation())
    }

    /// `O(L_top)` variant of `accessible_mineral_abundances` using a precomputed `surface_elev_km`.
    pub fn accessible_mineral_abundances_with_elev(
        &self,
        max_depth_km: f32,
        surface_elev_km: f32,
    ) -> BTreeMap<MineralKind, (f32, f32)> {
        let mut map: BTreeMap<MineralKind, (f32, f32)> = BTreeMap::new();
        if max_depth_km <= 0.0 || self.layers.is_empty() {
            return map;
        }

        let mut depth_top = 0.0f32;
        for (idx, layer) in self.layers.iter().enumerate().rev() {
            if depth_top >= max_depth_km {
                break;
            }
            let accessible_frac =
                ((max_depth_km - depth_top).min(layer.thickness) / layer.thickness.max(1e-5))
                    .clamp(0.0, 1.0);
            for occ in self.derive_layer_occurrences_with_elev(idx, depth_top, surface_elev_km) {
                let eff_thick = occ.thickness_km * accessible_frac;
                for (mk, mg) in occ.minerals {
                    let entry = map.entry(mk).or_insert((0.0, 0.0));
                    entry.0 = entry.0.max(mg);
                    entry.1 += mg * eff_thick;
                }
            }

            let depth_bot = depth_top + layer.thickness;
            // Check contact interface at the base of this layer (`idx - 1` below, `idx` above):
            if idx > 0 && depth_bot <= max_depth_km + 0.05 {
                for occ in
                    self.derive_interface_occurrences_with_elev(idx - 1, depth_bot, surface_elev_km)
                {
                    for (mk, mg) in occ.minerals {
                        let entry = map.entry(mk).or_insert((0.0, 0.0));
                        entry.0 = entry.0.max(mg);
                        entry.1 += mg * occ.thickness_km;
                    }
                }
            }
            depth_top = depth_bot;
        }

        map
    }

    /// Serialize this tile's 1D stratigraphic column (ordered top-to-bottom from surface to Moho),
    /// its quantitative layer occurrences, and its layer-to-layer contact interfaces into JSON
    /// for the Side View cross-section canvas.
    pub fn to_stratigraphy_json(&self, tile_idx: usize, surface_elev_override: Option<f32>) -> String {
        fn opt_f32_json(v: Option<f32>) -> String {
            match v {
                Some(x) if x.is_finite() => format!("{x:.4}"),
                _ => "null".to_string(),
            }
        }

        fn occ_to_json(occ: &DepositOccurrence) -> String {
            let mins_json: Vec<String> = occ
                .minerals
                .iter()
                .map(|(mk, mg)| format!("[\"{}\",{:.4}]", mk.code(), mg))
                .collect();
            let esc = occ.label.replace('\\', "\\\\").replace('"', "\\\"");
            format!(
                "{{\"label\":\"{}\",\"grade\":{:.4},\"thicknessKm\":{:.4},\"abundance\":{:.4},\"isInterface\":{},\"minerals\":[{}]}}",
                esc,
                occ.grade,
                occ.thickness_km,
                occ.abundance,
                if occ.is_interface { "true" } else { "false" },
                mins_json.join(",")
            )
        }

        let total_thick = self.thickness();
        let surface_elev = surface_elev_override.unwrap_or_else(|| self.elevation());
        let bottom_km = surface_elev - total_thick;
        let impact_sig = self.impact_signature();

        let mut layers_json = Vec::with_capacity(self.layers.len());
        let mut interfaces_json = Vec::new();
        let mut cur_top = surface_elev;
        let mut depth_from_surface = 0.0f32;

        // Iterate from surface (top layer, `top_down_idx = 0`) down to Moho (bottom layer):
        for (top_down_idx, (idx, layer)) in self.layers.iter().enumerate().rev().enumerate() {
            let cur_bot = cur_top - layer.thickness;
            let occs =
                self.derive_layer_occurrences_with_elev(idx, depth_from_surface, surface_elev);
            let occs_json: Vec<String> = occs.iter().map(occ_to_json).collect();
            let deposits = self.layer_deposits_from_occurrences(
                idx,
                depth_from_surface,
                surface_elev,
                &occs,
            );
            let deps_json: Vec<String> = deposits
                .iter()
                .map(|d| format!("\"{}\"", d.replace('\\', "\\\\").replace('"', "\\\"")))
                .collect();
            let r = &layer.rock;
            layers_json.push(format!(
                "{{\"name\":\"{}\",\"kind\":\"{}\",\"thicknessKm\":{:.4},\"topKm\":{:.4},\"bottomKm\":{:.4},\"depthTopKm\":{:.4},\"depthBottomKm\":{:.4},\"felsity\":{:.4},\"toughness\":{:.4},\"clasticity\":{},\"calcity\":{},\"bogginess\":{},\"quartzity\":{},\"sulfidity\":{},\"salinity\":{},\"deposits\":[{}],\"occurrences\":[{}]}}",
                r.name,
                r.kind.code(),
                layer.thickness,
                cur_top,
                cur_bot,
                depth_from_surface,
                depth_from_surface + layer.thickness,
                r.felsity,
                r.toughness,
                opt_f32_json(r.clasticity),
                opt_f32_json(r.calcity),
                opt_f32_json(r.bogginess),
                opt_f32_json(r.quartzity),
                opt_f32_json(r.sulfidity),
                opt_f32_json(r.salinity),
                deps_json.join(","),
                occs_json.join(",")
            ));

            let contact_depth = depth_from_surface + layer.thickness;
            if idx > 0 {
                let intf_occs = self.derive_interface_occurrences_with_elev(
                    idx - 1,
                    contact_depth,
                    surface_elev,
                );
                if !intf_occs.is_empty() {
                    let zone_km = intf_occs
                        .iter()
                        .map(|o| o.thickness_km)
                        .fold(0.02f32, f32::max);
                    let peak_grade = intf_occs.iter().map(|o| o.grade).fold(0.0f32, f32::max);
                    let intf_occs_json: Vec<String> = intf_occs.iter().map(occ_to_json).collect();
                    let intf_labels_json: Vec<String> = intf_occs
                        .iter()
                        .map(|o| format!("\"{}\"", o.label.replace('\\', "\\\\").replace('"', "\\\"")))
                        .collect();
                    interfaces_json.push(format!(
                        "{{\"upperLayerIdx\":{},\"lowerLayerIdx\":{},\"elevationKm\":{:.4},\"depthKm\":{:.4},\"thicknessKm\":{:.4},\"peakGrade\":{:.4},\"deposits\":[{}],\"occurrences\":[{}]}}",
                        top_down_idx,
                        top_down_idx + 1,
                        cur_bot,
                        contact_depth,
                        zone_km,
                        peak_grade,
                        intf_labels_json.join(","),
                        intf_occs_json.join(",")
                    ));
                }
            }

            cur_top = cur_bot;
            depth_from_surface = contact_depth;
        }

        let bedrock_soil_fertility = self.surface_soil_fertility();
        let effective_soil_fertility = self.effective_soil_fertility();
        format!(
            "{{\"hasColumn\":true,\"tileIdx\":{},\"bottomKm\":{:.4},\"elevationKm\":{:.4},\"thicknessKm\":{:.4},\"mountainosity\":{:.4},\"mountainosityNorm\":{:.4},\"subduction\":{:.4},\"impactScar\":{:.4},\"soilDepthM\":{:.4},\"soilFertility\":{:.4},\"bedrockSoilFertility\":{:.4},\"effectiveSoilFertility\":{:.4},\"layers\":[{}],\"interfaces\":[{}]}}",
            tile_idx,
            bottom_km,
            surface_elev,
            total_thick,
            self.mountainosity,
            self.mountainosity_norm(),
            self.subduction,
            impact_sig,
            self.soil_depth_m,
            self.soil_fertility,
            bedrock_soil_fertility,
            effective_soil_fertility,
            layers_json.join(","),
            interfaces_json.join(",")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_layers() {
        let sources = vec![
            (-15.0, 0.0, vec![('A', 10.0), ('B', 18.0), ('C', 10.0)]),
            (
                -35.0,
                0.0,
                vec![('A', 9.0), ('D', 27.0), ('B', 7.0), ('E', 10.0), ('C', 12.0)],
            ),
            (
                -18.0,
                0.0,
                vec![('A', 7.0), ('E', 4.0), ('B', 10.0), ('C', 17.0)],
            ),
        ];

        let (bottom, _, layers) = TileGeology::merge_layers(&sources);
        assert_eq!(bottom, -35.0);
        assert_eq!(
            layers,
            vec![
                ('A', 9.0),
                ('D', 11.0),
                ('A', 4.0),
                ('D', 4.0),
                ('A', 2.0),
                ('B', 18.0),
                ('C', 17.0)
            ]
        );
    }

    #[test]
    fn test_merge_layers_voting_and_ties() {
        // Preserve height
        let (_, _, l1) = TileGeology::merge_layers(&[
            (0.0, 0.0, vec![('A', 1.0)]),
            (0.0, 0.0, vec![('A', 2.0)]),
        ]);
        assert_eq!(l1[0].1, 2.0);

        // Preserve depth
        let (b2, _, _) = TileGeology::merge_layers(&[
            (-5.0, 0.0, vec![('A', 10.0)]),
            (-2.0, 0.0, vec![('A', 10.0)]),
        ]);
        assert_eq!(b2, -5.0);

        // Most common wins
        let (_, _, l3) = TileGeology::merge_layers(&[
            (0.0, 0.0, vec![('A', 1.0)]),
            (0.0, 0.0, vec![('B', 1.0)]),
            (0.0, 0.0, vec![('B', 1.0)]),
        ]);
        assert_eq!(l3[0].0, 'B');

        // Tie prefers change from layer above
        let (_, _, l4) = TileGeology::merge_layers(&[
            (0.0, 0.0, vec![('A', 2.0)]),
            (0.0, 0.0, vec![('B', 1.0), ('A', 1.0)]),
        ]);
        assert_eq!(l4[0].0, 'B');
    }

    #[test]
    fn test_kimberlite_eruption_and_precious_gems() {
        // 1. Thick continental column with carbon-bearing eclogite at depth -> diamondiferous kimberlite
        let mut deep_eclogite = Rock {
            kind: RockKind::Metamorphic,
            name: "eclogite",
            felsity: 0.50,
            toughness: 0.85,
            clasticity: None,
            calcity: None,
            bogginess: Some(0.18),
            quartzity: None,
            sulfidity: None,
            salinity: None,
        };
        let mut tile = TileGeology {
            bottom: -44.0,
            mountainosity: 0.5,
            subduction: 0.0,
            intrusion: None,
            impact_scar: 0.0,
            soil_depth_m: 2.0,
            soil_fertility: 1.0,
            layers: vec![
                RockLayer {
                    rock: deep_eclogite.clone(),
                    thickness: 12.0,
                },
                RockLayer {
                    rock: Rock::intrusive(0.72),
                    thickness: 33.0,
                },
            ],
        };
        assert!(tile.erupt_kimberlite(), "Expected diamondiferous kimberlite");
        let top_idx = tile.layers.len() - 1;
        let gems = tile.derive_layer_gems(top_idx, 0.0);
        assert!(
            LayerGems::is_workable(gems.diamond),
            "Surface kimberlite pipe should host workable diamonds, got {}",
            gems.diamond
        );
        let deps = tile.derive_layer_deposits(top_idx, 0.0).join(" ");
        assert!(deps.contains("Diamondiferous Kimberlite Pipe"));

        // Verify protobuf snapshot roundtrip preserves kimberlite diamond grade
        let proto = tile.to_proto_geological_tile();
        let mut norm = tile.clone();
        norm.normalize_for_snapshot();
        let restored = TileGeology::from_proto_geological_tile(&proto).unwrap();
        assert_eq!(norm.layers, restored.layers);
        assert!(LayerGems::is_workable(
            restored.derive_layer_gems(restored.layers.len() - 1, 0.0).diamond
        ));

        // 2. Barren craton without deep carbon -> barren kimberlite (peridot/xenoliths, no diamonds)
        deep_eclogite.bogginess = None;
        let mut barren_tile = TileGeology {
            bottom: -35.0,
            mountainosity: 0.2,
            subduction: 0.0,
            intrusion: None,
            impact_scar: 0.0,
            soil_depth_m: 2.0,
            soil_fertility: 1.0,
            layers: vec![
                RockLayer {
                    rock: deep_eclogite,
                    thickness: 10.0,
                },
                RockLayer {
                    rock: Rock::intrusive(0.70),
                    thickness: 26.0,
                },
            ],
        };
        assert!(!barren_tile.erupt_kimberlite(), "Expected barren kimberlite without deep carbon");
        let b_top = barren_tile.layers.len() - 1;
        assert!(!LayerGems::is_present(barren_tile.derive_layer_gems(b_top, 0.0).diamond));
        assert!(barren_tile.derive_layer_deposits(b_top, 0.0).join(" ").contains("Ultramafic Kimberlite Pipe"));

        // 3. Verify Ruby, Emerald, Jade, and Interface Contact Deposits
        let gem_tile = TileGeology {
            bottom: -8.0,
            mountainosity: 1.0,
            subduction: 0.0,
            intrusion: None,
            impact_scar: 0.0,
            soil_depth_m: 2.0,
            soil_fertility: 1.0,
            layers: vec![
                RockLayer {
                    rock: Rock::intrusive(0.75), // adjacent felsic granite pegmatite
                    thickness: 3.0,
                },
                RockLayer {
                    rock: Rock {
                        kind: RockKind::Metamorphic,
                        name: "greenschist",
                        felsity: 0.48,
                        toughness: 0.75,
                        clasticity: None,
                        calcity: None,
                        bogginess: None,
                        quartzity: Some(0.12),
                        sulfidity: None,
                        salinity: None,
                    },
                    thickness: 2.0,
                },
                RockLayer {
                    rock: Rock {
                        kind: RockKind::Metamorphic,
                        name: "blueschist",
                        felsity: 0.50,
                        toughness: 0.75,
                        clasticity: None,
                        calcity: None,
                        bogginess: None,
                        quartzity: Some(0.08),
                        sulfidity: None,
                        salinity: None,
                    },
                    thickness: 2.0,
                },
                RockLayer {
                    rock: Rock {
                        kind: RockKind::Metamorphic,
                        name: "marble",
                        felsity: 0.50,
                        toughness: 0.80,
                        clasticity: None,
                        calcity: Some(0.85),
                        bogginess: None,
                        quartzity: Some(0.03),
                        sulfidity: Some(0.02),
                        salinity: Some(0.08),
                    },
                    thickness: 2.0,
                },
            ],
        };
        assert!(LayerGems::is_workable(gem_tile.derive_layer_gems(1, 4.0).emerald));
        assert!(LayerGems::is_workable(gem_tile.derive_layer_gems(2, 2.0).jade));
        assert!(LayerGems::is_workable(gem_tile.derive_layer_gems(3, 0.0).ruby));

        // Pegmatite (layer 0) <-> Greenschist (layer 1) contact interface produces Emerald reaction zone:
        let em_intf = gem_tile.derive_interface_occurrences(0, 6.0);
        assert!(
            em_intf
                .iter()
                .any(|o| o.is_interface && o.minerals.iter().any(|&(m, g)| m == MineralKind::Emerald && g >= WORKABLE_GRADE)),
            "Expected workable Emerald reaction zone along granite-greenschist interface, got {:?}",
            em_intf
        );
    }

    #[test]
    fn test_gold_rarity_vs_barren_quartz_and_interfaces() {
        // 1. Eroded pre-biotic BIF chert (`q = 0.324`, `s = 0.11`, `clasticity = 1.0`) and ordinary
        //    low-quartz sandstone (`q = 0.12`) must yield Quartz/Flint pebbles, NOT Alluvial Placer Gold!
        let common_tile = TileGeology {
            bottom: -8.0,
            mountainosity: 0.2,
            subduction: 0.0,
            intrusion: None,
            impact_scar: 0.0,
            soil_depth_m: 2.0,
            soil_fertility: 1.0,
            layers: vec![
                RockLayer {
                    rock: Rock::intrusive(0.65),
                    thickness: 7.5,
                },
                RockLayer {
                    rock: Rock {
                        kind: RockKind::Sedimentary,
                        name: "sandstone",
                        felsity: 0.55,
                        toughness: 0.0,
                        clasticity: Some(1.0),
                        calcity: None,
                        bogginess: None,
                        quartzity: Some(0.324),
                        sulfidity: Some(0.11),
                        salinity: None,
                    },
                    thickness: 1.0,
                },
            ],
        };
        let common_occs = common_tile.derive_layer_occurrences(1, 0.0);
        assert!(
            !common_occs
                .iter()
                .any(|o| o.minerals.iter().any(|&(m, _)| m == MineralKind::Gold)),
            "Eroded BIF chert / moderate quartz sandstone must NOT produce placer gold: {:?}",
            common_occs
        );
        assert!(
            common_occs
                .iter()
                .any(|o| o.minerals.iter().any(|&(m, _)| m == MineralKind::Quartz)),
            "Should still identify Quartzose Gravel: {:?}",
            common_occs
        );

        // 2. Genuine high-grade auriferous lode basement (`greenschist` with `q = 0.52, s = 0.24`)
        //    overlain by proximal fluvial gravel (`q = 0.58, s = 0.14`) produces continuous grades
        //    AND an enriched Unconformity Bedrock-Riffle Placer Gold Pay-Streak at the interface:
        let bonanza_tile = TileGeology {
            bottom: -7.0,
            mountainosity: 6.5,
            subduction: 0.0,
            intrusion: None,
            impact_scar: 0.0,
            soil_depth_m: 2.0,
            soil_fertility: 1.0,
            layers: vec![
                RockLayer {
                    rock: Rock {
                        kind: RockKind::Metamorphic,
                        name: "greenschist",
                        felsity: 0.48,
                        toughness: 0.75,
                        clasticity: None,
                        calcity: None,
                        bogginess: Some(0.10),
                        quartzity: Some(0.66),
                        sulfidity: Some(0.28),
                        salinity: None,
                    },
                    thickness: 8.5,
                },
                RockLayer {
                    rock: Rock {
                        kind: RockKind::Sedimentary,
                        name: "sandstone",
                        felsity: 0.47,
                        toughness: 0.0,
                        clasticity: Some(1.0),
                        calcity: None,
                        bogginess: None,
                        quartzity: Some(0.72),
                        sulfidity: Some(0.18),
                        salinity: None,
                    },
                    thickness: 0.35,
                },
            ],
        };
        let lode_occs = bonanza_tile.derive_layer_occurrences(0, 0.35);
        let placer_occs = bonanza_tile.derive_layer_occurrences(1, 0.0);
        let intf_occs = bonanza_tile.derive_interface_occurrences(0, 0.35);
        assert!(
            lode_occs
                .iter()
                .any(|o| o.minerals.iter().any(|&(m, g)| m == MineralKind::Gold && g >= WORKABLE_GRADE)),
            "Expected workable orogenic lode gold: {:?}",
            lode_occs
        );
        assert!(
            placer_occs
                .iter()
                .any(|o| o.minerals.iter().any(|&(m, g)| m == MineralKind::Gold && g >= WORKABLE_GRADE)),
            "Expected workable proximal alluvial placer gold: {:?}",
            placer_occs
        );
        assert!(
            intf_occs
                .iter()
                .any(|o| o.is_interface && o.minerals.iter().any(|&(m, g)| m == MineralKind::Gold && g >= WORKABLE_GRADE)),
            "Expected unconformity bedrock-riffle placer lag at interface: {:?}",
            intf_occs
        );

        let acc = bonanza_tile.accessible_mineral_abundances(0.5);
        let &(max_au_grade, total_au_abund) = acc.get(&MineralKind::Gold).expect("accessible gold");
        assert!(max_au_grade >= WORKABLE_GRADE);
        assert!(total_au_abund > 0.01);
    }

    #[test]
    fn test_compact_coalesces_micro_and_nearly_identical_layers_while_preserving_events() {
        let mut s1 = Rock {
            kind: RockKind::Sedimentary,
            name: "sandstone",
            felsity: 0.60,
            toughness: 0.0,
            clasticity: Some(1.0),
            calcity: None,
            bogginess: None,
            quartzity: Some(0.200),
            sulfidity: None,
            salinity: None,
        };
        let mut s2 = s1.clone();
        s2.quartzity = Some(0.208); // Within 0.015 tolerance -> coalesces

        let mut kimberlite = Rock::extrusive(0.28);
        kimberlite.bogginess = Some(0.18);

        let mut tile = TileGeology {
            bottom: -10.0,
            mountainosity: 0.0,
            subduction: 0.0,
            intrusion: None,
            impact_scar: 0.0,
            soil_depth_m: 2.0,
            soil_fertility: 1.0,
            layers: vec![
                RockLayer {
                    rock: Rock::intrusive(0.70),
                    thickness: 8.0,
                },
                // Thin fresh sedimentary veneer on granite (different RockKind -> preserved!)
                RockLayer {
                    rock: s1.clone(),
                    thickness: 0.0012,
                },
                RockLayer {
                    rock: s2,
                    thickness: 1.0,
                },
                RockLayer {
                    rock: kimberlite,
                    thickness: 0.18,
                },
            ],
        };
        let orig_thick = tile.thickness();
        tile.compact();
        assert!((tile.thickness() - orig_thick).abs() < 1e-4);
        assert_eq!(tile.layers.len(), 3); // granite + coalesced sandstone + kimberlite pipe
        assert!(tile.layers.last().unwrap().rock.is_special_event_layer());

        // Stress-test 500 alternating sub-millimeter slivers -> bounded to <= MAX_LAYERS
        s1.quartzity = Some(0.10);
        let lim = Rock {
            kind: RockKind::Sedimentary,
            name: "limestone",
            felsity: 0.50,
            toughness: 0.0,
            clasticity: Some(50.0),
            calcity: Some(0.85),
            bogginess: None,
            quartzity: None,
            sulfidity: None,
            salinity: None,
        };
        let mut fragmented = TileGeology {
            bottom: -10.0,
            mountainosity: 0.0,
            subduction: 0.0,
            intrusion: None,
            impact_scar: 0.0,
            soil_depth_m: 2.0,
            soil_fertility: 1.0,
            layers: (0..500)
                .map(|i| RockLayer {
                    rock: if i % 2 == 0 {
                        Rock::extrusive(0.48)
                    } else {
                        lim.clone()
                    },
                    thickness: 0.02,
                })
                .collect(),
        };
        let frag_thick = fragmented.thickness();
        fragmented.compact();
        assert!(fragmented.layers.len() <= TileGeology::MAX_LAYERS);
        assert!((fragmented.thickness() - frag_thick).abs() < 1e-3);
    }

    #[test]
    fn test_surface_soil_fertility_distinguishes_volcanic_andisols_from_atolls_and_cratons() {
        let mut hotspot_tile = TileGeology::birth_seafloor();
        hotspot_tile.layers.push(RockLayer {
            rock: Rock::extrusive(0.44),
            thickness: 2.0,
        });
        let mut arc_tile = TileGeology::birth_seafloor();
        arc_tile.layers.push(RockLayer {
            rock: Rock::intrusive(0.58),
            thickness: 2.5,
        });
        let mut atoll_tile = hotspot_tile.clone();
        atoll_tile.layers.push(RockLayer {
            rock: Rock {
                kind: RockKind::Sedimentary,
                name: "limestone",
                felsity: 0.50,
                toughness: 0.45,
                clasticity: Some(4.0),
                calcity: Some(0.92),
                bogginess: None,
                quartzity: None,
                sulfidity: None,
                salinity: None,
            },
            thickness: 0.25,
        });
        let granite_tile = TileGeology::empty_land(Rock::intrusive(0.75), 0.5);

        assert!(hotspot_tile.surface_soil_fertility() > 1.30);
        assert!(arc_tile.surface_soil_fertility() > 1.20);
        assert!(atoll_tile.surface_soil_fertility() < 0.80);
        assert!(granite_tile.surface_soil_fertility() < 0.90);
    }

    #[test]
    fn test_effective_soil_fertility_and_proto_roundtrip_with_legacy_fallback() {
        let mut tile = TileGeology::empty_land(Rock::intrusive(0.74), 1.0);
        let bedrock = tile.surface_soil_fertility();
        assert!((tile.soil_depth_m - 2.0).abs() < 1e-6);
        assert!((tile.soil_fertility - bedrock).abs() < 1e-6);
        // At 2.0 m normal regolith, depth_factor == 1.00.
        assert!((tile.effective_soil_fertility() - bedrock).abs() < 1e-5);

        // Bare scoured bedrock (0.0 m) drops effective fertility by 0.68x.
        tile.soil_depth_m = 0.0;
        assert!((tile.effective_soil_fertility() - (bedrock * 0.68)).abs() < 1e-5);

        // Deep enriched periglacial loess (10.0 m, fertility 1.35) gets 1.08x depth factor.
        tile.soil_depth_m = 10.0;
        tile.soil_fertility = 1.35;
        assert!((tile.effective_soil_fertility() - (1.35 * 1.08)).abs() < 1e-5);

        // Protobuf round-trip preserves exact soil_depth_m and soil_fertility.
        let proto = tile.to_proto_geological_tile();
        assert_eq!(proto.soil_depth_m, Some(10.0));
        assert_eq!(proto.soil_fertility, Some(1.35));
        let decoded = TileGeology::from_proto_geological_tile(&proto).unwrap();
        assert!((decoded.soil_depth_m - 10.0).abs() < 1e-6);
        assert!((decoded.soil_fertility - 1.35).abs() < 1e-6);

        // Stratigraphy JSON exposes physical surficial soil fields for the Geology Detail Modal.
        let strat_json = tile.to_stratigraphy_json(7, None);
        assert!(strat_json.contains("\"soilDepthM\":10.0000"));
        assert!(strat_json.contains("\"soilFertility\":1.3500"));
        assert!(strat_json.contains("\"bedrockSoilFertility\":"));
        assert!(strat_json.contains("\"effectiveSoilFertility\":1.4580"));

        // Legacy snapshots omitting optional soil fields fall back cleanly to 2.0 m (land) / 0.0 m (ocean)
        // and bedrock surface_soil_fertility().
        let mut legacy_land = proto.clone();
        legacy_land.soil_depth_m = None;
        legacy_land.soil_fertility = None;
        let decoded_land = TileGeology::from_proto_geological_tile(&legacy_land).unwrap();
        assert!((decoded_land.soil_depth_m - 2.0).abs() < 1e-6);
        assert!((decoded_land.soil_fertility - decoded_land.surface_soil_fertility()).abs() < 1e-6);

        let ocean = TileGeology::empty_ocean(Rock::seafloor());
        let mut legacy_ocean = ocean.to_proto_geological_tile();
        legacy_ocean.soil_depth_m = None;
        legacy_ocean.soil_fertility = None;
        let decoded_ocean = TileGeology::from_proto_geological_tile(&legacy_ocean).unwrap();
        assert!((decoded_ocean.soil_depth_m - 0.0).abs() < 1e-6);
        assert!(
            (decoded_ocean.soil_fertility - decoded_ocean.surface_soil_fertility()).abs() < 1e-6
        );
    }
}
