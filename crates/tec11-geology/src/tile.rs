use crate::rock::{Rock, RockKind, RockLayer};

#[derive(Clone, Debug)]
pub struct TileGeology {
    pub bottom: f32,
    pub layers: Vec<RockLayer>,
    pub mountainosity: f32,
    pub subduction: f32,
    pub intrusion: Option<(f32, f32)>,
}

impl TileGeology {
    pub const MAX_HEIGHT: f32 = 10.0;
    pub const MIN_THICKNESS: f32 = 4.0;
    pub const MAX_THICKNESS: f32 = 75.0;

    /// Continental column initialized with a base crust of 9 km + surface relief `h`.
    pub fn empty_land(rock: Rock, h: f32) -> Self {
        Self {
            bottom: -9.0,
            layers: vec![RockLayer {
                rock,
                thickness: 9.0 + h,
            }],
            mountainosity: 0.0,
            subduction: 0.0,
            intrusion: None,
        }
    }

    /// Oceanic column initialized with 5 km basaltic crust at abyssal depth (`elevation = -4.0 km`).
    pub fn empty_ocean(rock: Rock) -> Self {
        Self::empty_ocean_with_depth(rock, -4.0)
    }

    /// Oceanic column initialized with 5 km basaltic crust at the specified submarine elevation (`<= 0.0 km`).
    pub fn empty_ocean_with_depth(rock: Rock, elevation_km: f32) -> Self {
        let clamped_elev = elevation_km.min(-0.05);
        Self {
            bottom: clamped_elev - 5.0,
            layers: vec![RockLayer {
                rock,
                thickness: 5.0,
            }],
            mountainosity: 0.0,
            subduction: 0.0,
            intrusion: None,
        }
    }

    /// Newly accreted mid-ocean ridge seafloor (`Section 5.4.2` of design doc):
    /// 3.0 km lower gabbro + 2.0 km upper pillow basalt = 5.0 km total thickness,
    /// thermally buoyant at `bottom = -7.5 km` (`elevation = -2.5 km` ridge crest).
    pub fn birth_seafloor() -> Self {
        Self {
            bottom: -7.5,
            layers: vec![
                RockLayer {
                    rock: Rock::gabbro_seafloor(),
                    thickness: 3.0,
                },
                RockLayer {
                    rock: Rock::seafloor(),
                    thickness: 2.0,
                },
            ],
            mountainosity: 0.0,
            subduction: 0.0,
            intrusion: None,
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

    /// Enforce `MAX_THICKNESS` (`75 km`) and `MAX_HEIGHT` (`10 km`) (`Tile.limit` in `tile.py:62-81`),
    /// without clamping negative ocean bathymetry to `0`.
    pub fn limit(&mut self) {
        let mut shrunk = 0.0f32;
        while self.thickness() > Self::MAX_THICKNESS && !self.layers.is_empty() {
            let dt = self.thickness() - Self::MAX_THICKNESS;
            let l = self.layers.remove(0);
            let dlt = dt.min(l.thickness);
            if dlt < l.thickness {
                self.layers.insert(
                    0,
                    RockLayer {
                        rock: l.rock,
                        thickness: l.thickness - dlt,
                    },
                );
            }
            shrunk += dlt;
        }

        self.bottom += shrunk;

        let de = self.elevation() - Self::MAX_HEIGHT;
        if de > 0.0 {
            self.bottom -= de;
        }

        if self.bottom > 0.0 {
            self.bottom = -self.thickness();
        }
    }

    /// Merge adjacent identical rock layers and remove zero-thickness layers (`Tile.compact` in `tile.py:87-96`).
    pub fn compact(&mut self) {
        self.layers.retain(|l| l.thickness > 1e-6);
        let mut i = 0;
        while i + 1 < self.layers.len() {
            if self.layers[i].rock == self.layers[i + 1].rock {
                let next_t = self.layers.remove(i + 1).thickness;
                self.layers[i].thickness += next_t;
            } else {
                i += 1;
            }
        }
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

        // Compact adjacent identical layers and drop zero-thickness layers
        let mut i = 0;
        while i + 1 < output.len() {
            if output[i].1 <= 1e-6 {
                output.remove(i);
            } else if output[i].0 == output[i + 1].0 {
                let next_t = output.remove(i + 1).1;
                output[i].1 += next_t;
            } else {
                i += 1;
            }
        }

        output.reverse();
        let mountainosity =
            sources.iter().map(|s| s.1).sum::<f32>() / (sources.len().max(1) as f32);

        (bottom, mountainosity, output)
    }

    /// Average each group's contributing tiles with `merge_layers`, then stack colliding groups
    /// (`Tile.mergesources` in `tile.py:185-194`).
    pub fn merge_sources(&mut self, groups: &[Vec<(f32, f32, Vec<RockLayer>)>]) {
        let mut merged_groups = Vec::with_capacity(groups.len());
        for g in groups {
            let n_src = g.len().max(1) as f32;
            let avg_b = g.iter().map(|(b, _, _)| *b).sum::<f32>() / n_src;
            let avg_t = g
                .iter()
                .map(|(_, _, ls)| ls.iter().map(|l| l.thickness).sum::<f32>())
                .sum::<f32>()
                / n_src;
            let converted: Vec<(f32, f32, Vec<(Rock, f32)>)> = g
                .iter()
                .map(|(b, m, ls)| {
                    (
                        *b,
                        *m,
                        ls.iter()
                            .map(|l| (l.rock.clone(), l.thickness))
                            .collect(),
                    )
                })
                .collect();
            let (_, mountainosity, mut layers) = Self::merge_layers(&converted);
            let raw_t: f32 = layers.iter().map(|l| l.1).sum();
            if g.len() > 1 && raw_t > 1e-6 {
                let scale = avg_t / raw_t;
                for l in &mut layers {
                    l.1 *= scale;
                }
            }
            merged_groups.push((avg_b, mountainosity, layers));
        }

        if merged_groups.len() == 1 {
            let (b, m, ls) = merged_groups.into_iter().next().unwrap();
            self.bottom = b;
            self.mountainosity = m;
            self.layers = ls
                .into_iter()
                .map(|(rock, thickness)| RockLayer { rock, thickness })
                .collect();
            self.limit();
            self.compact();
        } else {
            let max_elev = merged_groups
                .iter()
                .map(|(b, _, ls)| *b + ls.iter().map(|l| l.1).sum::<f32>())
                .fold(0.5f32, f32::max);
            let mut target_elev = max_elev;
            for _ in 1..merged_groups.len() {
                let uplift = 2.6 * ((7.2 - target_elev) / 6.4).clamp(0.08, 1.0);
                target_elev = (target_elev + uplift).min(7.8);
            }
            self.mountainosity = merged_groups.iter().map(|g| g.1).sum::<f32>()
                + 4.0 * (merged_groups.len().saturating_sub(1) as f32);
            self.layers = merged_groups
                .into_iter()
                .flat_map(|g| {
                    g.2.into_iter()
                        .map(|(rock, thickness)| RockLayer { rock, thickness })
                })
                .collect();
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

    pub fn clear_temp(&mut self) {
        self.subduction = 0.0;
        self.intrusion = None;
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
}
